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

struct HeaderApplyContext {
    pending_run_work: Option<ChainWork>,
    pending_run_count: u32,
    active_nbits: CompactTarget,
    active_target: Target,
    active_work: ChainWork,
}

impl HeaderApplyContext {
    fn new(current_nbits: CompactTarget) -> Self {
        let active_target = target_from_bits(current_nbits);
        let active_work =
            work_from_target(active_target).expect("current target must be valid blockchain data");
        Self {
            pending_run_work: None,
            pending_run_count: 0,
            active_nbits: current_nbits,
            active_target,
            active_work,
        }
    }
}

impl State {
    /// The circular-buffer slot containing the current tip timestamp.
    #[must_use]
    fn current_timestamp_slot(&self) -> usize {
        self.height as usize % WINDOW_SIZE
    }

    /// Whether this state has the full 11-block timestamp window needed by the
    /// steady-state median-time-past path.
    #[must_use]
    fn has_full_timestamp_window(&self) -> bool {
        self.height as usize >= WINDOW_SIZE - 1
    }

    /// Number of incoming headers that must stay on the cold, variable-window
    /// path before the steady-state path can assume 11 timestamps exist.
    #[must_use]
    fn cold_header_count(&self, incoming_headers: usize) -> usize {
        if self.has_full_timestamp_window() {
            0
        } else {
            ((WINDOW_SIZE - 1) - self.height as usize).min(incoming_headers)
        }
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
    fn median_time_past_hinted_cold(&self, claimed_median: BlockTimestamp) -> bool {
        let window_len = self.height as usize + 1;
        let median_index = window_len / 2;
        self.median_time_past_hinted_for_window(claimed_median, window_len, median_index)
    }

    #[must_use]
    #[tracked]
    fn median_time_past_hinted_warm(&self, claimed_median: BlockTimestamp) -> bool {
        self.median_time_past_hinted_for_window(claimed_median, WINDOW_SIZE, WINDOW_SIZE / 2)
    }

    #[must_use]
    #[tracked]
    fn median_time_past_hinted_for_window(
        &self,
        claimed_median: BlockTimestamp,
        window_len: usize,
        median_index: usize,
    ) -> bool {
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

    fn flush_pending_chain_work(&mut self, context: &mut HeaderApplyContext) {
        if let Some(run_work) = context.pending_run_work.take() {
            self.apply_chain_work_run(run_work, context.pending_run_count);
        }
        context.pending_run_count = 0;
    }

    #[allow(clippy::result_large_err)]
    fn apply_header_after_median_check<F>(
        &mut self,
        new_header: &NewHeader,
        claimed_median: BlockTimestamp,
        context: &mut HeaderApplyContext,
        block_hash_fn: &mut F,
    ) -> Result<(), ApplyFailure>
    where
        F: FnMut(&NewHeader, &BlockHash, CompactTarget) -> BlockHash,
    {
        let candidate_height = cycle_track!(self.height + 1);
        let previous_timestamp = cycle_track!({ self.timestamps[self.current_timestamp_slot()] });
        let timestamp_slot = candidate_height as usize % WINDOW_SIZE;

        if cycle_track!({ new_header.timestamp <= claimed_median }) {
            self.flush_pending_chain_work(context);
            return Err(ApplyFailure {
                last_valid_state: self.clone(),
                error_code: ValidationErrorCode::TimestampTooOld,
                failure_height: candidate_height,
            });
        }

        if cycle_track!({ candidate_height.is_multiple_of(EPOCH_LENGTH) }) {
            self.flush_pending_chain_work(context);
            let prepared = self
                .prepare_new_epoch(context.active_target, previous_timestamp)
                .expect("epoch target must not be u256::MAX — invalid blockchain data");
            context.active_nbits = prepared.0;
            context.active_target = prepared.1;
            context.active_work = prepared.2;
        }

        let block_hash =
            cycle_track!({ block_hash_fn(new_header, &self.block_hash, context.active_nbits) });

        if !check_proof_of_work(block_hash, context.active_target) {
            self.flush_pending_chain_work(context);
            return Err(ApplyFailure {
                last_valid_state: self.clone(),
                error_code: ValidationErrorCode::PowInsufficient,
                failure_height: candidate_height,
            });
        }

        if context.pending_run_work != Some(context.active_work) {
            self.flush_pending_chain_work(context);
            context.pending_run_work = Some(context.active_work);
        }

        context.pending_run_count += 1;
        cycle_track!({
            self.height = candidate_height;
            self.timestamps[timestamp_slot] = new_header.timestamp;
            self.current_nbits = context.active_nbits;
            if candidate_height.is_multiple_of(EPOCH_LENGTH) {
                self.epoch_start_timestamp = new_header.timestamp;
            }
            self.block_hash = block_hash;
        });

        Ok(())
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

        let mut context = HeaderApplyContext::new(self.current_nbits);
        let cold_count = self.cold_header_count(headers.len());
        let (cold_headers, warm_headers) = headers.split_at(cold_count);
        let (cold_median_hints, warm_median_hints) = median_hints.split_at(cold_count);

        for (header_index, (new_header, claimed_median)) in cold_headers
            .iter()
            .zip(cold_median_hints.iter().copied())
            .enumerate()
        {
            assert!(
                self.median_time_past_hinted_cold(claimed_median),
                "invalid median time past hint at header index {}",
                header_index
            );
            self.apply_header_after_median_check(
                new_header,
                claimed_median,
                &mut context,
                &mut block_hash_fn,
            )?;
        }

        for (warm_index, (new_header, claimed_median)) in warm_headers
            .iter()
            .zip(warm_median_hints.iter().copied())
            .enumerate()
        {
            let header_index = cold_count + warm_index;
            assert!(
                self.median_time_past_hinted_warm(claimed_median),
                "invalid median time past hint at header index {}",
                header_index
            );
            self.apply_header_after_median_check(
                new_header,
                claimed_median,
                &mut context,
                &mut block_hash_fn,
            )?;
        }

        self.flush_pending_chain_work(&mut context);

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
