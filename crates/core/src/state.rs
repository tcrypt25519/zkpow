#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use crate::{
    calculate_next_target_required, check_proof_of_work, copy_from_bytes, copy_to_bytes,
    target_from_bits, tracked, work_from_target, ApplyFailure, BlockHash, BlockTimestamp,
    ChainWork, Claim, CompactTarget, ContinuationData, NewHeader, ParseError, Target, TargetError,
    ValidationErrorCode, CONTINUATION_DATA_SIZE, EPOCH_LENGTH, STATE_SIZE, WINDOW_SIZE,
};

#[cfg(all(target_os = "zkvm", feature = "profiling"))]
#[inline(always)]
#[doc(hidden)]
pub fn run_cycle_track<T, F>(label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    sp1_zkvm::io::write(
        1,
        alloc::format!("cycle-tracker-report-start: {label}\n").as_bytes(),
    );
    let output = f();
    sp1_zkvm::io::write(
        1,
        alloc::format!("cycle-tracker-report-end: {label}\n").as_bytes(),
    );
    output
}

#[cfg(not(all(target_os = "zkvm", feature = "profiling")))]
#[inline(always)]
#[doc(hidden)]
pub fn run_cycle_track<T, F>(_label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    f()
}

/// Complete authenticated validation state, serialized between recursive iterations.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub current_nbits: CompactTarget,
    pub height: u32,
    pub chain_work: ChainWork,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

impl State {
    /// The circular-buffer slot containing the current tip timestamp.
    #[must_use]
    fn current_timestamp_slot(&self) -> usize {
        self.height as usize % WINDOW_SIZE
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; STATE_SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        copy_from_bytes(bytes)
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
    ) -> Result<(CompactTarget, Target, ChainWork), TargetError> {
        let (current_nbits, current_target) = calculate_next_target_required(
            current_target,
            self.epoch_start_timestamp,
            previous_timestamp,
        );
        let current_work = work_from_target(current_target)?;
        Ok((current_nbits, current_target, current_work))
    }

    #[must_use]
    #[tracked]
    fn median_time_past_hinted(&self, claimed_median: BlockTimestamp) -> bool {
        let window_len = crate::timestamp_count(self.height as u64);
        if window_len == 0 {
            return true;
        }

        let median_index = window_len / 2;

        let (less_count, equal_count, greater_count) = cycle_track!({
            let mut less_count = 0usize;
            let mut equal_count = 0usize;
            let mut greater_count = 0usize;
            for timestamp in self.timestamps.iter().take(window_len) {
                if *timestamp < claimed_median {
                    less_count += 1;
                } else if *timestamp > claimed_median {
                    greater_count += 1;
                } else {
                    equal_count += 1;
                }
            }
            (less_count, equal_count, greater_count)
        });

        cycle_track!({
            less_count + equal_count + greater_count == window_len
                && less_count <= median_index
                && less_count + equal_count > median_index
        })
    }

    fn apply_chain_work_run(&mut self, run_work: ChainWork, run_count: u32) {
        if run_count > 0 {
            cycle_track!({
                let accumulated_work = run_work * run_count;
                self.chain_work = self.chain_work + accumulated_work;
            });
        }
    }

    #[allow(clippy::result_large_err)]
    #[tracked]
    pub fn apply_headers<F>(
        &mut self,
        headers: &[NewHeader],
        median_hints: &[BlockTimestamp],
        mut block_hash_fn: F,
    ) -> Result<(), ApplyFailure>
    where
        F: FnMut(&NewHeader, &BlockHash, CompactTarget) -> BlockHash,
    {
        assert_eq!(
            headers.len(),
            median_hints.len(),
            "median hint count must match header count"
        );

        let mut pending_run_work: Option<ChainWork> = None;
        let mut pending_run_count: u32 = 0;
        let mut active_nbits = self.current_nbits;
        let mut active_target = target_from_bits(active_nbits);
        let mut active_work =
            work_from_target(active_target).expect("current target must be valid blockchain data");

        let flush_pending_chain_work =
            |state: &mut State, run_work: &mut Option<ChainWork>, run_count: &mut u32| {
                if let Some(run_work) = run_work.take() {
                    state.apply_chain_work_run(run_work, *run_count);
                }
                *run_count = 0;
            };

        for (header_index, (new_header, claimed_median)) in
            headers.iter().zip(median_hints.iter().copied()).enumerate()
        {
            let candidate_height = cycle_track!(self.height + 1);
            let previous_timestamp =
                cycle_track!({ self.timestamps[self.current_timestamp_slot()] });
            let timestamp_slot = candidate_height as usize % WINDOW_SIZE;

            assert!(
                self.median_time_past_hinted(claimed_median),
                "invalid median time past hint at header index {}",
                header_index
            );

            if cycle_track!({ new_header.timestamp <= claimed_median }) {
                flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                return Err(ApplyFailure {
                    last_valid_state: self.clone(),
                    error_code: ValidationErrorCode::TimestampTooOld,
                    failure_height: candidate_height,
                });
            }

            if cycle_track!({ candidate_height.is_multiple_of(EPOCH_LENGTH) }) {
                flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                let prepared = self
                    .prepare_new_epoch(active_target, previous_timestamp)
                    .expect("epoch target must not be u256::MAX — invalid blockchain data");
                active_nbits = prepared.0;
                active_target = prepared.1;
                active_work = prepared.2;
            }

            let block_hash =
                cycle_track!({ block_hash_fn(new_header, &self.block_hash, active_nbits) });

            if !check_proof_of_work(block_hash, active_target) {
                flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                return Err(ApplyFailure {
                    last_valid_state: self.clone(),
                    error_code: ValidationErrorCode::PowInsufficient,
                    failure_height: candidate_height,
                });
            }

            if pending_run_work != Some(active_work) {
                flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                pending_run_work = Some(active_work);
            }

            pending_run_count += 1;
            cycle_track!({
                self.height = candidate_height;
                self.timestamps[timestamp_slot] = new_header.timestamp;
                self.current_nbits = active_nbits;
                if candidate_height.is_multiple_of(EPOCH_LENGTH) {
                    self.epoch_start_timestamp = new_header.timestamp;
                }
                self.block_hash = block_hash;
            });
        }

        flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);

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
