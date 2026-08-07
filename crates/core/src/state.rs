#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use bytemuck::{Pod, PodCastError, Zeroable};
// #[cfg(feature = "host")]
// use serde::{Deserialize, Serialize};
pub use sp1_cycle::tracked;

use crate::primitives::ByteInfo;

use crate::{
    advance_ordered_timestamp_run, block_work_from_target, calculate_next_target_required, check_proof_of_work,
    copy_to_bytes, has_ordered_timestamp_window, hash_header, ordered_median_timestamp_slot, ordered_timestamp_run,
    target_from_bits, timestamp_slot, ApplyFailure, BlockHash, BlockTimestamp, BlockWork, ChainWork, Claim,
    CompactTarget, ContinuationData, HeaderInput, ReturnCode, Target, CONTINUATION_DATA_SIZE, EPOCH_LENGTH,
    HALF_WINDOW_SIZE, WINDOW_SIZE,
};

// use rkyv::{Archive, Serialize, Deserialize};

/// Complete authenticated validation state, serialized between recursive iterations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[cfg_attr(
    not(target_os = "zkvm"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct State {
    pub chain_work: ChainWork,
    pub genesis_hash: BlockHash,
    pub block_hash: BlockHash,
    pub height: u32,

    pub current_nbits: CompactTarget,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

impl State {
    /// The circular-buffer slot containing the current tip timestamp.
    #[must_use]
    #[inline]
    fn current_timestamp_slot(&self) -> usize {
        timestamp_slot(self.height.into())
    }

    #[must_use]
    #[inline]
    fn previous_timestamp(&self) -> BlockTimestamp {
        self.timestamps[self.current_timestamp_slot()]
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.ff
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<&Self, PodCastError> {
        bytemuck::try_from_bytes::<Self>(bytes)
    }

    /// Extract the verifier-visible public claim from this state.
    #[must_use]
    pub fn public_claim(&self) -> Claim {
        Claim {
            chain_work: self.chain_work,
            genesis_hash: self.genesis_hash,
            tip_hash: self.block_hash,
            height: self.height,
        }
    }

    /// Serialize the continuation data directly to bytes.
    #[must_use]
    pub fn continuation_bytes(&self) -> [u8; CONTINUATION_DATA_SIZE] {
        ContinuationData::from_state(self).to_bytes()
    }

    /// Compute the difficulty values that become active at a new epoch boundary.
    #[tracked]
    fn prepare_new_epoch(
        &self,
        current_target: Target,
        previous_timestamp: BlockTimestamp,
    ) -> (CompactTarget, Target, BlockWork) {
        calculate_next_target_required(current_target, self.epoch_start_timestamp, previous_timestamp)
    }

    #[allow(clippy::result_large_err)]
    fn apply_header_after_median_check(
        &mut self,
        new_header: &HeaderInput,
        claimed_median: BlockTimestamp,
        active_nbits: CompactTarget,
        active_target: Target,
    ) -> ReturnCode {
        let candidate_height = sp1_cycle::track!(self.height + 1);
        let timestamp_slot = timestamp_slot(candidate_height.into());

        if sp1_cycle::track!(new_header.timestamp <= claimed_median) {
            return ReturnCode::TIMESTAMP_TOO_OLD;
        }

        let block_hash = sp1_cycle::track!({
            hash_header(
                new_header.version,
                &self.block_hash,
                new_header.merkle_root,
                new_header.timestamp,
                active_nbits,
                new_header.nonce,
            )
        });

        if !check_proof_of_work(block_hash, &active_target) {
            return ReturnCode::POW_INSUFFICIENT;
        }

        HeaderUpdate::new(self, candidate_height, timestamp_slot, new_header.timestamp, block_hash);

        ReturnCode::SUCCESS
    }

    #[allow(clippy::result_large_err)]
    #[tracked]
    pub fn apply_headers(
        &mut self,
        headers: &[HeaderInput],
        median_hints: &[BlockTimestamp],
    ) -> Result<(), ApplyFailure> {
        let mut header_iter = headers.iter();
        self.apply_cold_start_headers(&mut header_iter)?;
        self.apply_warm_headers(&mut header_iter, median_hints)?;
        Ok(())
    }

    fn apply_cold_start_headers(
        &mut self,
        header_iter: &mut core::slice::Iter<'_, HeaderInput>,
    ) -> Result<(), ApplyFailure> {
        let cold_len = (WINDOW_SIZE - 1).saturating_sub(self.height as usize).min(header_iter.len());
        let context = EpochManager::new(self);
        for new_header in header_iter.take(cold_len) {
            let claimed_median = context.state.timestamps[(context.state.height as usize).div_ceil(2)];
            let return_code = context.state.apply_header_after_median_check(
                new_header,
                claimed_median,
                context.active_nbits,
                context.active_target,
            );
            if return_code.is_failure() {
                return Err(ApplyFailure {
                    error_code: return_code.error_code().unwrap(),
                });
            }
        }
        Ok(())
    }

    fn apply_warm_headers(
        &mut self,
        header_iter: &mut core::slice::Iter<'_, HeaderInput>,
        median_hints: &[BlockTimestamp],
    ) -> Result<(), ApplyFailure> {
        let mut median_hint_iter = median_hints.iter();
        let mut ordered_run_len =
            sp1_cycle::track!({ ordered_timestamp_run(&self.timestamps, self.height.into()) }) as usize;

        while header_iter.len() > 0 {
            let context = EpochManager::new(self);
            let current_height = context.state.height;
            let headers_to_end_of_epoch = (EPOCH_LENGTH - ((current_height + 1) % EPOCH_LENGTH)) as usize;
            let chunk_len = header_iter.len().min(headers_to_end_of_epoch);

            for new_header in header_iter.take(chunk_len) {
                let claimed_median = if has_ordered_timestamp_window(ordered_run_len as u64) {
                    context.state.timestamps[ordered_median_timestamp_slot(context.state.height.into())]
                } else {
                    let claimed_median = *median_hint_iter
                        .next()
                        .unwrap_or_else(|| panic!("missing median time past hint at header {}", context.state.height));
                    let (less_count, equal_count) = sp1_cycle::track!({
                        // let mut less_count = 0usize;
                        // let mut equal_count = 0usize;
                        let mut counts: [usize; 3] = [0, 0, 0];
                        for &timestamp in &context.state.timestamps {
                            counts[(timestamp.cmp(&claimed_median) as usize) + 1] += 1;
                        }
                        (counts[0], counts[1])
                        // (less_count, equal_count)
                    });
                    assert!(
                        less_count <= HALF_WINDOW_SIZE && less_count + equal_count > HALF_WINDOW_SIZE,
                        "invalid median time past hint at header index {}",
                        context.state.height
                    );
                    claimed_median
                };

                let previous_timestamp = sp1_cycle::track!({ context.state.previous_timestamp() });
                ordered_run_len = advance_ordered_timestamp_run(
                    ordered_run_len as u64,
                    previous_timestamp.into_inner(),
                    new_header.timestamp.into_inner(),
                ) as usize;

                let return_code = context.state.apply_header_after_median_check(
                    new_header,
                    claimed_median,
                    context.active_nbits,
                    context.active_target,
                );
                if return_code.is_failure() {
                    return Err(ApplyFailure {
                        error_code: return_code.error_code().unwrap(),
                    });
                }
            }
        }

        assert_eq!(median_hint_iter.next(), None, "unused sparse median time past hints");

        Ok(())
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            block_hash: BlockHash::default(),
            genesis_hash: BlockHash::default(),
            current_nbits: CompactTarget::default(),
            height: 0,
            chain_work: ChainWork::default(),
            epoch_start_timestamp: BlockTimestamp::default(),
            timestamps: [BlockTimestamp::default(); WINDOW_SIZE],
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
    fn new(state: &'a mut State) -> Self {
        let (active_nbits, active_target, active_work);

        if (state.height + 1).is_multiple_of(EPOCH_LENGTH) {
            let previous_timestamp = sp1_cycle::track!({ state.previous_timestamp() });
            let prepared = sp1_cycle::track!({
                state.prepare_new_epoch(target_from_bits(state.current_nbits), previous_timestamp)
            });
            active_nbits = prepared.0;
            active_target = prepared.1;
            active_work = prepared.2;
        } else {
            active_nbits = state.current_nbits;
            active_target = target_from_bits(active_nbits);
            active_work = block_work_from_target(active_target);
        }

        let start_height = state.height;
        Self {
            state,
            start_height,
            active_block_work: active_work,
            active_nbits,
            active_target,
        }
    }

    fn flush(&mut self) {
        let processed = self.state.height - self.start_height;
        if processed > 0 {
            sp1_cycle::track!({
                let new_chain_work: ChainWork = self.active_block_work * processed;
                self.state.chain_work = self.state.chain_work + new_chain_work;
            });
        }
        self.state.current_nbits = self.active_nbits;
    }
}

impl<'a> Drop for EpochManager<'a> {
    fn drop(&mut self) {
        self.flush();
    }
}

struct HeaderUpdate<'a> {
    state: &'a mut State,
    height: u32,
    timestamp_slot: usize,
    timestamp: BlockTimestamp,
    block_hash: BlockHash,
}

impl<'a> HeaderUpdate<'a> {
    fn new(
        state: &'a mut State,
        height: u32,
        timestamp_slot: usize,
        timestamp: BlockTimestamp,
        block_hash: BlockHash,
    ) -> Self {
        Self {
            state,
            height,
            timestamp_slot,
            timestamp,
            block_hash,
        }
    }
}

impl<'a> Drop for HeaderUpdate<'a> {
    fn drop(&mut self) {
        sp1_cycle::track!({
            self.state.height = self.height;
            self.state.timestamps[self.timestamp_slot] = self.timestamp;
            if self.height.is_multiple_of(EPOCH_LENGTH) {
                self.state.epoch_start_timestamp = self.timestamp;
            }
            self.state.block_hash = self.block_hash;
        });
    }
}
