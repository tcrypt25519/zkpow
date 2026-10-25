use std::{
    collections::BTreeSet,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use sp1_sdk::ExecutionReport;
use tracing::field::{Field, Visit};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

use crate::{memory_monitor, pipeline::BoxError};

static PHASE_TIMINGS: OnceLock<Mutex<Vec<(String, Duration)>>> = OnceLock::new();
static SHARD_AREAS: Mutex<Vec<ShardArea>> = Mutex::new(Vec::new());
static CONFLICTING_CHIPS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Prover gas at which a core shard reaches SP1's `ELEMENT_THRESHOLD` trace area. Gas is
/// `(3 * trace_area + complexity) / 191`, so the crossing point moves with the workload's
/// complexity-to-area ratio; this is calibrated for the SHA-heavy header mix. Set
/// `ZKPOW_GAS_PER_SHARD` to recalibrate against an observed count.
const DEFAULT_GAS_PER_SHARD: u64 = 7_800_000;

/// Trace cells at which SP1 starts a new core shard, mirroring
/// `sp1_core_executor::ELEMENT_THRESHOLD` and its `ELEMENT_THRESHOLD` override.
const DEFAULT_SHARD_AREA: u64 = (1 << 28) + (1 << 27);

/// Every chip a recursion machine has, one per `RecursionAir` variant, with `Poseidon2Wide` listed
/// under both names it takes by degree. The RISC-V machine's chips are open-ended and share no name
/// with any of these, so this set stands for one machine and its complement for the other: a chip
/// in it is evidence of a recursion shard, and any other chip is evidence of a shard SP1 cut
/// against [`DEFAULT_SHARD_AREA`]. Both are proved by the same code, under the same target, and
/// SP1 reports no proof mode alongside either. A shard whose chips point both ways means the two
/// sets have drifted apart; [`ShardKind::Unknown`] holds it rather than attributing it.
/// `scripts/check-sp1-chip-sets.sh` checks this set against SP1's own two machines.
pub(crate) const RECURSION_CHIPS: [&str; 12] = [
    "BaseAlu",
    "ExtAlu",
    "ExtFeltConvert",
    "MemoryConst",
    "MemoryVar",
    "Poseidon2LinearLayer",
    "Poseidon2SBox",
    "Poseidon2WideDeg3",
    "Poseidon2WideDeg9",
    "PrefixSumChecks",
    "PublicValues",
    "Select",
];

/// One labelled phase's summed wall-clock time within a batch.
#[derive(Debug, Clone)]
pub struct PhaseTiming {
    pub label:               String,
    pub total_duration_secs: f64,
    pub invocations:         u32,
}

fn env_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

/// Which machine proved a shard, as far as its chip list says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShardKind {
    Core,
    Recursion,
    Unknown,
}

/// One proved shard's trace area, and which machine proved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShardArea {
    pub cells: u64,
    pub kind:  ShardKind,
}

/// The trace area SP1 reports for one shard, read out of its own log line.
pub(crate) fn shard_area(message: &str) -> Option<u64> {
    let cells = message.strip_prefix("Total number of cells: ")?.split(',').next()?;
    cells.replace('_', "").parse().ok()
}

/// The chip a shard statistics line is about, or `None` for any other line.
pub(crate) fn chip_name(message: &str) -> Option<&str> {
    let (name, rest) = message.split_once('|')?;
    rest.trim_start().starts_with("Prep Cols = ").then(|| name.trim())
}

/// Which machines a shard's chip list has pointed to so far.
#[derive(Default)]
struct ShardChips {
    core:      bool,
    recursion: bool,
}

impl ShardChips {
    /// Evidence for exactly one machine settles the shard; none or both leaves it unattributed.
    fn kind(&self) -> ShardKind {
        match (self.core, self.recursion) {
            (true, false) => ShardKind::Core,
            (false, true) => ShardKind::Recursion,
            _ => ShardKind::Unknown,
        }
    }

    /// Records what one chip says, and whether it put the shard on both machines at once.
    fn observe(&mut self, name: &str) -> bool {
        let (side, other) = if RECURSION_CHIPS.contains(&name) {
            (&mut self.recursion, self.core)
        } else {
            (&mut self.core, self.recursion)
        };
        *side = true;
        other
    }
}

#[derive(Default)]
struct MessageVisitor(Option<String>);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

