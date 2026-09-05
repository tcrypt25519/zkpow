use std::{sync::Arc, time::Instant};

use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};

pub const MAGIC: &[u8; 4] = b"ZKVT";
pub const CMD_LIST: u8 = 0;
pub const CMD_RUN: u8 = 1;

pub const FLAG_IGNORE: u8 = 1 << 0;
pub const FLAG_SHOULD_PANIC: u8 = 1 << 1;
pub const FLAG_EXPECTED_EXIT_CODE: u8 = 1 << 2;
pub const FLAG_MAX_CYCLES: u8 = 1 << 3;
pub const FLAG_FIXTURE: u8 = 1 << 4;
pub const FLAG_SNAPSHOT: u8 = 1 << 5;

pub const METRICS_DIR: &str = ".zkvm-test-metrics";
pub const DEFAULT_BASELINE_NAME: &str = "default";
pub const DEFAULT_PROVER: &str = "mock";
pub const PROVER_ENV_VAR: &str = "ZKVM_PROVER";
const DEFAULT_CONCURRENCY: usize = 4;

#[derive(Debug, Clone)]
pub struct TestDesc {
    pub name:               String,
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_exit_code: Option<u32>,
    pub max_cycles:         Option<u64>,
    pub fixture:            Option<String>,
    pub snapshot:           Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProofMode {
    #[default]
    None,
    Core,
    Compressed,
    Plonk,
    Groth16,
}

impl std::str::FromStr for ProofMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" | "mock" | "execute" => Ok(ProofMode::None),
            "core" => Ok(ProofMode::Core),
            "compressed" => Ok(ProofMode::Compressed),
            "plonk" => Ok(ProofMode::Plonk),
            "groth16" => Ok(ProofMode::Groth16),
            other => Err(format!(
                "unknown proof mode '{other}'. Expected 'none', 'core', 'compressed', 'plonk', or 'groth16'"
            )),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnerArgs {
    pub elf_path:           String,
    pub filters:            Vec<String>,
    pub list:               bool,
    pub exact:              bool,
    pub ignored:            bool,
    pub include_ignored:    bool,
    pub nocapture:          bool,
    pub quiet:              bool,
    pub profile:            bool,
    pub instructions:       bool,
    pub mode:               Option<String>,
    pub test_threads:       usize,
    pub prover:             Option<String>,
    pub save_baseline:      Option<String>,
    pub compare_baseline:   Option<String>,
    pub fail_on_regression: Option<f64>,
    pub accept_snapshots:   bool,
    pub filter_expr:        Option<String>,
    pub junit_path:         Option<String>,
    pub package_id:         Option<String>,
    pub target_name:        Option<String>,
    pub target_kind:        Option<String>,
    pub package_graph:      Option<Arc<guppy::graph::PackageGraph>>,
}

pub fn parse_args_from<I, T>(iter: I) -> RunnerArgs
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let mut raw_args = iter.into_iter().map(Into::into);
    let mut args = RunnerArgs {
        test_threads: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(DEFAULT_CONCURRENCY),
        ..Default::default()
    };

    let Some(elf) = raw_args.next() else {
        eprintln!("usage: zkvm-test-runner <elf-path> [filters/flags...]");
        std::process::exit(2);
    };
    args.elf_path = elf;

    let mut iter = raw_args.peekable();
    while let Some(arg) = iter.next() {
        let (key, mut inline_val) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        let mut next_val = || inline_val.take().or_else(|| iter.next());

        match key {
            "--list" => args.list = true,
            "--exact" => args.exact = true,
            "--ignored" => args.ignored = true,
            "--include-ignored" => args.include_ignored = true,
            "--nocapture" | "--no-capture" => args.nocapture = true,
            "--profile" => args.profile = true,
            "--instructions" | "--opcodes" => args.instructions = true,
            "--mode" | "--proof-mode" => {
                args.mode = next_val();
            },
            "-E" | "--expr" | "--filter-expr" => {
                args.filter_expr = next_val();
            },
            "--junit-path" => {
                args.junit_path = next_val();
            },
            "--package-id" => {
                args.package_id = next_val();
            },
            "--target-name" => {
                args.target_name = next_val();
            },
            "--target-kind" => {
                args.target_kind = next_val();
            },
            "--accept-snapshots" | "--update-snapshots" => args.accept_snapshots = true,
            "-q" | "--quiet" => args.quiet = true,
            "--test-threads" => {
                if let Some(val) = next_val() {
                    if let Ok(n) = val.parse::<usize>() {
                        args.test_threads = n.max(1);
                    }
                }
            },
            "--prover" => {
                args.prover = next_val();
            },
            "--save-baseline" => {
                args.save_baseline = inline_val.or_else(|| {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            return iter.next();
                        }
                    }
                    Some(DEFAULT_BASELINE_NAME.to_string())
                });
            },
            "--compare-baseline" => {
                args.compare_baseline = next_val();
            },
            "--fail-on-regression" => {
                if let Some(val) = next_val() {
                    let cleaned = val.trim_end_matches('%');
                    if let Ok(pct) = cleaned.parse::<f64>() {
                        args.fail_on_regression = Some(pct);
                    }
                }
            },
            "--color" => {
                let _ = next_val();
            },
            s if s.starts_with("--format") || s.starts_with("-Z") || s == "--show-output" => {
                // Ignore standard libtest formatting flags
            },
            s if s.starts_with('-') => {
                // Ignore unknown flags for forward compatibility with cargo
            },
            _ => {
                args.filters.push(arg);
            },
        }
    }

    if args.nocapture {
        args.test_threads = 1;
    }

    args
}

pub struct ByteReader<'a> {
    data:   &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub const fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        let val = *self.data.get(self.offset)?;
        self.offset += 1;
        Some(val)
    }

    pub fn read_u32_le(&mut self) -> Option<u32> {
        let slice = self.data.get(self.offset..self.offset + 4)?;
        self.offset += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    }

    pub fn read_u64_le(&mut self) -> Option<u64> {
        let slice = self.data.get(self.offset..self.offset + 8)?;
        self.offset += 8;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    }

    pub fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        let slice = self.data.get(self.offset..self.offset + len)?;
        self.offset += len;
        Some(slice)
    }

    pub fn read_len_prefixed_str(&mut self) -> Option<String> {
        let len = self.read_u32_le()? as usize;
        let bytes = self.read_bytes(len)?;
        std::str::from_utf8(bytes).ok().map(String::from)
    }

    pub fn read_conditional_u32_le(&mut self, condition: bool) -> Option<Option<u32>> {
        if condition {
            self.read_u32_le().map(Some)
        } else {
            Some(None)
        }
    }

    pub fn read_conditional_u64_le(&mut self, condition: bool) -> Option<Option<u64>> {
        if condition {
            self.read_u64_le().map(Some)
        } else {
            Some(None)
        }
    }

    pub fn read_conditional_len_prefixed_str(&mut self, condition: bool) -> Option<Option<String>> {
        if condition {
            self.read_len_prefixed_str().map(Some)
        } else {
            Some(None)
        }
    }
}

