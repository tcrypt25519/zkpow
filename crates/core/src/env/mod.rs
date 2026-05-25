#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

use crate::{
    calculate_next_work_required, check_proof_of_work, copy_from_bytes, copy_to_bytes,
    ref_from_bytes, target_gt, work_from_target, ApplyFailure, BlockHash, BlockTimestamp,
    ChainWork, Header, ParseError, PublicChainClaim, Target, ValidationErrorCode,
    EXPECTED_EPOCH_TIMESPAN, GENESIS_TARGET, MAX_EPOCH_TIMESPAN, MIN_EPOCH_TIMESPAN,
    PRIVATE_CONTINUATION_STATE_SIZE, STATE_SIZE, WINDOW_SIZE,
};
use core::marker::PhantomData;

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

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for environment-specific state APIs.
#[doc(hidden)]
pub trait Env: sealed::Sealed + core::fmt::Debug + Clone + Copy + Default + PartialEq + Eq {}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuestEnvironment;

impl sealed::Sealed for GuestEnvironment {}
impl Env for GuestEnvironment {}

#[cfg(feature = "host")]
mod host;

#[cfg(feature = "host")]
pub(crate) use host::HostEnvironment;

#[cfg(not(feature = "host"))]
type SelectedEnvironment = GuestEnvironment;
#[cfg(feature = "host")]
type SelectedEnvironment = HostEnvironment;

/// Complete authenticated validation state, serialized between recursive iterations.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct StateInner<E: Env> {
    pub header: Header,
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub height: u32,
    pub chain_work: ChainWork,
    pub work: ChainWork,
    pub target: Target,

    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
    pub _environment: PhantomData<E>,
}

/// Public state type selected by the `host` feature.
pub type State = StateInner<SelectedEnvironment>;

impl<E: Env> StateInner<E> {
    /// The number of timestamps currently tracked for median-time-past.
    #[must_use]
    pub fn timestamp_count(&self) -> usize {
        (self.height as usize + 1).min(WINDOW_SIZE)
    }

    /// The circular-buffer slot where the next timestamp should be written.
    #[must_use]
    pub(crate) fn next_timestamp_slot(&self) -> usize {
        (self.height as usize + 1) % WINDOW_SIZE
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
        out[0..32].copy_from_slice(&self.work.to_le_bytes());
        out[32..64].copy_from_slice(&self.target.to_le_bytes());
        out[64..68].copy_from_slice(&self.epoch_start_timestamp.to_le_bytes());
        for (i, ts) in self.timestamps.iter().enumerate() {
            out[68 + i * 4..68 + (i + 1) * 4].copy_from_slice(&ts.to_le_bytes());
        }
        out
    }

