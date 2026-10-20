#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use zkpow_mtp::{self as mtp, errors::NotMedian, timestamp_count, timestamp_slot, Cold, Warm, WINDOW_SIZE};
use zkpow_params::Consensus;

use crate::{
    block_work_from_target, calculate_next_target_required, check_proof_of_work_words, hash_header_input,
    target_from_bits, ApplyError, BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget, HeaderInput, Target,
};

/// The canonical chain state the prover attests to and each batch of transitions starts
/// from. While a batch is being applied it still describes the last committed header; it
/// reflects the updated chain only after the batch commits.
#[wiretype::wiretype]
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub chain_work:   ChainWork,
    pub genesis_hash: BlockHash,
    pub block_hash:   BlockHash,
    pub height:       u32,

    /// The compact target in force for the current difficulty epoch.
    pub current_nbits:   CompactTarget,
    /// Timestamp of the current epoch's first block; the next retarget measures from here.
    pub epoch_timestamp: BlockTimestamp,
    /// The MTP window, slot `height % WINDOW_SIZE`.
    pub timestamps:      [BlockTimestamp; WINDOW_SIZE],
}

impl State {
    /// Serialize to exactly [`State::SIZE`] bytes.
    // #[must_use]
    // pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    //     *bytemuck::bytes_of::<Self>(self)
    //         .as_array()
    //         .expect("bytes_of(State)'s len should equal State::SIZE")
    // }

    /// Called when the State reflects the final block of epoch N
    /// to compute the difficulty values that are active in epoch N+1.
    fn prepare_next_epoch<C: Consensus>(
        &self,
        current_timestamp: BlockTimestamp,
    ) -> (CompactTarget, Target, BlockWork) {
        let current_target = target_from_bits(self.current_nbits);
        calculate_next_target_required::<C>(current_target, self.epoch_timestamp, current_timestamp)
    }