impl TestDesc {
    pub fn decode(reader: &mut ByteReader) -> Option<Self> {
        let flags = reader.read_u8()?;
        let expected_exit_code = reader.read_conditional_u32_le((flags & FLAG_EXPECTED_EXIT_CODE) != 0)?;
        let max_cycles = reader.read_conditional_u64_le((flags & FLAG_MAX_CYCLES) != 0)?;
        let fixture = reader.read_conditional_len_prefixed_str((flags & FLAG_FIXTURE) != 0)?;
        let snapshot = reader.read_conditional_len_prefixed_str((flags & FLAG_SNAPSHOT) != 0)?;
        let name = reader.read_len_prefixed_str()?;

        Some(Self {
            name,
            ignore: (flags & FLAG_IGNORE) != 0,
            should_panic: (flags & FLAG_SHOULD_PANIC) != 0,
            expected_exit_code,
            max_cycles,
            fixture,
            snapshot,
        })
    }
}

pub fn decode_test_list(bytes: &[u8]) -> Option<Vec<TestDesc>> {
    let mut reader = ByteReader::new(bytes);
    let count = reader.read_u32_le()? as usize;
    let mut tests = Vec::with_capacity(count);
    for _ in 0..count {
        tests.push(TestDesc::decode(&mut reader)?);
    }
    Some(tests)
}

