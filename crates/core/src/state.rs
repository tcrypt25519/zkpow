#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use bytemuck::{Pod, Zeroable};
use derive_size::Size;
use sp1_cycle::tracked;
use zkpow_params::{Consensus, Mainnet};

use crate::{
    advance_ordered_timestamp_run, block_work_from_target, calculate_next_target_required, check_proof_of_work,
    has_ordered_timestamp_window, hash_header_input, ordered_median_timestamp_slot, ordered_timestamp_run,
    target_from_bits, timestamp_count, timestamp_slot, ApplyError, BlockHash, BlockTimestamp, BlockWork, ChainWork,
    CompactTarget, HeaderInput, Target,
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
    pub timestamps:      [BlockTimestamp; Mainnet::PARAMS.window_size],
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

    fn new_epoch_manager<'a, C: Consensus>(&'a mut self) -> EpochManager<'a> {
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
        let cold_count = C::PARAMS.window_size - 1;
        let to_apply = headers.len().min(cold_count);
        let (cold, warm) = headers.split_at(to_apply);

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
        let context = EpochManager::from_genesis::<C>(self);
        for new_header in headers {
            let claimed_median = context.state.timestamps[(context.state.height as usize).div_ceil(2)];
            let active_nbits = context.active_nbits;
            let active_target = context.active_target;
            context
                .state
                .seal_header::<C>(active_nbits, active_target, new_header, claimed_median)?;
        }
        drop(context);
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
            let height = self.height;
            let mut context = self.new_epoch_manager::<C>();
            let epoch_len_remaining = (C::PARAMS.epoch_length - ((height + 1) % C::PARAMS.epoch_length)) as usize;

            for new_header in header_iter.by_ref().take(epoch_len_remaining) {
                let state = &mut context.state;
                let active_nbits = context.active_nbits;
                let active_target = context.active_target;

                let claimed_median = if has_ordered_timestamp_window(ordered_run_len as u64) {
                    state.timestamps[ordered_median_timestamp_slot(height)]
                } else {
                    let claimed_median = *hint_iter.next().ok_or(ApplyError::InvalidMedianHint)?;

                    let (less_count, equal_count) = sp1_cycle::track!({
                        let mut counts: [usize; 3] = [0, 0, 0];
                        let count = timestamp_count(height);
                        for i in 0..count {
                            let timestamp = state.timestamps[timestamp_slot(height.wrapping_sub(i as u32))];
                            counts[((timestamp.cmp(&claimed_median) as i8) + 1) as usize] += 1;
                        }
                        (counts[0], counts[1])
                    });

                    let half = timestamp_count(height) / 2;
                    if less_count > half || less_count + equal_count <= half {
                        return Err(ApplyError::InvalidMedianHint);
                    }
                    claimed_median
                };

                let current_timestamp = state.current_timestamp();
                ordered_run_len = advance_ordered_timestamp_run(
                    ordered_run_len as u64,
                    current_timestamp.into_inner(),
                    new_header.timestamp.into_inner(),
                ) as usize;

                state.seal_header::<C>(active_nbits, active_target, new_header, claimed_median)?;
            }
        }

        if hint_iter.next().is_some() {
            return Err(ApplyError::InvalidMedianHint);
        }

        Ok(())
    }

    fn seal_header<C: Consensus>(
        &mut self,
        active_nbits: CompactTarget,
        active_target: Target,
        new_header: &HeaderInput,
        known_median: BlockTimestamp,
    ) -> Result<(), ApplyError> {
        if new_header.timestamp <= known_median {
            return Err(ApplyError::TimestampTooOld);
        }

        let hash = hash_header_input(new_header, &self.block_hash, &active_nbits);

        if !check_proof_of_work(hash, &active_target) {
            return Err(ApplyError::PowInsufficient);
        }

        sp1_cycle::track!({
            self.height = self.height + 1;
            self.block_hash = hash;
            self.timestamps[timestamp_slot(self.height)] = new_header.timestamp;
            if (self.height).is_multiple_of(C::PARAMS.epoch_length) {
                self.epoch_timestamp = new_header.timestamp;
            }
        });

        Ok(())
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
            timestamps:      [BlockTimestamp::default(); Mainnet::PARAMS.window_size],
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
    fn from_genesis<C: Consensus>(state: &'a mut State) -> Self {
        Self {
            state,
            start_height: 0,
            active_block_work: C::PARAMS.block_work(),
            active_nbits: C::PARAMS.compact_target(),
            active_target: C::PARAMS.target(),
        }
    }

    fn flush(&mut self) {
        let processed = self.state.height - self.start_height;
        // An epoch that took no headers must not commit its prepared nbits: the next manager would
        // then retarget again from the already-retargeted value.
        if processed == 0 {
            return;
        }

        sp1_cycle::track!({
            let new_chain_work: ChainWork = self.active_block_work * processed;
            self.state.chain_work = self.state.chain_work + new_chain_work;
        });
        self.state.current_nbits = self.active_nbits;
    }
}

impl<'a> Drop for EpochManager<'a> {
    fn drop(&mut self) {
        self.flush();
    }
}
