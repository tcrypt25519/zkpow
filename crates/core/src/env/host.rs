use super::{cycle_track, Env, StateInner};
use crate::{BlockTimestamp, WINDOW_SIZE};

#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostEnvironment;

impl super::sealed::Sealed for HostEnvironment {}
impl Env for HostEnvironment {}

impl StateInner<HostEnvironment> {
    /// Return the upper median time past for the currently tracked timestamps.
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
}
