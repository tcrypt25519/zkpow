use memory_usage::{rss_kb, AllocationTracker, StageSample};

pub static ALLOCATION_TRACKER: AllocationTracker = AllocationTracker::new();

pub fn sample() -> StageSample {
    StageSample::new(rss_kb().unwrap_or(0), ALLOCATION_TRACKER.live_kb())
}

pub fn log_point(label: &'static str, message: &'static str) -> StageSample {
    let sample = sample();
    if logging_enabled() {
        tracing::info!(
            label,
            rss_kb = sample.rss_kb(),
            live_kb = sample.live_kb(),
            peak_kb = ALLOCATION_TRACKER.peak_kb(),
            "{message}"
        );
    }
    sample
}

pub fn log_delta(
    label: &'static str,
    started: StageSample,
    finished: StageSample,
    elapsed: std::time::Duration,
    message: &'static str,
) {
    if logging_enabled() {
        tracing::info!(
            label,
            elapsed_us = elapsed.as_micros() as u64,
            start_rss_kb = started.rss_kb(),
            end_rss_kb = finished.rss_kb(),
            rss_delta_kb = finished.rss_kb() as i64 - started.rss_kb() as i64,
            start_live_kb = started.live_kb(),
            end_live_kb = finished.live_kb(),
            live_delta_kb = finished.live_kb() as i64 - started.live_kb() as i64,
            peak_kb = ALLOCATION_TRACKER.peak_kb(),
            "{message}"
        );
    }
}

#[cfg(feature = "memory-diagnostics")]
pub fn logging_enabled() -> bool {
    true
}

#[cfg(not(feature = "memory-diagnostics"))]
pub fn logging_enabled() -> bool {
    false
}