/// Collects the trace area SP1 reports as it proves each shard.
///
/// Nothing on SP1's public API reports trace area, and trace area is what decides where a shard is
/// cut, so the only measured number comes from `sp1_hypercube`'s own log lines. Registering this
/// layer is what raises that one target to debug.
pub(crate) struct ShardAreas;

impl<S> Layer<S> for ShardAreas
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.event_span(event) else { return };
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let Some(message) = visitor.0 else { return };

        if let Some(name) = chip_name(&message) {
            let mut extensions = span.extensions_mut();
            if extensions.get_mut::<ShardChips>().is_none() {
                extensions.insert(ShardChips::default());
            }
            if let Some(chips) = extensions.get_mut::<ShardChips>() {
                if chips.observe(name) {
                    if let Ok(mut conflicting) = CONFLICTING_CHIPS.lock() {
                        conflicting.insert(name.to_string());
                    }
                }
            }
            return;
        }
        let Some(cells) = shard_area(&message) else { return };
        let Some(kind) = span.extensions().get::<ShardChips>().map(ShardChips::kind) else { return };
        if let Ok(mut areas) = SHARD_AREAS.lock() {
            areas.push(ShardArea { cells, kind });
        }
    }
}

pub(crate) fn clear_shard_areas() {
    if let Ok(mut areas) = SHARD_AREAS.lock() {
        areas.clear();
    }
    if let Ok(mut conflicting) = CONFLICTING_CHIPS.lock() {
        conflicting.clear();
    }
}

/// Chip names that put a shard on both machines at once, since the last clear.
pub(crate) fn conflicting_chips() -> Vec<String> {
    CONFLICTING_CHIPS.lock().map(|chips| chips.iter().cloned().collect()).unwrap_or_default()
}

/// Every shard proved since the last clear, fullest first.
pub(crate) fn shard_areas() -> Vec<ShardArea> {
    let mut areas = SHARD_AREAS.lock().map(|areas| areas.clone()).unwrap_or_default();
    areas.sort_unstable_by_key(|area| std::cmp::Reverse(area.cells));
    areas
}

/// Log how full each shard of the RISC-V machine actually was, plus a count of the fixed-shape
/// recursion shards proved on top of them, and warn about any shard neither set accounts for.
pub(crate) fn log_shard_areas() {
    let areas = shard_areas();
    if areas.is_empty() {
        return;
    }
    let of_kind = |kind| areas.iter().filter(move |area| area.kind == kind);
    let budget = env_u64("ELEMENT_THRESHOLD", DEFAULT_SHARD_AREA);

    tracing::info!("Core shards proved: {} (fullest first)", of_kind(ShardKind::Core).count());
    for area in of_kind(ShardKind::Core) {
        tracing::info!(
            "  {:.1}% full: {} of {} trace cells",
            (area.cells as f64 * 100.0) / budget as f64,
            area.cells,
            budget
        );
    }

    let recursion = of_kind(ShardKind::Recursion).count();
    if recursion > 0 {
        tracing::info!("Recursion shards proved: {}", recursion);
    }

    let unknown = of_kind(ShardKind::Unknown).count();
    if unknown > 0 {
        tracing::warn!(
            "{unknown} shards left unattributed: chip lists point to both machines, so SP1's chip \
             sets no longer match RECURSION_CHIPS. Run scripts/check-sp1-chip-sets.sh. Chips on \
             both sides: {}",
            conflicting_chips().join(", ")
        );
        for area in of_kind(ShardKind::Unknown) {
            tracing::warn!("  unattributed: {} trace cells", area.cells);
        }
    }
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

/// Log a guest execution's instruction count, prover gas, estimated shard fill, and hottest
/// cycle-tracker spans. The fill is an estimate because it runs before proving, from gas alone;
/// [`log_shard_areas`] reports what the prover measured afterwards.
pub fn log_execution_report(report: &ExecutionReport, total_proving_time_secs: f64) {
    tracing::info!("Execution report");
    tracing::info!("  total instructions: {}", report.total_instruction_count());

    let prover_gas = report.gas().unwrap_or(0);
    tracing::info!("  prover gas: {}", prover_gas);

    if prover_gas > 0 {
        let budget = env_u64("ZKPOW_GAS_PER_SHARD", DEFAULT_GAS_PER_SHARD);
        tracing::info!(
            "  estimated shard fill: {:.1}% of one shard ({} of {} gas), estimated shards: {}",
            (prover_gas as f64 * 100.0) / budget as f64,
            prover_gas,
            budget,
            prover_gas.div_ceil(budget)
        );
    }

    if report.cycle_tracker.is_empty() {
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
