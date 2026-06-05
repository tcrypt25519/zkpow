#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use crate::{
    calculate_next_target_required, check_proof_of_work, copy_from_bytes, copy_to_bytes,
    ref_from_bytes, work_from_target, ApplyFailure, BlockHash, BlockTimestamp, ChainWork,
    CompactTarget, Header, NewHeader, ParseError, PublicChainClaim, Target, ValidationErrorCode,
    EPOCH_LENGTH, PRIVATE_CONTINUATION_STATE_SIZE, STATE_SIZE, WINDOW_SIZE,
};

/// Execute a closure while emitting stable, report-backed cycle-tracker markers in the guest.
#[cfg(all(target_os = "zkvm", feature = "profiling"))]
#[inline(always)]
pub fn cycle_track_report<T, F>(label: &'static str, f: F) -> T
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

/// Execute a closure while preserving the call shape when report-backed
/// profiling is not enabled.
#[cfg(not(all(target_os = "zkvm", feature = "profiling")))]
#[inline(always)]
pub fn cycle_track_report<T, F>(_label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    f()
}

/// Backwards-compatible helper for existing call sites.
#[inline(always)]
pub fn cycle_track<T, F>(label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    cycle_track_report(label, f)
}

/// Complete authenticated validation state, serialized between recursive iterations.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub header: Header,
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub current_nbits: CompactTarget,
    pub height: u32,
    pub chain_work: ChainWork,
    pub current_work: ChainWork,
    /// Expanded 256-bit target for the current difficulty period.
    /// Cached to avoid recomputing from `current_nbits` on every block.
    /// Updated only at retarget boundaries and at genesis.
    pub current_target: Target,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

impl State {
    /// The number of timestamps currently tracked for median-time-past.
    #[must_use]
    pub fn timestamp_count(&self) -> usize {
        (self.height as usize + 1).min(WINDOW_SIZE)
    }

    /// The circular-buffer slot containing the current tip timestamp.
    #[must_use]
    pub(crate) fn current_timestamp_slot(&self) -> usize {
        self.height as usize % WINDOW_SIZE
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; STATE_SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        cycle_track("parse/state", || copy_from_bytes(bytes))
    }

    /// Borrow a [`State`] directly from aligned protocol bytes.
    pub fn ref_from_bytes(bytes: &[u8]) -> Result<&Self, ParseError> {
        cycle_track("parse/state_ref", || ref_from_bytes(bytes))
    }

    /// Extract the verifier-visible public claim from this state.
    #[must_use]
    pub fn public_claim(&self) -> PublicChainClaim {
        PublicChainClaim {
            genesis_hash: self.genesis_hash,
            tip_hash: self.block_hash,
            chain_work: self.chain_work,
            height: self.height,
        }
    }

    /// Serialize the private continuation fields directly to bytes,
    /// bypassing [`PrivateContinuationState`](crate::PrivateContinuationState) construction.
    #[must_use]
    pub fn continuation_bytes(&self) -> [u8; PRIVATE_CONTINUATION_STATE_SIZE] {
        let mut out = [0u8; PRIVATE_CONTINUATION_STATE_SIZE];
        out[0..4].copy_from_slice(self.current_nbits.to_le_bytes_slice());
        out[4..36].copy_from_slice(self.current_work.to_le_bytes_slice());
        out[36..68].copy_from_slice(self.current_target.to_le_bytes_slice());
        out[68..72].copy_from_slice(self.epoch_start_timestamp.to_le_bytes_slice());
        for (i, ts) in self.timestamps.iter().enumerate() {
            out[72 + i * 4..72 + (i + 1) * 4].copy_from_slice(ts.to_le_bytes_slice());
        }
        out
    }

    /// The expanded proof-of-work target active at the current height.
    #[must_use]
    pub fn current_target(&self) -> Target {
        self.current_target
    }

    /// Compute the difficulty values that become active at a new epoch boundary.
    pub(crate) fn prepare_new_epoch(
        &self,
        previous_timestamp: BlockTimestamp,
    ) -> (CompactTarget, Target, ChainWork) {
        cycle_track("state/prepare_new_epoch", || {
            let (current_nbits, current_target) = calculate_next_target_required(
                self.current_target,
                self.epoch_start_timestamp,
                previous_timestamp,
            );
            let current_work = work_from_target(current_target);
            (current_nbits, current_target, current_work)
        })
    }

    #[must_use]
    fn median_time_past_hinted(&self, claimed_median: BlockTimestamp) -> bool {
        cycle_track("state/median_time_past_hinted", || {
            let window_len = self.timestamp_count();
            if window_len == 0 {
                return true;
            }

            let median_index = window_len / 2;

            let (less_count, equal_count, greater_count) =
                cycle_track("state/median_time_past_hinted/loop", || {
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

            cycle_track("state/median_time_past_hinted/check_counts", || {
                less_count + equal_count + greater_count == window_len
                    && less_count <= median_index
                    && less_count + equal_count > median_index
            })
        })
    }

    /// Return the upper median time past for the currently tracked timestamps.
    #[cfg(feature = "host")]
    #[must_use]
    pub fn median_time_past(&self) -> BlockTimestamp {
        cycle_track("state/host/median_time_past", || {
            let count = self.timestamp_count();
            let mut sorted = self.timestamps;
            if count >= WINDOW_SIZE {
                cycle_track("state/host/median_time_past/sort", || {
                    sorted.sort_unstable();
                });
                return sorted[WINDOW_SIZE / 2];
            }

            cycle_track("state/host/median_time_past/sort", || {
                sorted[..count].sort_unstable();
            });
            sorted[count / 2]
        })
    }

    pub fn apply_chain_work_run(&mut self, run_work: ChainWork, run_count: u32) {
        if run_count > 0 {
            cycle_track("state/apply_headers/chain_work_flush", || {
                let accumulated_work = run_work * run_count;
                self.chain_work = self.chain_work + accumulated_work;
            });
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn apply_headers<F>(
        &mut self,
        headers: &[NewHeader],
        median_hints: &[BlockTimestamp],
        mut hash_header: F,
    ) -> Result<(), ApplyFailure>
    where
        F: FnMut(&Header) -> BlockHash,
    {
        cycle_track("state/apply_headers", || {
            assert_eq!(
                headers.len(),
                median_hints.len(),
                "median hint count must match header count"
            );

            let mut pending_run_work: Option<ChainWork> = None;
            let mut pending_run_count: u32 = 0;

            let flush_pending_chain_work =
                |state: &mut State, run_work: &mut Option<ChainWork>, run_count: &mut u32| {
                    if let Some(run_work) = run_work.take() {
                        state.apply_chain_work_run(run_work, *run_count);
                    }
                    *run_count = 0;
                };

            for (header_index, (new_header, claimed_median)) in headers
                .iter()
                .copied()
                .zip(median_hints.iter().copied())
                .enumerate()
            {
                let candidate_height =
                    cycle_track("state/apply_headers/candidate_height", || self.height + 1);
                let previous_timestamp =
                    cycle_track("state/apply_headers/load_previous_timestamp", || {
                        self.timestamps[self.current_timestamp_slot()]
                    });
                let timestamp_slot = candidate_height as usize % WINDOW_SIZE;

                cycle_track("state/apply_headers/median_hint_check", || {
                    assert!(
                        self.median_time_past_hinted(claimed_median),
                        "invalid median time past hint at header index {}",
                        header_index
                    );
                });

                if cycle_track("state/apply_headers/validate/median_time_past", || {
                    new_header.timestamp <= claimed_median
                }) {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                    return Err(ApplyFailure {
                        last_valid_state: self.clone(),
                        error_code: ValidationErrorCode::TimestampTooOld,
                        failure_height: candidate_height,
                    });
                }

                let mut active_nbits = self.current_nbits;
                let mut active_target = self.current_target;
                let mut active_work = self.current_work;
                if cycle_track("state/apply_headers/check_retarget", || {
                    candidate_height.is_multiple_of(EPOCH_LENGTH)
                }) {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                    let prepared = self.prepare_new_epoch(previous_timestamp);
                    active_nbits = prepared.0;
                    active_target = prepared.1;
                    active_work = prepared.2;
                }

                let header = cycle_track("state/apply_headers/build_header", || {
                    new_header.into_header(self.block_hash, active_nbits)
                });
                let block_hash =
                    cycle_track("state/apply_headers/hash_header", || hash_header(&header));

                if cycle_track("state/apply_headers/validate/pow", || {
                    !check_proof_of_work(block_hash, active_target)
                }) {
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
                cycle_track("state/apply_headers/assign_state", || {
                    self.height = candidate_height;
                    self.timestamps[timestamp_slot] = header.timestamp;
                    self.current_nbits = active_nbits;
                    self.current_target = active_target;
                    self.current_work = active_work;
                    if candidate_height.is_multiple_of(EPOCH_LENGTH) {
                        self.epoch_start_timestamp = header.timestamp;
                    }
                    self.header = header;
                    self.block_hash = block_hash;
                });
            }

            flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);

            Ok(())
        })
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            header: Header::default(),
            block_hash: BlockHash::default(),
            genesis_hash: BlockHash::default(),
            current_nbits: CompactTarget::default(),
            height: 0,
            chain_work: ChainWork::default(),
            current_work: ChainWork::default(),
            current_target: Target::default(),
            epoch_start_timestamp: BlockTimestamp::default(),
            timestamps: [BlockTimestamp::default(); WINDOW_SIZE],
        }
    }
}
