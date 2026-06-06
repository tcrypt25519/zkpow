use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::memory_monitor;
use crate::pipeline::BoxError;
use crate::util;
use sp1_sdk::ExecutionReport;

const ACTUAL_ANSI_START: &str = "\x1b[31m";
const EXPECTED_ANSI_START: &str = "\x1b[32m";
const LEADING_ZERO_ANSI_START: &str = "\x1b[97m";
const ANSI_END: &str = "\x1b[0m";
const BRACKET_START: &str = "[";
const BRACKET_END: &str = "]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HighlightMode {
    Ansi,
    Brackets,
}

fn format_claim_pretty(claim: &util::PublicChainClaim) -> String {
    let mut output = String::new();
    output.push_str("    PublicChainClaim {\n");
    push_claim_field(
        &mut output,
        "genesis_hash",
        &claim.genesis_hash_display_hex(),
    );
    push_claim_field(&mut output, "tip_hash", &claim.tip_hash_display_hex());
    push_claim_field(&mut output, "chain_work", &claim.chain_work_display_hex());
    push_claim_field(&mut output, "height", &claim.height_display_hex());
    output.push_str("    }");
    output
}

fn push_claim_field(output: &mut String, name: &str, value: &str) {
    let prefix = field_prefix(name);
    if value.len() == 64 {
        output.push_str(&prefix);
        output.push_str(&value[..32]);
        output.push('\n');
        output.push_str(&value_prefix(name));
        output.push_str(&value[32..]);
        output.push_str(",\n");
    } else {
        output.push_str(&prefix);
        output.push_str(value);
        output.push_str(",\n");
    }
}

pub(crate) fn format_claim_mismatch(
    actual: &util::PublicChainClaim,
    expected: &util::PublicChainClaim,
) -> String {
    format_claim_mismatch_with_mode(actual, expected, highlight_mode_from_env())
}

pub(crate) fn format_claim_mismatch_with_mode(
    actual: &util::PublicChainClaim,
    expected: &util::PublicChainClaim,
    mode: HighlightMode,
) -> String {
    let mut output = String::new();
    output.push_str("  actual claim:\n");
    output.push_str("    PublicChainClaim {\n");
    push_claim_mismatch_field(
        &mut output,
        "genesis_hash",
        &actual.genesis_hash_display_hex(),
        &expected.genesis_hash_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "tip_hash",
        &actual.tip_hash_display_hex(),
        &expected.tip_hash_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "chain_work",
        &actual.chain_work_display_hex(),
        &expected.chain_work_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "height",
        &actual.height_display_hex(),
        &expected.height_display_hex(),
        mode,
    );
    output.push_str("    }\n");
    output.push_str("  expected claim:\n");
    output.push_str(&format_claim_pretty(expected));
    output
}

fn push_claim_mismatch_field(
    output: &mut String,
    name: &str,
    actual: &str,
    expected: &str,
    mode: HighlightMode,
) {
    if actual.len() == 64 {
        push_wide_claim_mismatch_field(output, name, actual, expected, mode);
    } else {
        let has_mismatch = actual != expected;
        let (actual, expected) =
            highlight_with_expected_mode(expected, actual, mode, actual.len() == 8);
        output.push_str(&field_prefix(name));
        output.push_str(&actual);
        output.push_str(",\n");
        if has_mismatch {
            output.push_str(&value_prefix(name));
            output.push_str(&expected);
            output.push('\n');
        }
    }
}

fn push_wide_claim_mismatch_field(
    output: &mut String,
    name: &str,
    actual: &str,
    expected: &str,
    mode: HighlightMode,
) {
    let first_actual = &actual[..32];
    let first_expected = &expected[..32];
    let second_actual = &actual[32..];
    let second_expected = &expected[32..];
    let first_mismatch = first_actual != first_expected;
    let second_mismatch = second_actual != second_expected;
    let has_mismatch = first_mismatch || second_mismatch;

    let (first_actual, first_expected) =
        highlight_with_expected_mode(first_expected, first_actual, mode, false);
    let (second_actual, second_expected) =
        highlight_with_expected_mode(second_expected, second_actual, mode, false);

    output.push_str(&field_prefix(name));
    output.push_str(&first_actual);
    output.push('\n');
    if first_mismatch {
        output.push_str(&value_prefix(name));
        output.push_str(&first_expected);
        output.push('\n');
    }
    if has_mismatch {
        output.push('\n');
    }
    output.push_str(&value_prefix(name));
    output.push_str(&second_actual);
    output.push_str(",\n");
    if second_mismatch {
        output.push_str(&value_prefix(name));
        output.push_str(&second_expected);
        output.push('\n');
    }
}