    fn new_epoch_manager<'a, C: Consensus>(&'_ mut self, current_timestamp: BlockTimestamp) -> EpochManager<'_> {
        let (active_nbits, active_target, active_block_work) =
            if (self.height + 1).is_multiple_of(C::PARAMS.epoch_length) {
                self.prepare_next_epoch::<C>(current_timestamp)
            } else {
                let active_nbits = self.current_nbits;
                let active_target = target_from_bits(active_nbits);
                let active_block_work = block_work_from_target(active_target);
                (active_nbits, active_target, active_block_work)
            };

        let start_height = self.height;
        EpochManager {
            state: self,
            start_height,
            active_block_work,
            active_nbits,
            active_target,
        }
    }

    /// Apply the cold start batch. This is executed exactly once for a chain, applying the first `window_size - 1`
    /// headers, isolating any logic dealing with an incomplete MTP window so that the main path can always assume
    /// it has a full window. It fails if fewer than `window_size - 1` headers are provided. If more headers than
    /// necessary are given, they're passed to on to [`State::apply_headers`] automatically.
    pub fn apply_headers_from_genesis<C: Consensus>(
        &mut self,
        headers: &[HeaderInput],
        median_hints: &[BlockTimestamp],
    ) -> Result<(), ApplyError> {
        // Requires at least `window_size - 1` headers.
        let (cold_headers, warm_headers) = mtp::split_off_cold_headers(headers)?;
        let mtp = self.apply_cold_batch::<C>(cold_headers)?;

        if !warm_headers.is_empty() {
            self.apply_warm_batch::<C>(warm_headers, median_hints, mtp)?
        }

        Ok(())
    }

    // Applies the first headers of the chain, before the MTP window has been filled. This lets the
    // warm path assume it always has a full window.
    fn apply_cold_batch<C: Consensus>(&mut self, headers: &[HeaderInput]) -> Result<mtp::Window<Warm>, ApplyError> {
        let start_height = self.height;
        let mut mtp = self.cold_mtp_ring();
        let mut prev_hash_words = zkpow_sha256::hash_to_state(self.block_hash.as_inner());
        for new_header in headers {
            let median = mtp
                .median()
                .or_else(|| mtp.median_by_sort())
                .expect("the cold batch starts from a genesis timestamp");
            if let Err(e) = self.seal_header::<C>(
                C::PARAMS.compact_target(),
                &C::PARAMS.genesis_target,
                new_header,
                BlockTimestamp::new(median),
                &mut prev_hash_words,
            ) {
                self.block_hash = BlockHash::new(zkpow_sha256::state_to_hash(&prev_hash_words));
                self.store_timestamps(mtp.parts().0);
                self.chain_work += C::PARAMS.genesis_work * (self.height - start_height);
                return Err(e);
            }
            mtp.push(new_header.timestamp.into_inner());
        }
        self.block_hash = BlockHash::new(zkpow_sha256::state_to_hash(&prev_hash_words));
        self.store_timestamps(mtp.parts().0);
        self.chain_work += C::PARAMS.genesis_work * headers.len() as u32;

        Ok(mtp.warm().expect("the cold batch did not fill the MTP window"))
    }

    /// Apply headers while assuming the MTP window is full. The caller is responsible for only
    /// invoking this once the window has been filled. It processes headers in batches of
    /// `epoch_length`, applying them to the current epoch and then transitioning to the next epoch
    /// when a boundary is reached.
    #[inline(always)]
    pub fn apply_headers<C: Consensus>(
        &mut self,
        headers: &[HeaderInput],
        median_hints: &[BlockTimestamp],
    ) -> Result<(), ApplyError> {
        let mtp = self.warm_mtp_ring();
        self.apply_warm_batch::<C>(headers, median_hints, mtp)
    }

    fn apply_warm_batch<C: Consensus>(
        &mut self,
        headers: &[HeaderInput],
        median_hints: &[BlockTimestamp],
        mut mtp: mtp::Window<Warm>,
    ) -> Result<(), ApplyError> {
        let mut header_iter = headers.iter();
        let mut hint_iter = median_hints.iter();

        let mut prev_hash_words = zkpow_sha256::hash_to_state(self.block_hash.as_inner());

        while header_iter.len() > 0 {
            let epoch_start_height = self.height;
            let mut context = self.new_epoch_manager::<C>(BlockTimestamp::new(mtp.newest()));
            let epoch_len_remaining =
                (C::PARAMS.epoch_length - ((epoch_start_height + 1) % C::PARAMS.epoch_length)) as usize;

            for new_header in header_iter.by_ref().take(epoch_len_remaining) {
                let state = &mut context.state;
                let active_nbits = context.active_nbits;
                let active_target = context.active_target;

                let claimed_median = match mtp.median() {
                    Some(median) => BlockTimestamp::new(median),
                    None => {
                        let Some(claimed_median) = hint_iter.next() else {
                            crate::invalid_hint!("median_hint not available for application");
                        };
                        match mtp.is_value_median(claimed_median.into_inner()) {
                            Ok(()) => {},
                            Err(NotMedian::TooNew) => {
                                crate::invalid_hint!(
                                    "claimed_median is newer than half of the known window: {claimed_median}"
                                );
                            },
                            Err(NotMedian::TooOld) => {
                                crate::invalid_hint!(
                                    "claimed_median is older than half of the known window: {claimed_median}"
                                );
                            },
                        }
                        *claimed_median
                    },
                };

                if let Err(e) = state.seal_header::<C>(
                    active_nbits,
                    &active_target,
                    new_header,
                    claimed_median,
                    &mut prev_hash_words,
                ) {
                    state.block_hash = BlockHash::new(zkpow_sha256::state_to_hash(&prev_hash_words));
                    state.store_timestamps(mtp.parts().0);
                    return Err(e);
                }

                mtp.push(new_header.timestamp.into_inner());
            }
        }

        self.block_hash = BlockHash::new(zkpow_sha256::state_to_hash(&prev_hash_words));
        self.store_timestamps(mtp.parts().0);

        if let Some(median_time) = hint_iter.next() {
            crate::invalid_hint!("median_hint left over after application: {}", *median_time);
        };

        Ok(())
    }

    fn store_timestamps(&mut self, values: &[u32; WINDOW_SIZE]) {
        self.timestamps = values.map(BlockTimestamp::new);
    }

    fn cold_mtp_ring(&self) -> mtp::Window {
        let values = self.timestamps.map(BlockTimestamp::into_inner);
        mtp::Window::<Cold>::from_parts(values, timestamp_count(self.height))
            .expect("apply_headers_from_genesis requires a cold MTP window")
    }

    fn warm_mtp_ring(&self) -> mtp::Window<Warm> {
        let values = self.timestamps.map(BlockTimestamp::into_inner);
        let next = timestamp_slot(self.height.wrapping_add(1));
        mtp::Window::<Warm>::from_parts(values, next, timestamp_count(self.height))
            .expect("apply_headers requires a full MTP window")
    }

    fn seal_header<C: Consensus>(
        &mut self,
        active_nbits: CompactTarget,
        active_target: &Target,
        new_header: &HeaderInput,
        known_median: BlockTimestamp,
        hash_words: &mut [u64; 8],
    ) -> Result<(), ApplyError> {
        // Check that the incoming timestamp is > `known_median`
        if new_header.timestamp <= known_median {
            return Err(ApplyError::TimestampTooOld {
                height:    self.height + 1,
                timestamp: new_header.timestamp,
                median:    known_median,
            });
        }

        // Compute the block's hash words and check that it meets the PoW target
        let next_hash_words = hash_header_input(new_header, hash_words, active_nbits);
        if !check_proof_of_work_words(&next_hash_words, active_target) {
            let hash = BlockHash::new(zkpow_sha256::state_to_hash(&next_hash_words));
            return Err(ApplyError::PowInsufficient {
                height: self.height + 1,
                hash,
                nbits: active_nbits,
            });
        }

        // Advance the state to be after this block; infallible
        self.advance_state::<C>(new_header.timestamp);
        *hash_words = next_hash_words;

        Ok(())
    }

    fn advance_state<C: Consensus>(&mut self, timestamp: BlockTimestamp) {
        self.height += 1;
        if self.height.is_multiple_of(C::PARAMS.epoch_length) {
            self.epoch_timestamp = timestamp;
        }
    }
}

struct EpochManager<'a> {
    state:             &'a mut State,
    start_height:      u32,
    active_block_work: BlockWork,
    active_nbits:      CompactTarget,
    active_target:     Target,
}

impl<'a> EpochManager<'a> {
    fn flush(&mut self) {
        let processed = self.state.height - self.start_height;
        // An epoch that took no headers must not commit its prepared nbits: the next manager would
        // then retarget again from the already-retargeted value.
        if processed == 0 {
            return;
        }

        self.state.chain_work += self.active_block_work * processed;

        // TODO: This only needs to happen after re-calculating difficulty
        self.state.current_nbits = self.active_nbits;
    }
}

impl<'a> Drop for EpochManager<'a> {
    fn drop(&mut self) {
        self.flush();
    }
}
