#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use bytemuck::{Pod, Zeroable};
use derive_size::Size;
use sp1_cycle::tracked;
use zkpow_params::{Consensus, WINDOW_SIZE, WINDOW_SMALL_HALF_SIZE};

use crate::{
    advance_ordered_timestamp_run, block_work_from_target, calculate_next_target_required, check_proof_of_work,
    has_ordered_timestamp_window, hash_header_input, ordered_median_timestamp_slot, ordered_timestamp_run,
    target_from_bits, timestamp_slot, ApplyError, BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget,
    HeaderInput, Target,
};

/// Complete authenticated validation state, serialized between recursive iterations.
/// This is the state of the *last* block header processed.
/// When appending blocks, this is the state of the parent block of the block being appended.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Size)]
pub struct State {
    pub chain_work:   ChainWork,
    pub genesis_hash: BlockHash,
    pub block_hash:   BlockHash,
    pub height:       u32,

    pub current_nbits:   CompactTarget,
    pub epoch_timestamp: BlockTimestamp,
    pub timestamps:      [BlockTimestamp; WINDOW_SIZE],
}

impl State {
    #[must_use]
    #[inline]
    fn current_timestamp(&self) -> BlockTimestamp {
        self.timestamps[timestamp_slot(self.height)]
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        *bytemuck::bytes_of::<Self>(self)
            .as_array()
            .expect("bytes_of(State)'s len should equal State::SIZE")
    }

    /// Called when the State reflects the final block of epoch N
    /// to compute the difficulty values that are active in epoch N+1.
    #[tracked]
    fn prepare_next_epoch<C: Consensus>(&self) -> (CompactTarget, Target, BlockWork) {
        let current_target = target_from_bits(self.current_nbits);
        let current_timestamp = self.current_timestamp();
        calculate_next_target_required::<C>(current_target, self.epoch_timestamp, current_timestamp)
    }

