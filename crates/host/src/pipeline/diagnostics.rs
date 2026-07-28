use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use sp1_sdk::ExecutionReport;

use crate::{memory_monitor, pipeline::BoxError};

static PHASE_TIMINGS: OnceLock<Mutex<HashMap<&'static str, PhaseTimingAccum>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct PhaseTiming {
    pub label:               String,
    pub total_duration_secs: f64,
    pub invocations:         u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct PhaseTimingAccum {
    total:       Duration,
    invocations: u32,
}

fn phase_timings_store() -> &'static Mutex<HashMap<&'static str, PhaseTimingAccum>> {
    PHASE_TIMINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn clear_phase_timings() {
    if let Ok(mut timings) = phase_timings_store().lock() {
        timings.clear();
    }
}

fn record_phase_timing(label: &'static str, elapsed: Duration) {
    if let Ok(mut timings) = phase_timings_store().lock() {
        let entry = timings.entry(label).or_default();
        entry.total += elapsed;
        entry.invocations += 1;
    }
}

pub(crate) fn collected_phase_timings() -> Vec<PhaseTiming> {
    let mut out = if let Ok(timings) = phase_timings_store().lock() {
        timings
            .iter()
            .map(|(label, accum)| PhaseTiming {
                label:               (*label).to_string(),
                total_duration_secs: accum.total.as_secs_f64(),
                invocations:         accum.invocations,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    out.sort_unstable_by(|a, b| {
        b.total_duration_secs
            .partial_cmp(&a.total_duration_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}

#[derive(Debug, Default)]
struct CycleTreeNode {
    self_cycles:      u64,
    self_invocations: u64,
    total_cycles:     u64,
    children:         BTreeMap<String, CycleTreeNode>,
}

impl CycleTreeNode {
    fn insert(&mut self, path: &str, cycles: u64, invocations: u64) {
        let mut current = self;
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            current = current.children.entry(segment.to_string()).or_default();
        }
        current.self_cycles = current.self_cycles.saturating_add(cycles);
        current.self_invocations = current.self_invocations.saturating_add(invocations);
    }

    fn finalize_totals(&mut self) -> u64 {
        let mut total = self.self_cycles;
        for child in self.children.values_mut() {
            total = total.saturating_add(child.finalize_totals());
        }
        self.total_cycles = total;
        total
    }
}

fn emit_cycle_tree(node: &CycleTreeNode, total_cycles: u64, depth: usize) {
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_unstable_by(|(name_a, node_a), (name_b, node_b)| {
        node_b.total_cycles.cmp(&node_a.total_cycles).then_with(|| name_a.cmp(name_b))
    });

    for (name, child) in children {
        let pct = if total_cycles == 0 {
            0.0
        } else {
            (child.total_cycles as f64 * 100.0) / total_cycles as f64
        };
        let indent = "  ".repeat(depth);
        tracing::info!(
            "  {}- {}: {} cycles ({:.2}%){}",
            indent,
            name,
            child.total_cycles,
            pct,
            if child.self_invocations > 1 {
                format!(", {} invocations", child.self_invocations)
            } else {
                String::new()
            }
        );
        emit_cycle_tree(child, total_cycles, depth + 1);
    }
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

    let mut tree = CycleTreeNode::default();
    for (label, cycles) in &entries {
        let invocations = report.invocation_tracker.get((*label).as_str()).copied().unwrap_or(1);
        tree.insert(label, **cycles, invocations);
    }
    tree.finalize_totals();
    tracing::info!("  cycle hierarchy:");
    emit_cycle_tree(&tree, total_tracked_cycles, 0);

    let prover_gas = report.gas().unwrap_or(0);
    tracing::info!("  prover gas:");
    tracing::info!("    total prover_gas: {}", prover_gas);
    tracing::info!(
        "    assumptions: proportional allocation from tracked cycle share (model coefficients internal to SP1)"
    );
    if prover_gas > 0 && total_tracked_cycles > 0 {
        tracing::info!("    estimated gas by hot span:");
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
