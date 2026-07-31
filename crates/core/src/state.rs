#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use bytemuck::{Pod, PodCastError, Zeroable};
#[cfg(feature = "host")]
use serde::{Deserialize, Serialize};
pub use sp1_cycle::tracked;

use crate::{
    advance_ordered_timestamp_run, block_work_from_target, calculate_next_target_required, check_proof_of_work,
    copy_to_bytes, has_ordered_timestamp_window, hash_header, ordered_median_timestamp_slot, ordered_timestamp_run,
    target_from_bits, timestamp_slot, ApplyFailure, BlockHash, BlockTimestamp, BlockWork, ChainWork, Claim,
    CompactTarget, ContinuationData, HeaderInput, Target, TargetError, ValidationErrorCode, CONTINUATION_DATA_SIZE,
    EPOCH_LENGTH, STATE_SIZE, WINDOW_SIZE,
};

/// Complete authenticated validation state, serialized between recursive iterations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[cfg_attr(feature = "host", derive(Serialize, Deserialize))]
pub struct State {
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub chain_work: ChainWork,
    pub current_nbits: CompactTarget,
    pub height: u32,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
    pub _padding: [u8; 8],
}

impl State {
    pub const SIZE: usize = size_of::<Self>();
    /// The circular-buffer slot containing the current tip timestamp.
    #[must_use]
    fn current_timestamp_slot(&self) -> usize {
        timestamp_slot(self.height.into())
    }