    fn new_epoch_manager<'a, C: Consensus>(&'_ mut self) -> EpochManager<'_> {
        let (active_nbits, active_target, active_block_work) =
            if (self.height + 1).is_multiple_of(C::PARAMS.epoch_length) {
                self.prepare_next_epoch::<C>()
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

    /// Apply the cold start batch. This path is intended for the first `window_size - 1` headers
    /// of the chain, starting from genesis. After those headers are applied any remaining headers
    /// are passed to [`State::apply_warm`].
    #[tracked]
    pub fn apply_headers_from_genesis<C: Consensus>(
        &mut self,
        headers: &[HeaderInput],
        median_hints: &[BlockTimestamp],
    ) -> Result<(), ApplyError> {
        // The first run *MUST* contain at least `window_size - 1` headers
        let cold_count = WINDOW_SIZE - 1;
        if headers.len() < cold_count {
            return Err(ApplyError::InsufficientColdHeaders {
                required: cold_count,
                actual:   headers.len(),
            });
        }

        // Invariant: We rejected calls with less than `cold_count` headers and now we split up to
        // cold_count` headers, so we necessarily have exactly `cold_count` cold headers
        let (cold, warm) = headers.split_at(cold_count);

        self.apply_cold_batch::<C>(cold)?;

        if warm.is_empty() {
            Ok(())
        } else {
            self.apply_headers::<C>(warm, median_hints)
        }
    }

    // Applies the first headers of the chain, before the MTP window has been filled. This lets the
    // warm path assume it always has a full window.
    fn apply_cold_batch<C: Consensus>(&mut self, headers: &[HeaderInput]) -> Result<(), ApplyError> {
        for (i, new_header) in headers.iter().enumerate() {
            let slot = i / 2;
            self.seal_header::<C>(
                C::PARAMS.compact_target(),
                &C::PARAMS.genesis_target,
                new_header,
                self.timestamps[slot],
            )?;
        }
        self.chain_work += C::PARAMS.genesis_work * headers.len() as u32;

        Ok(())
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
        let mut header_iter = headers.iter();
        let mut hint_iter = median_hints.iter();

        let mut ordered_run_len = sp1_cycle::track!({ ordered_timestamp_run(&self.timestamps, self.height) }) as usize;

        while header_iter.len() > 0 {
            let epoch_start_height = self.height;
            let mut context = self.new_epoch_manager::<C>();
            let epoch_len_remaining =
                (C::PARAMS.epoch_length - ((epoch_start_height + 1) % C::PARAMS.epoch_length)) as usize;

            for new_header in header_iter.by_ref().take(epoch_len_remaining) {
                let state = &mut context.state;
                let height = state.height;
                let active_nbits = context.active_nbits;
                let active_target = context.active_target;

                let claimed_median = if has_ordered_timestamp_window(ordered_run_len as u64) {
                    state.timestamps[ordered_median_timestamp_slot(height)]
                } else {
                    let Some(claimed_median) = hint_iter.next() else {
                        crate::invalid_hint!("median_hint not available for application");
                    };

                    let (less_count, equal_count) = sp1_cycle::track!({
                        let mut counts: [usize; 3] = [0, 0, 0];
                        for i in 0..WINDOW_SIZE {
                            let timestamp = state.timestamps[timestamp_slot(height.wrapping_sub(i as u32))];
                            counts[((timestamp.cmp(claimed_median) as i8) + 1) as usize] += 1;
                        }
                        (counts[0], counts[1])
                    });
                    let half = WINDOW_SMALL_HALF_SIZE;
                    if less_count > half {
                        crate::invalid_hint!("claimed_median is newer than half of the known window: {claimed_median} (less_count={less_count})");
                    }
                    if less_count + equal_count <= half {
                        crate::invalid_hint!("claimed_median is older than half of the known window: {claimed_median} (less_count={less_count}, equal_count={equal_count})");
                    }
                    if equal_count == 0 {
                        crate::invalid_hint!("claimed_median is not in the known window: {claimed_median}");
                    }
                    *claimed_median
                };

                let current_timestamp = state.current_timestamp();
                ordered_run_len = advance_ordered_timestamp_run(
                    ordered_run_len as u64,
                    current_timestamp.into_inner(),
                    new_header.timestamp.into_inner(),
                ) as usize;

                state.seal_header::<C>(active_nbits, &active_target, new_header, claimed_median)?;
            }
        }

        if let Some(median_time) = hint_iter.next() {
            crate::invalid_hint!("median_hint left over after application: {}", *median_time);
        };

        Ok(())
    }

    fn seal_header<C: Consensus>(
        &mut self,
        active_nbits: CompactTarget,
        active_target: &Target,
        new_header: &HeaderInput,
        known_median: BlockTimestamp,
    ) -> Result<(), ApplyError> {
        // Check that the incoming timestamp is > `known_median`
        if new_header.timestamp <= known_median {
            return Err(ApplyError::TimestampTooOld {
                height:    self.height + 1,
                timestamp: new_header.timestamp,
                median:    known_median,
            });
        }

        // Compute the block's hash and check that it meets the PoW target
        let hash = hash_header_input(new_header, &self.block_hash, active_nbits);
        if !check_proof_of_work(hash, active_target) {
            return Err(ApplyError::PowInsufficient {
                height: self.height + 1,
                hash,
                nbits: active_nbits,
            });
        }

        // Advance the state to be after this block; infallible
        self.advance_state::<C>(new_header.timestamp, hash);

        Ok(())
    }

    #[tracked]
    fn advance_state<C: Consensus>(&mut self, timestamp: BlockTimestamp, hash: BlockHash) {
        self.height += 1;
        self.block_hash = hash;
        self.timestamps[timestamp_slot(self.height)] = timestamp;
        if self.height.is_multiple_of(C::PARAMS.epoch_length) {
            self.epoch_timestamp = timestamp;
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            block_hash:      BlockHash::default(),
            genesis_hash:    BlockHash::default(),
            current_nbits:   CompactTarget::default(),
            height:          0,
            chain_work:      ChainWork::default(),
            epoch_timestamp: BlockTimestamp::default(),
            timestamps:      [BlockTimestamp::default(); WINDOW_SIZE],
        }
    }
}

struct EpochManager<'a> {
    state: &'a mut State,
    start_height: u32,
    active_block_work: BlockWork,
    active_nbits: CompactTarget,
    active_target: Target,
}

impl<'a> EpochManager<'a> {
    fn flush(&mut self) {
        let processed = self.state.height - self.start_height;
        // An epoch that took no headers must not commit its prepared nbits: the next manager would
        // then retarget again from the already-retargeted value.
        if processed == 0 {
            return;
        }

        sp1_cycle::track!({
            self.state.chain_work += self.active_block_work * processed;
        });

        // TODO: This only needs to happen after re-calculating difficulty
        self.state.current_nbits = self.active_nbits;
    }
}

impl<'a> Drop for EpochManager<'a> {
    fn drop(&mut self) {
        self.flush();
    }
}
