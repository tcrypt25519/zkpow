//! Lightweight memory monitoring and process RSS tracking.

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::{self, Command};
#[cfg(any(target_os = "macos", test))]
use std::str::from_utf8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageSample {
    rss_kb: u64,
    live_kb: u64,
}

impl StageSample {
    pub const fn new(rss_kb: u64, live_kb: u64) -> Self {
        Self { rss_kb, live_kb }
    }

    pub const fn rss_kb(&self) -> u64 {
        self.rss_kb
    }

    pub const fn live_kb(&self) -> u64 {
        self.live_kb
    }
}

/// Resident set size of this process in KiB, on Linux and macOS.
pub fn rss_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        parse_proc_status_rss(&fs::read_to_string("/proc/self/status").ok()?)
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps").args(["-o", "rss=", "-p"]).arg(process::id().to_string()).output().ok()?;
        parse_ps_rss(&output.stdout)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

#[allow(dead_code)]
#[cfg(any(target_os = "linux", test))]
fn parse_proc_status_rss(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let mut fields = line.strip_prefix("VmRSS:")?.split_whitespace();
        let value = fields.next()?.parse().ok()?;
        (fields.next() == Some("kB") && fields.next().is_none()).then_some(value)
    })
}

#[cfg(any(target_os = "macos", test))]
fn parse_ps_rss(stdout: &[u8]) -> Option<u64> {
    from_utf8(stdout).ok()?.trim().parse().ok()
}

/// Sample RSS.
pub fn sample() -> StageSample {
    StageSample::new(rss_kb().unwrap_or(0), 0)
}

/// Sample and log a memory data point. When memory logging is disabled this
/// is free: no RSS read happens and a zero sample is returned.
pub fn log_point(label: &'static str, message: &'static str) -> StageSample {
    if !logging_enabled() {
        return StageSample::new(0, 0);
    }
    let sample = sample();
    tracing::info!(
        label,
        rss_kb = sample.rss_kb(),
        live_kb = sample.live_kb(),
        "{message}"
    );
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
            "{message}"
        );
    }
}

pub fn logging_enabled() -> bool {
    #[cfg(feature = "memory-diagnostics")]
    {
        return true;
    }
    #[cfg(not(feature = "memory-diagnostics"))]
    {
        std::env::var_os("ZKPOW_MEMORY_LOGGING")
            .map(|val| val == "1" || val == "true" || val == "on")
            .unwrap_or(false)
    }
}