    #[must_use]
    fn previous_timestamp(&self) -> BlockTimestamp {
        self.timestamps[self.current_timestamp_slot()]
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; STATE_SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<&Self, PodCastError> {
        bytemuck::try_from_bytes::<Self>(bytes)
    }

    /// Deserialize by copy from exactly [`STATE_SIZE`] bytes.
    ///
    /// Unlike [`State::parse`] this has no alignment requirement, so it works
    /// on slices taken at arbitrary offsets of a larger buffer.
    pub fn parse_unaligned(bytes: &[u8]) -> Result<Self, PodCastError> {
        bytemuck::try_pod_read_unaligned(bytes)
    }

    /// Extract the verifier-visible public claim from this state.
    #[must_use]
    pub fn public_claim(&self) -> Claim {
        Claim {
            genesis_hash: self.genesis_hash,
            tip_hash: self.block_hash,
            chain_work: self.chain_work,
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
    ) -> Result<(CompactTarget, Target, BlockWork), TargetError> {
        let (current_nbits, current_target) =
            calculate_next_target_required(current_target, self.epoch_start_timestamp, previous_timestamp);
        let block_work = block_work_from_target(current_target)?;
        Ok((current_nbits, current_target, block_work))
    }

    #[allow(clippy::result_large_err)]
    fn apply_header_after_median_check(
        &mut self,
        new_header: &HeaderInput,
        claimed_median: BlockTimestamp,
        active_nbits: CompactTarget,
        active_target: Target,
    ) -> Result<(), ApplyFailure> {
        let candidate_height = sp1_cycle::track!(self.height + 1);
        let timestamp_slot = timestamp_slot(candidate_height.into());

        if sp1_cycle::track!(new_header.timestamp <= claimed_median) {
            return Err(ApplyFailure {
                last_valid_state: *self,
                error_code: ValidationErrorCode::TimestampTooOld,
                failure_height: candidate_height,
            });
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

        if !check_proof_of_work(block_hash, active_target) {
            return Err(ApplyFailure {
                last_valid_state: *self,
                error_code: ValidationErrorCode::PowInsufficient,
                failure_height: candidate_height,
            });
        }

        sp1_cycle::track!({
            self.height = candidate_height;
            self.timestamps[timestamp_slot] = new_header.timestamp;
            self.current_nbits = active_nbits;
            if candidate_height.is_multiple_of(EPOCH_LENGTH) {
                self.epoch_start_timestamp = new_header.timestamp;
            }
            self.block_hash = block_hash;
        });

        Ok(())
    }

    #[allow(clippy::result_large_err)]
    #[tracked]
    pub fn apply_headers(
        &mut self,
        headers: &[HeaderInput],
        median_hints: &[BlockTimestamp],
    ) -> Result<(), ApplyFailure> {
        let mut headers_consumed = 0;

        if self.height < (WINDOW_SIZE - 1) as u32 {
            let cold_len = (WINDOW_SIZE - 1 - self.height as usize).min(headers.len());
            let mut context = EpochManager::new(self);
            for new_header in &headers[..cold_len] {
                let claimed_median = context.state.timestamps[(context.state.height as usize).div_ceil(2)];
                if let Err(mut failure) = context.state.apply_header_after_median_check(
                    new_header,
                    claimed_median,
                    context.active_nbits,
                    context.active_target,
                ) {
                    context.flush();
                    failure.last_valid_state = *context.state;
                    return Err(failure);
                }
                context.pending_run_count += 1;
            }
            headers_consumed = cold_len;
        }

        let mut median_hint_iter = median_hints.iter();

        if headers_consumed < headers.len() {
            let mut ordered_run_len =
                sp1_cycle::track!({ ordered_timestamp_run(&self.timestamps, self.height.into()) }) as usize;

            while headers_consumed < headers.len() {
                let mut context = EpochManager::new(self);

                let current_height = context.state.height;
                let headers_to_end_of_epoch = (EPOCH_LENGTH - ((current_height + 1) % EPOCH_LENGTH)) as usize;

                let chunk_len = (headers.len() - headers_consumed).min(headers_to_end_of_epoch);
                let chunk_headers = &headers[headers_consumed..headers_consumed + chunk_len];

                for (chunk_index, new_header) in chunk_headers.iter().enumerate() {
                    let header_index = headers_consumed + chunk_index;
                    let claimed_median = if has_ordered_timestamp_window(ordered_run_len as u64) {
                        context.state.timestamps[ordered_median_timestamp_slot(context.state.height.into())]
                    } else {
                        let claimed_median = *median_hint_iter
                            .next()
                            .unwrap_or_else(|| panic!("missing median time past hint at header index {header_index}"));
                        let (less_count, equal_count) = sp1_cycle::track!({
                            let mut less_count = 0usize;
                            let mut equal_count = 0usize;
                            for &timestamp in &context.state.timestamps {
                                if timestamp < claimed_median {
                                    less_count += 1;
                                } else if timestamp == claimed_median {
                                    equal_count += 1;
                                }
                            }
                            (less_count, equal_count)
                        });
                        assert!(
                            less_count <= WINDOW_SIZE / 2 && less_count + equal_count > WINDOW_SIZE / 2,
                            "invalid median time past hint at header index {header_index}"
                        );
                        claimed_median
                    };

                    let previous_timestamp = sp1_cycle::track!({ context.state.previous_timestamp() });
                    ordered_run_len = advance_ordered_timestamp_run(
                        ordered_run_len as u64,
                        previous_timestamp.into_inner(),
                        new_header.timestamp.into_inner(),
                    ) as usize;

                    if let Err(mut failure) = context.state.apply_header_after_median_check(
                        new_header,
                        claimed_median,
                        context.active_nbits,
                        context.active_target,
                    ) {
                        context.flush();
                        failure.last_valid_state = *context.state;
                        return Err(failure);
                    }
                    context.pending_run_count += 1;
                }

                headers_consumed += chunk_len;
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
            _padding: [0; 8],
        }
    }
}

struct EpochManager<'a> {
    state: &'a mut State,
    pending_run_count: u32,
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
            })
            .expect("epoch target must not be u256::MAX — invalid blockchain data");
            active_nbits = prepared.0;
            active_target = prepared.1;
            active_work = prepared.2;
        } else {
            active_nbits = state.current_nbits;
            active_target = target_from_bits(active_nbits);
            active_work = block_work_from_target(active_target).expect("current target must be valid blockchain data");
        }

        Self {
            state,
            active_block_work: active_work,
            pending_run_count: 0,
            active_nbits,
            active_target,
        }
    }

    fn flush(&mut self) {
        if self.pending_run_count > 0 {
            sp1_cycle::track!({
                let new_chain_work: ChainWork = self.active_block_work * self.pending_run_count;
                self.state.chain_work = self.state.chain_work + new_chain_work;
            });
            self.pending_run_count = 0;
        }
    }
}

impl<'a> Drop for EpochManager<'a> {
    fn drop(&mut self) {
        self.flush();
    }
}