    /// Build the next authenticated state from the current state, a prover-supplied header,
    /// and a pre-computed block hash.
    fn next_inner(
        &mut self,
        header: Header,
        block_hash: BlockHash,
        median_time_past: BlockTimestamp,
        update_chain_work: bool,
    ) -> Result<(), ValidationErrorCode> {
        cycle_track("state/next_inner", || {
            // If this block opens a new difficulty epoch, retarget *before* the PoW
            // check so the new target is already in place when we validate the hash.
            if cycle_track("state/next_inner/check_retarget", || {
                self.height.is_multiple_of(2016) && self.height != 0
            }) {
                cycle_track("state/next_inner/retarget", || {
                    let actual_timespan =
                        self.header.timestamp.as_i64() - self.epoch_start_timestamp.as_i64();
                    let clamped_timespan =
                        actual_timespan.clamp(MIN_EPOCH_TIMESPAN, MAX_EPOCH_TIMESPAN) as u32;

                    let (mut new_target, new_nbits) = calculate_next_work_required(
                        self.target,
                        clamped_timespan,
                        EXPECTED_EPOCH_TIMESPAN,
                    );
                    if target_gt(new_target, GENESIS_TARGET) {
                        new_target = GENESIS_TARGET;
                    }

                    self.header.compact_target = new_nbits;
                    self.target = new_target;
                    self.work = work_from_target(new_target);
                });
            }

            let (required_target, required_work, timestamp_slot) =
                cycle_track("state/next_inner/setup", || {
                    (self.target, self.work, self.next_timestamp_slot())
                });

            cycle_track("state/next_inner/validate/median_time_past", || {
                if header.timestamp <= median_time_past {
                    return Err(ValidationErrorCode::TimestampTooOld);
                }
                Ok(())
            })?;

            cycle_track("state/next_inner/validate/pow", || {
                if !check_proof_of_work(block_hash, required_target) {
                    return Err(ValidationErrorCode::PowInsufficient);
                }
                Ok(())
            })?;

            cycle_track("state/next_inner/update_height", || {
                self.height += 1;
            });
            cycle_track("state/next_inner/timestamp_window", || {
                self.timestamps[timestamp_slot] = header.timestamp;
            });

            if cycle_track("state/next_inner/check_epoch_timestamp", || {
                self.height.is_multiple_of(2016)
            }) {
                cycle_track("state/next_inner/epoch_timestamp", || {
                    self.epoch_start_timestamp = header.timestamp;
                });
            }

            if update_chain_work {
                self.chain_work = cycle_track("state/next_inner/chain_work", || {
                    self.chain_work + required_work
                });
            }
            cycle_track("state/next_inner/assign_state", || {
                self.header = header;
                self.block_hash = block_hash;
            });
            Ok(())
        })
    }
}

impl<E: Env> StateInner<E> {
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

    #[allow(clippy::result_large_err)]
    pub fn apply_headers<F>(
        &mut self,
        headers: &[Header],
        median_hints: &[BlockTimestamp],
        mut hash_header: F,
    ) -> Result<(), ApplyFailure<E>>
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
                |state: &mut StateInner<E>,
                 run_work: &mut Option<ChainWork>,
                 run_count: &mut u32| {
                    if let (Some(run_work), count) = (run_work.take(), *run_count) {
                        if count > 0 {
                            cycle_track("state/apply_headers/chain_work_flush", || {
                                let accumulated_work = run_work * count;
                                state.chain_work = state.chain_work + accumulated_work;
                            });
                        }
                    }
                    *run_count = 0;
                };

            for (header_index, (header, claimed_median)) in headers
                .iter()
                .copied()
                .zip(median_hints.iter().copied())
                .enumerate()
            {
                let required_work = self.work;
                // let required_nbits = self.header.compact_target;
                // let header = cycle_track("state/apply_headers/build_header", || {
                //     new_header.into_header(self.block_hash, required_nbits)
                // });
                let hash = cycle_track("state/apply_headers/hash_header", || hash_header(&header));
                cycle_track("state/apply_headers/median_hint_check", || {
                    assert!(
                        self.median_time_past_hinted(claimed_median),
                        "invalid median time past hint at header index {}",
                        header_index
                    );
                });
                let median_time_past = claimed_median;
                if pending_run_work != Some(required_work) {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                    pending_run_work = Some(required_work);
                }

                if let Err(error_code) = self.next_inner(header, hash, median_time_past, false) {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                    return Err(ApplyFailure {
                        last_valid_state: self.clone(),
                        error_code,
                        failure_height: self.height + 1,
                    });
                }

                pending_run_count += 1;
                // After next_inner, self.work may have changed (retarget fired). If so,
                // the pending run batch is over and we flush before starting the new rate.
                if self.work != required_work {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                }
            }

            flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);

            Ok(())
        })
    }
}

impl<E: Env> Default for StateInner<E> {
    fn default() -> Self {
        Self {
            header: Header::default(),
            block_hash: BlockHash::default(),
            genesis_hash: BlockHash::default(),
            height: 0,
            chain_work: ChainWork::default(),
            work: ChainWork::default(),
            target: Target::default(),
            epoch_start_timestamp: BlockTimestamp::default(),
            timestamps: [BlockTimestamp::default(); WINDOW_SIZE],
            _environment: PhantomData,
        }
    }
}