pub(crate) fn highlight_with_expected_mode(
    expected: &str,
    actual: &str,
    mode: HighlightMode,
    highlight_leading_zeros: bool,
) -> (String, String) {
    assert_eq!(expected.len(), actual.len());
    assert_eq!(expected.len() % 2, 0);

    let mut actual_out = String::with_capacity(actual.len() * 2);
    let mut expected_out = String::with_capacity(expected.len() * 2);
    let mut in_mismatch_run = false;
    let mut in_leading_zero_run = false;
    let mut still_leading_zeros = highlight_leading_zeros;

    for i in (0..actual.len()).step_by(2) {
        let exp = &expected[i..i + 2];
        let act = &actual[i..i + 2];

        if exp == act {
            if in_mismatch_run {
                actual_out.push_str(mode.end());
                expected_out.push_str(mode.end());
                in_mismatch_run = false;
            }
            if still_leading_zeros && exp == "00" && mode == HighlightMode::Ansi {
                if !in_leading_zero_run {
                    actual_out.push_str(LEADING_ZERO_ANSI_START);
                    in_leading_zero_run = true;
                }
            } else {
                if in_leading_zero_run {
                    actual_out.push_str(ANSI_END);
                    in_leading_zero_run = false;
                }
                still_leading_zeros = false;
            }
            actual_out.push_str(act);
            expected_out.push_str("  ");
        } else {
            if in_leading_zero_run {
                actual_out.push_str(ANSI_END);
                in_leading_zero_run = false;
            }
            still_leading_zeros = false;
            if !in_mismatch_run {
                actual_out.push_str(mode.actual_start());
                expected_out.push_str(mode.expected_start());
                in_mismatch_run = true;
            }
            actual_out.push_str(act);
            expected_out.push_str(exp);
        }
    }

    if in_leading_zero_run {
        actual_out.push_str(ANSI_END);
    }
    if in_mismatch_run {
        actual_out.push_str(mode.end());
        expected_out.push_str(mode.end());
    }

    (actual_out, expected_out)
}

impl HighlightMode {
    fn actual_start(self) -> &'static str {
        match self {
            Self::Ansi => ACTUAL_ANSI_START,
            Self::Brackets => BRACKET_START,
        }
    }

    fn expected_start(self) -> &'static str {
        match self {
            Self::Ansi => EXPECTED_ANSI_START,
            Self::Brackets => BRACKET_START,
        }
    }

    fn end(self) -> &'static str {
        match self {
            Self::Ansi => ANSI_END,
            Self::Brackets => BRACKET_END,
        }
    }
}

fn highlight_mode_from_env() -> HighlightMode {
    match std::env::var_os("CLICOLOR") {
        Some(value) if value.to_string_lossy() != "0" => HighlightMode::Ansi,
        _ => HighlightMode::Brackets,
    }
}

fn field_prefix(name: &str) -> String {
    format!("      {name}: ")
}

pub(crate) fn value_prefix(name: &str) -> String {
    format!("      {}  ", " ".repeat(name.len()))
}

trait ClaimHex {
    fn genesis_hash_display_hex(&self) -> String;
    fn tip_hash_display_hex(&self) -> String;
    fn chain_work_display_hex(&self) -> String;
    fn height_display_hex(&self) -> String;
}

impl ClaimHex for util::PublicChainClaim {
    fn genesis_hash_display_hex(&self) -> String {
        display_hex_32(self.genesis_hash.to_le_bytes())
    }

    fn tip_hash_display_hex(&self) -> String {
        display_hex_32(self.tip_hash.to_le_bytes())
    }

    fn chain_work_display_hex(&self) -> String {
        display_hex_32(self.chain_work.to_le_bytes())
    }

    fn height_display_hex(&self) -> String {
        hex::encode(self.height.to_be_bytes())
    }
}

fn display_hex_32(mut bytes: [u8; 32]) -> String {
    bytes.reverse();
    hex::encode(bytes)
}

#[derive(Debug, Clone)]
pub struct PhaseTiming {
    pub label: String,
    pub total_duration_secs: f64,
    pub invocations: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct PhaseTimingAccum {
    total: Duration,
    invocations: u32,
}

static PHASE_TIMINGS: OnceLock<Mutex<HashMap<&'static str, PhaseTimingAccum>>> = OnceLock::new();

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
                label: (*label).to_string(),
                total_duration_secs: accum.total.as_secs_f64(),
                invocations: accum.invocations,
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
    self_cycles: u64,
    self_invocations: u64,
    total_cycles: u64,
    children: BTreeMap<String, CycleTreeNode>,
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
        node_b
            .total_cycles
            .cmp(&node_a.total_cycles)
            .then_with(|| name_a.cmp(name_b))
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
        let invocations = report
            .invocation_tracker
            .get((*label).as_str())
            .copied()
            .unwrap_or(1);
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
        let invocations = report
            .invocation_tracker
            .get((*label).as_str())
            .copied()
            .unwrap_or(1);
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
    let end_memory = memory_monitor::sample();
    record_phase_timing(label, elapsed);
    memory_monitor::log_delta(
        label,
        start_memory,
        end_memory,
        elapsed,
        "proof phase finished",
    );
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
    let end_memory = memory_monitor::sample();
    record_phase_timing(label, elapsed);
    memory_monitor::log_delta(
        label,
        start_memory,
        end_memory,
        elapsed,
        "proof phase finished",
    );
    output
}
