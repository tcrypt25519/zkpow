use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use sp1_sdk::ExecutionReport;

use crate::{memory_monitor, pipeline::BoxError};

static PHASE_TIMINGS: OnceLock<Mutex<Vec<(String, Duration)>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct PhaseTiming {
    pub label:               String,
    pub total_duration_secs: f64,
    pub invocations:         u32,
}

fn phase_timings_store() -> &'static Mutex<Vec<(String, Duration)>> {
    PHASE_TIMINGS.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn clear_phase_timings() {
    if let Ok(mut timings) = phase_timings_store().lock() {
        timings.clear();
    }
}

fn record_phase_timing(label: &'static str, elapsed: Duration) {
    if let Ok(mut timings) = phase_timings_store().lock() {
        timings.push((label.to_string(), elapsed));
    }
}

pub(crate) fn collected_phase_timings() -> Vec<PhaseTiming> {
    let mut grouped: std::collections::HashMap<String, (Duration, u32)> = std::collections::HashMap::new();
    if let Ok(timings) = phase_timings_store().lock() {
        for (label, elapsed) in timings.iter() {
            let entry = grouped.entry(label.clone()).or_insert((Duration::ZERO, 0));
            entry.0 += *elapsed;
            entry.1 += 1;
        }
    }
    let mut out: Vec<_> = grouped
        .into_iter()
        .map(|(label, (total, invocations))| PhaseTiming {
            label,
            total_duration_secs: total.as_secs_f64(),
            invocations,
        })
        .collect();
    out.sort_unstable_by(|a, b| {
        b.total_duration_secs
            .partial_cmp(&a.total_duration_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}

pub fn log_execution_report(report: &ExecutionReport, total_proving_time_secs: f64) {
    tracing::info!("Execution report");
    tracing::info!("  total instructions: {}", report.total_instruction_count());

    if report.cycle_tracker.is_empty() {
        tracing::info!("  cycle tracker: unavailable or empty");
        tracing::info!("  prover gas: {}", report.gas().unwrap_or(0));
        return;
    }

    let mut entries: Vec<_> = report.cycle_tracker.iter().collect();
    entries.sort_unstable_by(|(label_a, cycles_a), (label_b, cycles_b)| {
        cycles_b.cmp(cycles_a).then_with(|| label_a.cmp(label_b))
    });

    let total_tracked_cycles: u64 = entries.iter().map(|(_, cycles)| **cycles).sum();
    tracing::info!(
        "  cycle tracker: {} tracked cycles across {} spans",
        total_tracked_cycles,
        entries.len()
    );

    tracing::info!("  top hot spans:");
    for (label, cycles) in entries.iter().take(12) {
        let invocations = report.invocation_tracker.get((*label).as_str()).copied().unwrap_or(1);
        let percent = if total_tracked_cycles == 0 {
            0.0
        } else {
            (**cycles as f64 * 100.0) / total_tracked_cycles as f64
        };
        tracing::info!(
            "    {}: {} cycles ({:.2}%){}",
            label,
            cycles,
            percent,
            if invocations > 1 {
                format!(" across {} invocations", invocations)
            } else {
                String::new()
            }
        );
    }

    let prover_gas = report.gas().unwrap_or(0);
    tracing::info!("  prover gas: {}", prover_gas);
    if prover_gas > 0 && total_tracked_cycles > 0 {
        tracing::info!("  estimated gas by hot span:");
        for (label, cycles) in entries.iter().take(10) {
            let share = **cycles as f64 / total_tracked_cycles as f64;
            let estimated_gas = (prover_gas as f64 * share).round() as u64;
            let estimated_secs = total_proving_time_secs * share;
            tracing::info!(
                "      {}: ~{} gas ({:.2}% of cycles, ~{:.2}s of total)",
                label,
                estimated_gas,
                share * 100.0,
                estimated_secs
            );
        }
    }
}

pub fn timed_sync<T, E, F>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Result<T, E>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    let start_memory = memory_monitor::log_point(label, "proof phase started");
    let output = f().map_err(Into::into);
    let elapsed = started.elapsed();
    record_phase_timing(label, elapsed);
    if memory_monitor::logging_enabled() {
        memory_monitor::log_delta(label, start_memory, memory_monitor::sample(), elapsed, "proof phase finished");
    }
    output
}

pub async fn timed_async<T, E, F, Fut>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    let start_memory = memory_monitor::log_point(label, "proof phase started");
    let output = f().await.map_err(Into::into);
    let elapsed = started.elapsed();
    record_phase_timing(label, elapsed);
    if memory_monitor::logging_enabled() {
        memory_monitor::log_delta(label, start_memory, memory_monitor::sample(), elapsed, "proof phase finished");
    }
    output
}