fn resolve_resource_path(path_str: &str, elf_path: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(path_str);
    if p.exists() {
        return Some(p.to_path_buf());
    }
    let cwd = std::env::current_dir().ok()?;
    let candidate = cwd.join(path_str);
    if candidate.exists() {
        return Some(candidate);
    }

    let elf_names = elf_path.and_then(|e| e.file_name()).and_then(|n| n.to_str()).map(|file_name| {
        let pkg_ident = file_name.split('-').next().unwrap_or("").to_string();
        let pkg_name = pkg_ident.replace('_', "-");
        let stripped = pkg_name.trim_start_matches("zkpow-").to_string();
        (pkg_ident, pkg_name, stripped)
    });

    let mut curr = cwd.as_path();
    loop {
        let candidate = curr.join(path_str);
        if candidate.exists() {
            return Some(candidate);
        }

        if let Some((ref pkg_ident, ref pkg_name, ref stripped)) = elf_names {
            for base in [curr.join("crates"), curr.to_path_buf()] {
                for dir_name in [pkg_name, pkg_ident, stripped] {
                    let candidate = base.join(dir_name).join(path_str);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }

        match curr.parent() {
            Some(parent) => curr = parent,
            None => break,
        }
    }
    None
}

fn resolve_and_read_fixture(path_str: &str, elf_path: Option<&std::path::Path>) -> Result<Vec<u8>, String> {
    let resolved = resolve_resource_path(path_str, elf_path)
        .ok_or_else(|| format!("fixture file '{path_str}' not found on host filesystem"))?;
    std::fs::read(&resolved).map_err(|e| format!("cannot read {}: {e}", resolved.display()))
}

fn resolve_snapshot_path(path_str: &str, elf_path: Option<&std::path::Path>) -> std::path::PathBuf {
    let p = std::path::Path::new(path_str);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    resolve_resource_path(path_str, elf_path).unwrap_or_else(|| {
        std::env::current_dir()
            .map(|cwd| cwd.join(path_str))
            .unwrap_or_else(|_| p.to_path_buf())
    })
}

fn format_hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(16) {
        for b in chunk {
            out.push_str(&format!("{b:02x} "));
        }
        out.push('\n');
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    Hex,
    Binary,
}

impl SnapshotFormat {
    pub fn from_path(path: &std::path::Path) -> Self {
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("hex")) {
            Self::Hex
        } else {
            Self::Binary
        }
    }

    pub fn format_committed(&self, committed: &[u8]) -> Vec<u8> {
        match self {
            Self::Hex => {
                let mut s = committed.iter().map(|b| format!("{b:02x}")).collect::<String>();
                s.push('\n');
                s.into_bytes()
            },
            Self::Binary => committed.to_vec(),
        }
    }
}

fn verify_or_update_snapshot(
    snapshot_path: &str,
    committed: &[u8],
    accept_snapshots: bool,
    elf_path: Option<&std::path::Path>,
) -> Result<(), String> {
    let resolved = resolve_snapshot_path(snapshot_path, elf_path);
    let format = SnapshotFormat::from_path(&resolved);

    let write_snapshot = |data: &[u8], action: &str| -> Result<(), String> {
        if let Some(parent) = resolved.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&resolved, data)
            .map_err(|e| format!("failed to {action} snapshot '{}': {e}", resolved.display()))?;
        println!("  [SNAPSHOT {}: {}]", action.to_uppercase(), resolved.display());
        Ok(())
    };

    if !resolved.exists() {
        if accept_snapshots {
            return write_snapshot(&format.format_committed(committed), "created");
        } else {
            return Err(format!(
                "snapshot file '{}' does not exist. Run with '--accept-snapshots' to create it.",
                resolved.display()
            ));
        }
    }

    match format {
        SnapshotFormat::Hex => {
            let content = std::fs::read_to_string(&resolved)
                .map_err(|e| format!("failed to read snapshot '{}': {e}", resolved.display()))?;
            let expected_hex = content.trim().to_ascii_lowercase();
            let actual_hex = committed.iter().map(|b| format!("{b:02x}")).collect::<String>();
            if expected_hex != actual_hex {
                if accept_snapshots {
                    write_snapshot(&format.format_committed(committed), "updated")?;
                } else {
                    return Err(format!(
                        "snapshot mismatch for '{}':\n  - expected ({} bytes):\n    {}\n  + actual ({} bytes):\n    {}\nTo accept these changes, run with '--accept-snapshots'.",
                        resolved.display(),
                        expected_hex.len() / 2,
                        expected_hex,
                        committed.len(),
                        actual_hex
                    ));
                }
            }
        },
        SnapshotFormat::Binary => {
            let expected_bytes = std::fs::read(&resolved)
                .map_err(|e| format!("failed to read snapshot '{}': {e}", resolved.display()))?;
            if expected_bytes != committed {
                if accept_snapshots {
                    write_snapshot(committed, "updated")?;
                } else {
                    return Err(format!(
                        "snapshot mismatch for '{}':\n  - expected ({} bytes):\n{}\n  + actual ({} bytes):\n{}\nTo accept these changes, run with '--accept-snapshots'.",
                        resolved.display(),
                        expected_bytes.len(),
                        format_hex_dump(&expected_bytes),
                        committed.len(),
                        format_hex_dump(committed)
                    ));
                }
            }
        },
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionCategory {
    Alu,
    MulDiv,
    Memory,
    ControlFlow,
    System,
}

impl InstructionCategory {
    pub const fn from_opcode(opcode: sp1_core_executor::Opcode) -> Self {
        use sp1_core_executor::Opcode;
        match opcode {
            Opcode::ADD
            | Opcode::ADDI
            | Opcode::SUB
            | Opcode::XOR
            | Opcode::OR
            | Opcode::AND
            | Opcode::SLL
            | Opcode::SRL
            | Opcode::SRA
            | Opcode::SLT
            | Opcode::SLTU
            | Opcode::ADDW
            | Opcode::SUBW
            | Opcode::SLLW
            | Opcode::SRLW
            | Opcode::SRAW
            | Opcode::AUIPC
            | Opcode::LUI => Self::Alu,
            Opcode::MUL
            | Opcode::MULH
            | Opcode::MULHU
            | Opcode::MULHSU
            | Opcode::DIV
            | Opcode::DIVU
            | Opcode::REM
            | Opcode::REMU
            | Opcode::MULW
            | Opcode::DIVW
            | Opcode::DIVUW
            | Opcode::REMW
            | Opcode::REMUW => Self::MulDiv,
            Opcode::LB
            | Opcode::LH
            | Opcode::LW
            | Opcode::LBU
            | Opcode::LHU
            | Opcode::LWU
            | Opcode::LD
            | Opcode::SB
            | Opcode::SH
            | Opcode::SW
            | Opcode::SD => Self::Memory,
            Opcode::BEQ
            | Opcode::BNE
            | Opcode::BLT
            | Opcode::BGE
            | Opcode::BLTU
            | Opcode::BGEU
            | Opcode::JAL
            | Opcode::JALR => Self::ControlFlow,
            Opcode::ECALL | Opcode::EBREAK | Opcode::UNIMP => Self::System,
        }
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstructionBreakdown {
    pub alu:          u64,
    pub mul_div:      u64,
    pub memory:       u64,
    pub control_flow: u64,
    pub system:       u64,
    pub syscalls:     u64,
}

impl InstructionBreakdown {
    pub fn from_report(report: &sp1_sdk::ExecutionReport) -> Self {
        let mut b = Self::default();
        for (opcode, count) in report.opcode_counts.iter() {
            if *count == 0 {
                continue;
            }
            match InstructionCategory::from_opcode(opcode) {
                InstructionCategory::Alu => b.alu += *count,
                InstructionCategory::MulDiv => b.mul_div += *count,
                InstructionCategory::Memory => b.memory += *count,
                InstructionCategory::ControlFlow => b.control_flow += *count,
                InstructionCategory::System => b.system += *count,
            }
        }
        b.syscalls = report.total_syscall_count();
        b
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestMetric {
    pub cycles:                u64,
    pub record_bytes:          u64,
    pub cycle_tracker:         std::collections::BTreeMap<String, u64>,
    pub invocations:           std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub instruction_breakdown: Option<InstructionBreakdown>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaselineSuite {
    pub created_at:   String,
    pub total_cycles: u64,
    pub tests:        std::collections::BTreeMap<String, TestMetric>,
}

pub struct TestOutcome {
    pub desc:                  TestDesc,
    pub passed:                bool,
    pub cycles:                u64,
    pub failure_reason:        Option<String>,
    pub stdout:                String,
    pub stderr:                String,
    pub record_bytes:          u64,
    pub cycle_tracker:         std::collections::BTreeMap<String, u64>,
    pub invocations:           std::collections::BTreeMap<String, u64>,
    pub instruction_breakdown: Option<InstructionBreakdown>,
    pub top_syscalls:          Vec<(String, u64)>,
    pub proof_duration:        Option<std::time::Duration>,
    pub duration:              std::time::Duration,
}

#[derive(Clone)]
pub enum AnyProver {
    Mock(sp1_sdk::MockProver),
    Cpu(sp1_sdk::CpuProver),
    #[cfg(feature = "cuda")]
    Cuda(sp1_sdk::CudaProver),
}

impl AnyProver {
    pub async fn from_choice(choice: &str) -> Result<Self, String> {
        match choice.to_lowercase().as_str() {
            "mock" => Ok(AnyProver::Mock(ProverClient::builder().mock().build().await)),
            "cpu" => Ok(AnyProver::Cpu(ProverClient::builder().cpu().build().await)),
            #[cfg(feature = "cuda")]
            "cuda" => Ok(AnyProver::Cuda(ProverClient::builder().cuda().build().await)),
            #[cfg(not(feature = "cuda"))]
            "cuda" => Err("CUDA prover requested, but zkvm-test-runner was compiled without the 'cuda' feature".to_string()),
            other => Err(format!("unknown prover '{other}'. Expected 'mock', 'cpu', or 'cuda'.")),
        }
    }

    pub async fn execute_test(
        &self,
        elf: Elf,
        stdin: SP1Stdin,
        stdout_tx: tokio::sync::watch::Sender<String>,
        stderr_tx: tokio::sync::watch::Sender<String>,
    ) -> Result<(sp1_sdk::SP1PublicValues, sp1_sdk::ExecutionReport), sp1_core_executor::ExecutionError> {
        match self {
            AnyProver::Mock(p) => p.execute(elf, stdin).stdout(stdout_tx).stderr(stderr_tx).await,
            AnyProver::Cpu(p) => p.execute(elf, stdin).stdout(stdout_tx).stderr(stderr_tx).await,
            #[cfg(feature = "cuda")]
            AnyProver::Cuda(p) => p.execute(elf, stdin).stdout(stdout_tx).stderr(stderr_tx).await,
        }
    }

    pub async fn setup_pk(&self, elf: Elf) -> Result<sp1_sdk::SP1ProvingKey, String> {
        match self {
            AnyProver::Mock(p) => p.setup(elf).await.map_err(|e| format!("mock setup error: {e}")),
            AnyProver::Cpu(p) => p.setup(elf).await.map_err(|e| format!("cpu setup error: {e}")),
            #[cfg(feature = "cuda")]
            AnyProver::Cuda(p) => p.setup(elf).await.map_err(|e| format!("cuda setup error: {e}")),
        }
    }

    pub async fn prove_test(
        &self,
        pk: &sp1_sdk::SP1ProvingKey,
        stdin: SP1Stdin,
        mode: ProofMode,
    ) -> Result<sp1_sdk::SP1ProofWithPublicValues, String> {
        match self {
            AnyProver::Mock(p) => {
                let req = p.prove(pk, stdin);
                let proof = match mode {
                    ProofMode::None => return Err("cannot prove in mode 'none'".to_string()),
                    ProofMode::Core => req.core().await.map_err(|e| format!("prove core error: {e}"))?,
                    ProofMode::Compressed => req.compressed().await.map_err(|e| format!("prove compressed error: {e}"))?,
                    ProofMode::Plonk => req.plonk().await.map_err(|e| format!("prove plonk error: {e}"))?,
                    ProofMode::Groth16 => req.groth16().await.map_err(|e| format!("prove groth16 error: {e}"))?,
                };
                p.verify(&proof, pk.verifying_key(), None).map_err(|e| format!("proof verification error: {e}"))?;
                Ok(proof)
            }
            AnyProver::Cpu(p) => {
                let req = p.prove(pk, stdin);
                let proof = match mode {
                    ProofMode::None => return Err("cannot prove in mode 'none'".to_string()),
                    ProofMode::Core => req.core().await.map_err(|e| format!("prove core error: {e}"))?,
                    ProofMode::Compressed => req.compressed().await.map_err(|e| format!("prove compressed error: {e}"))?,
                    ProofMode::Plonk => req.plonk().await.map_err(|e| format!("prove plonk error: {e}"))?,
                    ProofMode::Groth16 => req.groth16().await.map_err(|e| format!("prove groth16 error: {e}"))?,
                };
                p.verify(&proof, pk.verifying_key(), None).map_err(|e| format!("proof verification error: {e}"))?;
                Ok(proof)
            }
            #[cfg(feature = "cuda")]
            AnyProver::Cuda(p) => {
                let req = p.prove(pk, stdin);
                let proof = match mode {
                    ProofMode::None => return Err("cannot prove in mode 'none'".to_string()),
                    ProofMode::Core => req.core().await.map_err(|e| format!("prove core error: {e}"))?,
                    ProofMode::Compressed => req.compressed().await.map_err(|e| format!("prove compressed error: {e}"))?,
                    ProofMode::Plonk => req.plonk().await.map_err(|e| format!("prove plonk error: {e}"))?,
                    ProofMode::Groth16 => req.groth16().await.map_err(|e| format!("prove groth16 error: {e}"))?,
                };
                p.verify(&proof, pk.verifying_key(), None).map_err(|e| format!("proof verification error: {e}"))?;
                Ok(proof)
            }
        }
    }
}

pub fn format_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let offset = s.len() % 3;
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (i % 3 == offset || (offset == 0 && i % 3 == 0)) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn print_profile_breakdown(
    cycle_tracker: &std::collections::BTreeMap<String, u64>,
    invocations: &std::collections::BTreeMap<String, u64>,
    total_cycles: u64,
) {
    if cycle_tracker.is_empty() {
        return;
    }
    println!("    Sub-Routine Cycles:");
    let mut entries: Vec<_> = cycle_tracker.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (i, (label, count)) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;
        let branch = if is_last { "└── " } else { "├── " };
        let pct = if total_cycles > 0 {
            (**count as f64 / total_cycles as f64) * 100.0
        } else {
            0.0
        };
        let inv_str = if let Some(inv) = invocations.get(*label) {
            format!(" [{} call{}]", inv, if *inv == 1 { "" } else { "s" })
        } else {
            String::new()
        };
        println!(
            "        {branch}{}: {} cycles ({:.1}%){inv_str}",
            label,
            format_commas(**count),
            pct
        );
    }
}

fn print_instruction_breakdown(b: &InstructionBreakdown, total_cycles: u64) {
    println!("    Instruction Categories:");
    let categories = [
        ("ALU (arithmetic/logic)", b.alu),
        ("Memory (load/store)", b.memory),
        ("Control (branch/jump)", b.control_flow),
        ("Math (mul/div/rem)", b.mul_div),
        ("System (ecall/trap)", b.system),
    ];
    let non_zero: Vec<_> = categories.into_iter().filter(|(_, c)| *c > 0).collect();
    for (i, (name, count)) in non_zero.iter().enumerate() {
        let is_last = i == non_zero.len() - 1;
        let branch = if is_last { "└── " } else { "├── " };
        let pct = if total_cycles > 0 {
            (*count as f64 / total_cycles as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "        {branch}{name:24} : {:>10} instructions ({:>5.1}%)",
            format_commas(*count),
            pct
        );
    }
}

fn print_top_syscalls(syscalls: &[(String, u64)]) {
    println!("    Syscalls & Precompiles:");
    for (i, (name, count)) in syscalls.iter().take(5).enumerate() {
        let is_last = i == syscalls.len().min(5) - 1;
        let branch = if is_last { "└── " } else { "├── " };
        println!("        {branch}{name:24} : {:>10} invocations", format_commas(*count));
    }
}

fn print_failures(failures: &[&TestOutcome]) {
    if failures.is_empty() {
        return;
    }
    println!("\nfailures:\n");
    for f in failures {
        println!("---- {} ----", f.desc.name);
        if let Some(reason) = &f.failure_reason {
            println!("{reason}");
        }
        if !f.stdout.is_empty() {
            println!("\n---- {} stdout ----", f.desc.name);
            print!("{}", f.stdout);
            if !f.stdout.ends_with('\n') {
                println!();
            }
        }
        if !f.stderr.is_empty() {
            println!("\n---- {} stderr ----", f.desc.name);
            print!("{}", f.stderr);
            if !f.stderr.ends_with('\n') {
                println!();
            }
        }
    }
    println!("\nfailures:");
    for f in failures {
        println!("    {}", f.desc.name);
    }
}

fn compare_against_baseline(
    baseline_name: &str,
    outcomes: &[TestOutcome],
    threshold: Option<f64>,
    is_explicit: bool,
) -> bool {
    let path = format!("{METRICS_DIR}/{baseline_name}.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        if is_explicit {
            eprintln!("zkvm-test-runner: failed to read baseline from {path}");
        }
        return false;
    };
    let Ok(baseline) = serde_json::from_str::<BaselineSuite>(&content) else {
        if is_explicit {
            eprintln!("zkvm-test-runner: failed to parse baseline from {path}");
        }
        return false;
    };

    let matching_count = outcomes
        .iter()
        .filter(|o| baseline.tests.contains_key(&o.desc.name))
        .count();
    if matching_count == 0 {
        if is_explicit {
            println!("\nBaseline comparison against '{baseline_name}': no matching tests found.");
        }
        return false;
    }

    println!("\nBaseline comparison against '{baseline_name}':");
    println!("{:<60} {:>14} {:>14} {:>14}", "Test Name", "Baseline", "Current", "Change");
    println!("{}", "-".repeat(106));

    let mut regression_detected = false;
    for o in outcomes {
        let Some(base_metric) = baseline.tests.get(&o.desc.name) else {
            continue;
        };
        let diff = (o.cycles as i64) - (base_metric.cycles as i64);
        let pct = if base_metric.cycles > 0 {
            (diff as f64 / base_metric.cycles as f64) * 100.0
        } else {
            0.0
        };
        let sign = if diff > 0 { "+" } else { "" };
        let is_regression = threshold.is_some_and(|t| pct > t);
        let flag = if is_regression {
            regression_detected = true;
            " [REGRESSION]"
        } else {
            ""
        };
        let change_str = format!("{sign}{pct:.1}%");
        println!(
            "{:<60} {:>14} {:>14} {:>14}{}",
            o.desc.name,
            format_commas(base_metric.cycles),
            format_commas(o.cycles),
            change_str,
            flag
        );
    }
    println!("{}", "-".repeat(106));
    regression_detected
}

fn save_metrics_baseline(save_name: &str, outcomes: &[TestOutcome], total_cycles: u64) {
    let _ = std::fs::create_dir_all(METRICS_DIR);
    let path = format!("{METRICS_DIR}/{save_name}.json");

    let mut tests_map = std::collections::BTreeMap::new();
    for o in outcomes {
        tests_map.insert(
            o.desc.name.clone(),
            TestMetric {
                cycles:                o.cycles,
                record_bytes:          o.record_bytes,
                cycle_tracker:         o.cycle_tracker.clone(),
                invocations:           o.invocations.clone(),
                instruction_breakdown: o.instruction_breakdown.clone(),
            },
        );
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let suite = BaselineSuite {
        created_at: format!("{now}"),
        total_cycles,
        tests: tests_map,
    };

    let Ok(json) = serde_json::to_string_pretty(&suite) else {
        eprintln!("zkvm-test-runner: failed to serialize baseline metrics");
        return;
    };
    if std::fs::write(&path, json).is_ok() {
        println!(
            "Saved metrics baseline to {path} ({} tests, {} total cycles)",
            outcomes.len(),
            format_commas(total_cycles)
        );
    } else {
        eprintln!("zkvm-test-runner: failed to write baseline to {path}");
    }
}

pub async fn run(args: RunnerArgs) -> i32 {
    let elf_bytes = match std::fs::read(&args.elf_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("zkvm-test-runner: failed to read {}: {err}", args.elf_path);
            return 2;
        },
    };

    let prover_choice = args
        .prover
        .clone()
        .or_else(|| std::env::var(PROVER_ENV_VAR).ok())
        .unwrap_or_else(|| DEFAULT_PROVER.to_string());

    let prover = match AnyProver::from_choice(&prover_choice).await {
        Ok(p) => Arc::new(p),
        Err(err) => {
            eprintln!("zkvm-test-runner: {err}");
            return 2;
        },
    };

    // 1. Query test catalog (Tier 2 LIST command)
    let mut list_stdin = SP1Stdin::new();
    let mut list_payload = Vec::new();
    list_payload.extend_from_slice(MAGIC);
    list_payload.push(CMD_LIST);
    list_stdin.write_slice(&list_payload);

    let (list_tx, _list_rx) = tokio::sync::watch::channel(String::new());
    let (list_err_tx, _list_err_rx) = tokio::sync::watch::channel(String::new());
    let list_res = prover.execute_test(Elf::from(elf_bytes.clone()), list_stdin, list_tx, list_err_tx).await;

    let test_catalog = match list_res {
        Ok((public_values, report)) if report.exit_code == 0 => decode_test_list(public_values.as_slice()),
        _ => None,
    };

    let all_tests = match test_catalog {
        Some(tests) => tests,
        None => {
            // Tier 1 Fallback: run the ELF once directly with empty stdin and forward output.
            let (stdout_tx, stdout_rx) = tokio::sync::watch::channel(String::new());
            let (stderr_tx, stderr_rx) = tokio::sync::watch::channel(String::new());

            let execute_res = prover
                .execute_test(Elf::from(elf_bytes), SP1Stdin::new(), stdout_tx, stderr_tx)
                .await;

            let stdout = stdout_rx.borrow().clone();
            let stderr = stderr_rx.borrow().clone();
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            let (_pv, report) = match execute_res {
                Ok(res) => res,
                Err(err) => {
                    eprintln!("zkvm-test-runner: execution failed: {err}");
                    return 1;
                },
            };
            let exit_code = u8::try_from(report.exit_code).unwrap_or(255);
            return exit_code.into();
        },
    };

    let raw_elf_name = std::path::Path::new(&args.elf_path)
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|name| name.split('-').next())
        .unwrap_or("");

    let pkg_name = args.target_name.clone().unwrap_or_else(|| raw_elf_name.to_string());

    let filter_context = if let Some(ref expr_str) = args.filter_expr {
        let graph = if let Some(ref g) = args.package_graph {
            g.clone()
        } else {
            let mut cmd = guppy::MetadataCommand::new();
            match guppy::graph::PackageGraph::from_command(&mut cmd) {
                Ok(g) => Arc::new(g),
                Err(err) => {
                    eprintln!("zkvm-test-runner: failed to load package graph: {err}");
                    return 2;
                }
            }
        };

        let parse_ctx = nextest_filtering::ParseContext::new(&graph);
        let known_groups = nextest_filtering::KnownGroups::Known {
            custom_groups: std::collections::HashSet::new(),
        };
        let filterset = match nextest_filtering::Filterset::parse(
            expr_str.clone(),
            &parse_ctx,
            nextest_filtering::FiltersetKind::Test,
            &known_groups,
        ) {
            Ok(f) => f,
            Err(err) => {
                let err_msgs = err
                    .errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                eprintln!("zkvm-test-runner: invalid filter expression '{}': {err_msgs}", err.input);
                return 2;
            }
        };

        let pkg = if let Some(ref pid) = args.package_id {
            graph.metadata(&guppy::PackageId::new(pid.clone())).ok()
        } else {
            graph.workspace().iter().find(|p| {
                p.name() == raw_elf_name || p.name().replace('-', "_") == raw_elf_name
            })
        };

        let Some(pkg) = pkg else {
            eprintln!("zkvm-test-runner: failed to resolve package for ELF '{}'", args.elf_path);
            return 2;
        };

        let target_name = args.target_name.clone().unwrap_or_else(|| {
            raw_elf_name.to_string()
        });
        let kind = nextest_metadata::RustTestBinaryKind::new(
            args.target_kind.clone().unwrap_or_else(|| "test".to_string()),
        );
        let binary_id = nextest_metadata::RustBinaryId::from_parts(pkg.name(), &kind, &target_name);

        Some((filterset, pkg.id().clone(), binary_id, target_name, kind))
    } else {
        None
    };

    // 2. Filter tests
    let matches_filter = |name: &str| -> bool {
        let pos_match = if args.filters.is_empty() {
            true
        } else if args.exact {
            args.filters.iter().any(|f| f == name)
        } else {
            args.filters.iter().any(|f| name.contains(f))
        };

        if !pos_match {
            return false;
        }

        if let Some((ref filterset, ref pkg_id, ref binary_id, ref target_name, ref kind)) = filter_context {
            let binary_query = nextest_filtering::BinaryQuery {
                package_id: pkg_id,
                binary_id,
                binary_name: target_name,
                kind,
                platform: guppy::graph::cargo::BuildPlatform::Target,
            };
            let eval_cx = nextest_filtering::EvalContext {
                default_filter: &nextest_filtering::CompiledExpr::ALL,
            };
            let test_name_smol = nextest_metadata::TestCaseName::new(name);
            let test_query = nextest_filtering::TestQuery {
                binary_query,
                test_name: &test_name_smol,
            };
            filterset.matches_test(&test_query, &eval_cx)
        } else {
            true
        }
    };

    if args.list {
        let mut count = 0;
        for t in &all_tests {
            if matches_filter(&t.name) {
                println!("{}: test", t.name);
                count += 1;
            }
        }
        println!("\n{count} tests, 0 benchmarks");
        return 0;
    }

    let mut tests_to_run = Vec::new();
    let mut ignored_count = 0;
    let mut filtered_out = 0;

    for t in &all_tests {
        if !matches_filter(&t.name) {
            filtered_out += 1;
            continue;
        }

        if t.ignore && !args.ignored && !args.include_ignored {
            ignored_count += 1;
            if !args.quiet {
                println!("test {} ... ignored", t.name);
            }
            continue;
        }

        if !t.ignore && args.ignored {
            filtered_out += 1;
            continue;
        }

        tests_to_run.push(t.clone());
    }

    let total_count = tests_to_run.len();
    if !args.quiet {
        println!(
            "\nrunning {total_count} test{} (prover: {prover_choice}, threads: {})",
            if total_count == 1 { "" } else { "s" },
            args.test_threads
        );
    }

    let proof_mode: ProofMode = match args.mode.as_deref() {
        Some(s) => match s.parse::<ProofMode>() {
            Ok(m) => m,
            Err(err) => {
                eprintln!("zkvm-test-runner: {err}");
                return 2;
            },
        },
        None => ProofMode::None,
    };

    let pk = if proof_mode != ProofMode::None && !tests_to_run.is_empty() {
        if !args.quiet {
            println!("Setting up proving key for {proof_mode:?} mode...");
        }
        let setup_start = Instant::now();
        match prover.setup_pk(Elf::from(elf_bytes.clone())).await {
            Ok(pk) => {
                if !args.quiet {
                    println!("Proving key ready in {:.2}s", setup_start.elapsed().as_secs_f64());
                }
                Some(Arc::new(pk))
            },
            Err(err) => {
                eprintln!("zkvm-test-runner: setup failed: {err}");
                return 1;
            },
        }
    } else {
        None
    };

    let start_time = Instant::now();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(args.test_threads));
    let mut handles = Vec::new();

    let peak_rss_kb = Arc::new(std::sync::atomic::AtomicU64::new(
        memuse::rss_kb().unwrap_or(0),
    ));
    let elf_path_buf = Arc::new(std::path::PathBuf::from(&args.elf_path));

    for test in tests_to_run {
        let sem = semaphore.clone();
        let prover = prover.clone();
        let pk = pk.clone();
        let elf_bytes = elf_bytes.clone();
        let elf_path = elf_path_buf.clone();
        let quiet = args.quiet;
        let profile = args.profile;
        let instructions = args.instructions;
        let nocapture = args.nocapture;
        let accept_snapshots = args.accept_snapshots;
        let peak_rss = peak_rss_kb.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let test_start = Instant::now();

            let mut run_stdin = SP1Stdin::new();
            let mut payload = Vec::new();
            payload.extend_from_slice(MAGIC);
            payload.push(CMD_RUN);
            payload.extend_from_slice(test.name.as_bytes());
            run_stdin.write_slice(&payload);

            let mut fixture_bytes_opt = None;
            if let Some(ref fix_path) = test.fixture {
                match resolve_and_read_fixture(fix_path, Some(&elf_path)) {
                    Ok(fix_bytes) => {
                        run_stdin.write_slice(&fix_bytes);
                        fixture_bytes_opt = Some(fix_bytes);
                    },
                    Err(err) => {
                        return TestOutcome {
                            desc: test,
                            passed: false,
                            cycles: 0,
                            record_bytes: 0,
                            failure_reason: Some(format!("failed to load fixture: {err}")),
                            stdout: String::new(),
                            stderr: String::new(),
                            cycle_tracker: std::collections::BTreeMap::new(),
                            invocations: std::collections::BTreeMap::new(),
                            instruction_breakdown: None,
                            top_syscalls: Vec::new(),
                            proof_duration: None,
                            duration: test_start.elapsed(),
                        };
                    },
                }
            }

            let (stdout_tx, stdout_rx) = tokio::sync::watch::channel(String::new());
            let (stderr_tx, stderr_rx) = tokio::sync::watch::channel(String::new());

            let res = prover
                .execute_test(Elf::from(elf_bytes), run_stdin, stdout_tx, stderr_tx)
                .await;

            if let Some(rss) = memuse::rss_kb() {
                peak_rss.fetch_max(rss, std::sync::atomic::Ordering::Relaxed);
            }

            let stdout = stdout_rx.borrow().clone();
            let stderr = stderr_rx.borrow().clone();

            let (mut passed, cycles, mut failure_reason, record_bytes, cycle_tracker, invocations, pv_bytes, breakdown, top_syscalls) = match res {
                Ok((pv, report)) => {
                    let exit_code = report.exit_code;
                    let cycles = report.total_instruction_count();
                    let record_bytes = report.total_record_size();
                    let breakdown = InstructionBreakdown::from_report(&report);
                    let mut top_syscalls: Vec<(String, u64)> = report
                        .syscall_counts
                        .iter()
                        .filter(|(_, count)| **count > 0)
                        .map(|(code, count)| (format!("{code:?}"), *count))
                        .collect();
                    top_syscalls.sort_by(|a, b| b.1.cmp(&a.1));
                    let cycle_tracker: std::collections::BTreeMap<String, u64> =
                        report.cycle_tracker.into_iter().collect();
                    let invocations: std::collections::BTreeMap<String, u64> =
                        report.invocation_tracker.into_iter().collect();

                    let (passed, failure_reason) = if let Some(expected) = test.expected_exit_code {
                        if exit_code == u64::from(expected) {
                            (true, None)
                        } else {
                            (false, Some(format!("guest exited with code {exit_code}, expected {expected}")))
                        }
                    } else if test.should_panic {
                        if exit_code != 0 {
                            (true, None)
                        } else {
                            (
                                false,
                                Some("test did not panic as expected (guest exited with code 0)".to_string()),
                            )
                        }
                    } else if exit_code == 0 {
                        (true, None)
                    } else {
                        (false, Some(format!("guest exited with code {exit_code}")))
                    };

                    (passed, cycles, failure_reason, record_bytes, cycle_tracker, invocations, Some(pv.to_vec()), Some(breakdown), top_syscalls)
                },
                Err(err) => (false, 0, Some(format!("executor error: {err}")), 0, Default::default(), Default::default(), None, None, Vec::new()),
            };

            // Check declarative cycle budget
            if passed {
                if let Some(max) = test.max_cycles && cycles > max {
                    passed = false;
                    failure_reason = Some(format!(
                        "cycle budget exceeded: {} cycles > budget of {}",
                        format_commas(cycles),
                        format_commas(max)
                    ));
                }
            }

            // Check public values snapshot
            if passed {
                if let Some(ref snap_path) = test.snapshot {
                    let bytes = pv_bytes.as_deref().unwrap_or(&[]);
                    if let Err(snap_err) = verify_or_update_snapshot(snap_path, bytes, accept_snapshots, Some(&elf_path)) {
                        passed = false;
                        failure_reason = Some(snap_err);
                    }
                }
            }

            // If proof mode requested, generate and verify ZK proof
            let mut proof_duration = None;
            if passed {
                if let Some(ref pk_arc) = pk {
                    let mut prove_stdin = SP1Stdin::new();
                    prove_stdin.write_slice(&payload);
                    if let Some(ref fix_bytes) = fixture_bytes_opt {
                        prove_stdin.write_slice(fix_bytes);
                    }
                    let p_start = Instant::now();
                    match prover.prove_test(pk_arc, prove_stdin, proof_mode).await {
                        Ok(_) => {
                            proof_duration = Some(p_start.elapsed());
                        },
                        Err(err) => {
                            passed = false;
                            failure_reason = Some(format!("proof generation/verification failed in mode {proof_mode:?}: {err}"));
                        },
                    }
                }
            }

            if !quiet {
                if passed {
                    let mut meta_items = Vec::new();
                    if let Some(expected) = test.expected_exit_code {
                        meta_items.push(format!("exit code: {expected}"));
                    } else if test.should_panic {
                        meta_items.push("should panic".to_string());
                    }
                    if let Some(max) = test.max_cycles {
                        meta_items.push(format!("budget: {}", format_commas(max)));
                    }
                    meta_items.push(format!("{} cycles", format_commas(cycles)));
                    if record_bytes > 0 {
                        meta_items.push(format!("trace: {}", format_bytes(record_bytes)));
                    }
                    if let Some(dur) = proof_duration {
                        meta_items.push(format!("{proof_mode:?} proof: {:.2}s", dur.as_secs_f64()));
                    }

                    println!("test {} ... ok ({})", test.name, meta_items.join(", "));

                    if profile {
                        print_profile_breakdown(&cycle_tracker, &invocations, cycles);
                    }
                    if profile || instructions {
                        if let Some(ref b) = breakdown {
                            print_instruction_breakdown(b, cycles);
                        }
                        if !top_syscalls.is_empty() {
                            print_top_syscalls(&top_syscalls);
                        }
                    }
                } else {
                    println!("test {} ... FAILED", test.name);
                }

                if nocapture {
                    if !stdout.is_empty() {
                        print!("{stdout}");
                    }
                    if !stderr.is_empty() {
                        eprint!("{stderr}");
                    }
                }
            }

            TestOutcome {
                desc: test,
                passed,
                cycles,
                failure_reason,
                stdout,
                stderr,
                record_bytes,
                cycle_tracker,
                invocations,
                instruction_breakdown: breakdown,
                top_syscalls,
                proof_duration,
                duration: test_start.elapsed(),
            }
        });

        handles.push(handle);
    }

    let mut outcomes = Vec::new();
    for h in handles {
        if let Ok(outcome) = h.await {
            outcomes.push(outcome);
        }
    }

    let elapsed = start_time.elapsed();
    let mut passed_count = 0;
    let mut failed_count = 0;
    let mut total_cycles = 0;
    let mut failures = Vec::new();

    for o in &outcomes {
        total_cycles += o.cycles;
        if o.passed {
            passed_count += 1;
        } else {
            failed_count += 1;
            failures.push(o);
        }
    }

    print_failures(&failures);

    let mut regression_failed = false;
    let default_baseline_path = format!("{METRICS_DIR}/{DEFAULT_BASELINE_NAME}.json");
    let (baseline_name, is_explicit) = if let Some(explicit) = &args.compare_baseline {
        (Some(explicit.as_str()), true)
    } else if std::path::Path::new(&default_baseline_path).exists() {
        (Some(DEFAULT_BASELINE_NAME), false)
    } else {
        (None, false)
    };

    if let Some(name) = baseline_name {
        regression_failed = compare_against_baseline(name, &outcomes, args.fail_on_regression, is_explicit);
    }

    if let Some(save_name) = &args.save_baseline {
        save_metrics_baseline(save_name, &outcomes, total_cycles);
    }

    if regression_failed {
        println!("\nerror: performance regression detected exceeding configured threshold!");
    }

    let peak_mb = peak_rss_kb.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1024.0;
    let peak_str = if peak_mb >= 1024.0 {
        format!(", peak host rss: {:.2} GB", peak_mb / 1024.0)
    } else if peak_mb > 0.0 {
        format!(", peak host rss: {:.1} MB", peak_mb)
    } else {
        String::new()
    };

    let status_str = if failed_count == 0 && !regression_failed { "ok" } else { "FAILED" };
    println!(
        "\ntest result: {status_str}. {passed_count} passed; {failed_count} failed; {ignored_count} ignored; \
         0 measured; {filtered_out} filtered out; finished in {:.2}s (total cycles: {}{})\n",
        elapsed.as_secs_f64(),
        format_commas(total_cycles),
        peak_str
    );

    if let Some(ref junit_path) = args.junit_path {
        let suite_name = if !pkg_name.is_empty() {
            pkg_name.to_string()
        } else {
            "zkvm-test".to_string()
        };

        let mut suite = crate::junit::JUnitTestSuite::new(suite_name);
        suite.time = elapsed.as_secs_f64();

        for o in &outcomes {
            let classname = o
                .desc
                .name
                .rsplit_once("::")
                .map(|(c, _)| c)
                .unwrap_or(&o.desc.name)
                .to_string();

            let status = if o.passed {
                crate::junit::TestCaseStatus::Passed
            } else {
                let msg = o
                    .failure_reason
                    .clone()
                    .unwrap_or_else(|| "test failed".to_string());
                let det = if !o.stderr.is_empty() {
                    format!("{msg}\n\nstderr:\n{}", o.stderr)
                } else {
                    msg.clone()
                };
                crate::junit::TestCaseStatus::Failed {
                    message: msg,
                    details: det,
                }
            };

            suite.cases.push(crate::junit::JUnitTestCase {
                name: o.desc.name.clone(),
                classname,
                time: o.duration.as_secs_f64(),
                cycles: o.cycles,
                record_bytes: o.record_bytes,
                status,
                stdout: o.stdout.clone(),
                stderr: o.stderr.clone(),
            });
        }

        let executed_names: std::collections::HashSet<_> = outcomes.iter().map(|o| &o.desc.name).collect();
        for t in &all_tests {
            if !executed_names.contains(&t.name) && matches_filter(&t.name) && t.ignore {
                let classname = t
                    .name
                    .rsplit_once("::")
                    .map(|(c, _)| c)
                    .unwrap_or(&t.name)
                    .to_string();
                suite.cases.push(crate::junit::JUnitTestCase {
                    name: t.name.clone(),
                    classname,
                    time: 0.0,
                    cycles: 0,
                    record_bytes: 0,
                    status: crate::junit::TestCaseStatus::Skipped,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
        }

        let p = std::path::Path::new(junit_path);
        if let Err(e) = crate::junit::write_or_merge_junit_xml(p, suite) {
            eprintln!("zkvm-test-runner: error writing JUnit XML: {e}");
        }
    }

    if failed_count > 0 || regression_failed { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_commas() {
        assert_eq!(format_commas(0), "0");
        assert_eq!(format_commas(999), "999");
        assert_eq!(format_commas(1000), "1,000");
        assert_eq!(format_commas(1234567), "1,234,567");
        assert_eq!(format_commas(1000000000), "1,000,000,000");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_proof_mode_from_str() {
        assert_eq!("none".parse::<ProofMode>().unwrap(), ProofMode::None);
        assert_eq!("mock".parse::<ProofMode>().unwrap(), ProofMode::None);
        assert_eq!("core".parse::<ProofMode>().unwrap(), ProofMode::Core);
        assert_eq!("compressed".parse::<ProofMode>().unwrap(), ProofMode::Compressed);
        assert_eq!("plonk".parse::<ProofMode>().unwrap(), ProofMode::Plonk);
        assert_eq!("groth16".parse::<ProofMode>().unwrap(), ProofMode::Groth16);
        assert!("invalid_mode".parse::<ProofMode>().is_err());
    }

    #[test]
    fn test_parse_args_from() {
        let args = vec![
            "target/zkvm/test-elf".to_string(),
            "--exact".to_string(),
            "--list".to_string(),
            "--ignored".to_string(),
            "--profile".to_string(),
            "--instructions".to_string(),
            "-q".to_string(),
            "--accept-snapshots".to_string(),
            "--mode=core".to_string(),
            "--test-threads=8".to_string(),
            "--prover=cpu".to_string(),
            "-E".to_string(),
            "package(foo)".to_string(),
            "--junit-path=/tmp/junit.xml".to_string(),
            "--package-id=pkg-123".to_string(),
            "--target-name=sha256".to_string(),
            "--target-kind=test".to_string(),
            "test_filter_1".to_string(),
        ];

        let parsed = parse_args_from(args);
        assert_eq!(parsed.elf_path, "target/zkvm/test-elf");
        assert!(!parsed.nocapture);
        assert!(parsed.exact);
        assert!(parsed.list);
        assert!(parsed.ignored);
        assert!(parsed.profile);
        assert!(parsed.instructions);
        assert!(parsed.quiet);
        assert!(parsed.accept_snapshots);
        assert_eq!(parsed.mode.as_deref(), Some("core"));
        assert_eq!(parsed.test_threads, 8);
        assert_eq!(parsed.prover.as_deref(), Some("cpu"));
        assert_eq!(parsed.filter_expr.as_deref(), Some("package(foo)"));
        assert_eq!(parsed.junit_path.as_deref(), Some("/tmp/junit.xml"));
        assert_eq!(parsed.package_id.as_deref(), Some("pkg-123"));
        assert_eq!(parsed.target_name.as_deref(), Some("sha256"));
        assert_eq!(parsed.target_kind.as_deref(), Some("test"));
        assert_eq!(parsed.filters, vec!["test_filter_1"]);

        // When --nocapture is set, test_threads must be forced to 1 to prevent interleaved stdout
        let nocap_args = vec![
            "target/zkvm/test-elf".to_string(),
            "--nocapture".to_string(),
            "--test-threads=8".to_string(),
        ];
        let nocap_parsed = parse_args_from(nocap_args);
        assert!(nocap_parsed.nocapture);
        assert_eq!(nocap_parsed.test_threads, 1);
    }

    #[test]
    fn test_decode_test_list_valid() {
        let mut bytes = Vec::new();
        // Count: 2 tests
        bytes.extend_from_slice(&2u32.to_le_bytes());

        // Test 1: should panic with expected exit code 42
        let flags1 = FLAG_SHOULD_PANIC | FLAG_EXPECTED_EXIT_CODE;
        bytes.push(flags1);
        bytes.extend_from_slice(&42u32.to_le_bytes()); // exit code
        let name1 = "tests::panic_test";
        bytes.extend_from_slice(&(name1.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name1.as_bytes());

        // Test 2: max cycles, fixture, snapshot, ignored
        let flags2 = FLAG_IGNORE | FLAG_MAX_CYCLES | FLAG_FIXTURE | FLAG_SNAPSHOT;
        bytes.push(flags2);
        bytes.extend_from_slice(&1_000_000u64.to_le_bytes()); // max cycles
        let fix = "fixtures/data.bin";
        bytes.extend_from_slice(&(fix.len() as u32).to_le_bytes());
        bytes.extend_from_slice(fix.as_bytes());
        let snap = "snapshots/golden.hex";
        bytes.extend_from_slice(&(snap.len() as u32).to_le_bytes());
        bytes.extend_from_slice(snap.as_bytes());
        let name2 = "tests::snapshot_test";
        bytes.extend_from_slice(&(name2.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name2.as_bytes());

        let decoded = decode_test_list(&bytes).expect("should decode valid list");
        assert_eq!(decoded.len(), 2);

        assert_eq!(decoded[0].name, "tests::panic_test");
        assert!(decoded[0].should_panic);
        assert_eq!(decoded[0].expected_exit_code, Some(42));
        assert!(!decoded[0].ignore);
        assert_eq!(decoded[0].max_cycles, None);
        assert_eq!(decoded[0].fixture, None);
        assert_eq!(decoded[0].snapshot, None);

        assert_eq!(decoded[1].name, "tests::snapshot_test");
        assert!(decoded[1].ignore);
        assert!(!decoded[1].should_panic);
        assert_eq!(decoded[1].max_cycles, Some(1_000_000));
        assert_eq!(decoded[1].fixture.as_deref(), Some("fixtures/data.bin"));
        assert_eq!(decoded[1].snapshot.as_deref(), Some("snapshots/golden.hex"));
    }

    #[test]
    fn test_decode_test_list_invalid() {
        // Empty bytes
        assert!(decode_test_list(&[]).is_none());

        // Truncated bytes: says 5 tests but ends
        let bytes = 5u32.to_le_bytes();
        assert!(decode_test_list(&bytes).is_none());
    }

    #[test]
    fn test_verify_or_update_snapshot_matching() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snap_file = temp_dir.path().join("golden.hex");
        std::fs::write(&snap_file, "deadbeef\n").unwrap();

        let committed = vec![0xde, 0xad, 0xbe, 0xef];
        let res = verify_or_update_snapshot(
            snap_file.to_str().unwrap(),
            &committed,
            false,
            None,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_verify_or_update_snapshot_mismatch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snap_file = temp_dir.path().join("golden.hex");
        std::fs::write(&snap_file, "deadbeef\n").unwrap();

        let committed = vec![0xca, 0xfe, 0xba, 0xbe];
        let res = verify_or_update_snapshot(
            snap_file.to_str().unwrap(),
            &committed,
            false,
            None,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("snapshot mismatch"));
        assert!(err.contains("--accept-snapshots"));
    }

    #[test]
    fn test_verify_or_update_snapshot_accept_update() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snap_file = temp_dir.path().join("golden.hex");
        std::fs::write(&snap_file, "deadbeef\n").unwrap();

        let committed = vec![0xca, 0xfe, 0xba, 0xbe];
        let res = verify_or_update_snapshot(
            snap_file.to_str().unwrap(),
            &committed,
            true, // accept
            None,
        );
        assert!(res.is_ok());

        // Verify updated file on disk
        let updated = std::fs::read_to_string(&snap_file).unwrap();
        assert_eq!(updated.trim(), "cafebabe");
    }
}

