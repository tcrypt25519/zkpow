use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};
pub use zkvm_test::{
    CMD_LIST, CMD_RUN, FLAG_EXPECTED_EXIT_CODE, FLAG_EXPECTED_MESSAGE, FLAG_FIXTURE, FLAG_IGNORE,
    FLAG_MAX_CYCLES, FLAG_SHOULD_PANIC, FLAG_SNAPSHOT, MAGIC,
};

pub const METRICS_DIR: &str = ".zkvm-test-metrics";
pub const DEFAULT_BASELINE_NAME: &str = "default";
pub const DEFAULT_PROVER: &str = "mock";
pub const PROVER_ENV_VAR: &str = "ZKVM_PROVER";
const DEFAULT_CONCURRENCY: usize = 4;
/// Syscall rows printed per test under `--profile` / `--instructions`.
const TOP_SYSCALL_COUNT: usize = 5;

#[derive(Debug, Clone)]
pub struct TestDesc {
    pub name:               String,
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_message:   Option<String>,
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

pub fn parse_args_from<I, T>(iter: I) -> Result<RunnerArgs, String>
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
        return Err("missing <elf-path>\nusage: zkvm-test-runner <elf-path> [filters/flags...]".to_string());
    };
    args.elf_path = elf;

    let mut iter = raw_args.peekable();
    while let Some(arg) = iter.next() {
        if !arg.starts_with('-') {
            args.filters.push(arg);
            continue;
        }

        let (key, mut inline_val) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        // A value flag takes `--flag=value` or the next argument; running out is an error
        // rather than a silently defaulted flag.
        let mut next_val = || {
            inline_val
                .take()
                .or_else(|| iter.next())
                .ok_or_else(|| format!("{key} expects a value"))
        };

        match key {
            "--list" => args.list = true,
            "--exact" => args.exact = true,
            "--ignored" => args.ignored = true,
            "--include-ignored" => args.include_ignored = true,
            "--nocapture" | "--no-capture" => args.nocapture = true,
            "--profile" | "--subcycles" => args.profile = true,
            "--instructions" | "--opcodes" => args.instructions = true,
            "--accept-snapshots" | "--update-snapshots" => args.accept_snapshots = true,
            "-q" | "--quiet" => args.quiet = true,
            "--mode" | "--proof-mode" => args.mode = Some(next_val()?),
            "-E" | "--expr" | "--filter-expr" => args.filter_expr = Some(next_val()?),
            "--junit-path" => args.junit_path = Some(next_val()?),
            "--package-id" => args.package_id = Some(next_val()?),
            "--target-name" => args.target_name = Some(next_val()?),
            "--target-kind" => args.target_kind = Some(next_val()?),
            "--prover" => args.prover = Some(next_val()?),
            "--compare-baseline" => args.compare_baseline = Some(next_val()?),
            "--test-threads" => {
                let val = next_val()?;
                args.test_threads = match val.parse::<usize>() {
                    Ok(n) if n > 0 => n,
                    _ => return Err(format!("--test-threads expects a positive integer, got '{val}'")),
                };
            },
            "--fail-on-regression" => {
                let val = next_val()?;
                args.fail_on_regression = match val.trim_end_matches('%').parse::<f64>() {
                    Ok(pct) if pct.is_finite() => Some(pct),
                    _ => return Err(format!("--fail-on-regression expects a percentage, got '{val}'")),
                };
            },
            "--save-baseline" => {
                // The name is optional: `--save-baseline` alone saves over the default.
                args.save_baseline = inline_val.or_else(|| {
                    iter.next_if(|next| !next.starts_with('-'))
                        .or_else(|| Some(DEFAULT_BASELINE_NAME.to_string()))
                });
            },
            "--color" => {
                let _ = next_val()?;
            },
            s if s.starts_with("--format") || s.starts_with("-Z") || s == "--show-output" => {
                // Ignore standard libtest formatting flags
            },
            _ => {
                // Ignore unknown flags for forward compatibility with cargo
            },
        }
    }

    if args.nocapture {
        args.test_threads = 1;
    }

    Ok(args)
}

pub struct ByteReader<'a> {
    data:   &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
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
}

impl TestDesc {
    /// Mirror of `zkvm_test::TestCase::encode`; the field order is the wire contract.
    pub fn decode(reader: &mut ByteReader) -> Option<Self> {
        let flags = reader.read_u8()?;
        let present = |flag: u8| (flags & flag) != 0;

        let expected_exit_code = match present(FLAG_EXPECTED_EXIT_CODE) {
            true => Some(reader.read_u32_le()?),
            false => None,
        };
        let max_cycles = match present(FLAG_MAX_CYCLES) {
            true => Some(reader.read_u64_le()?),
            false => None,
        };
        let expected_message = match present(FLAG_EXPECTED_MESSAGE) {
            true => Some(reader.read_len_prefixed_str()?),
            false => None,
        };
        let fixture = match present(FLAG_FIXTURE) {
            true => Some(reader.read_len_prefixed_str()?),
            false => None,
        };
        let snapshot = match present(FLAG_SNAPSHOT) {
            true => Some(reader.read_len_prefixed_str()?),
            false => None,
        };
        let name = reader.read_len_prefixed_str()?;

        Some(Self {
            name,
            ignore: present(FLAG_IGNORE),
            should_panic: present(FLAG_SHOULD_PANIC),
            expected_message,
            expected_exit_code,
            max_cycles,
            fixture,
            snapshot,
        })
    }
}

/// What an ELF answered the `CMD_LIST` command with.
#[derive(Debug)]
pub enum Catalog {
    /// A decodable list of the tests the binary holds.
    Tests(Vec<TestDesc>),
    /// A zkvm-test reply that could not be decoded, which means this runner and the
    /// `zkvm-test` the ELF was built against disagree about the wire format.
    Corrupt,
    /// No zkvm-test reply at all: a stock libtest binary that ignored the command.
    Absent,
}

pub fn decode_test_list(bytes: &[u8]) -> Catalog {
    let Some(body) = bytes.strip_prefix(MAGIC.as_slice()) else {
        return Catalog::Absent;
    };

    let mut reader = ByteReader::new(body);
    let Some(count) = reader.read_u32_le() else {
        return Catalog::Corrupt;
    };
    let mut tests = Vec::with_capacity((count as usize).min(1024));
    for _ in 0..count {
        match TestDesc::decode(&mut reader) {
            Some(test) => tests.push(test),
            None => return Catalog::Corrupt,
        }
    }
    Catalog::Tests(tests)
}

/// A crate named `a-b-c` conventionally lives in `crates/b-c` or `crates/c`, and the ELF
/// is named after the crate, so `zkpow_sha256-1a2b3c` yields `zkpow_sha256`, `zkpow-sha256`
/// and `sha256` as directories worth probing for a fixture.
fn package_dir_candidates(elf_file_name: &str) -> Vec<String> {
    let stem = elf_file_name.split('-').next().unwrap_or_default();
    if stem.is_empty() {
        return Vec::new();
    }
    let hyphenated = stem.replace('_', "-");
    let mut names = vec![stem.to_string(), hyphenated.clone()];

    let mut rest = hyphenated.as_str();
    while let Some((_, tail)) = rest.split_once('-') {
        names.push(tail.to_string());
        rest = tail;
    }
    names.dedup();
    names
}

/// Directories a fixture or snapshot path may be relative to, most specific first.
/// `package_root` is known whenever the cargo plugin resolved the ELF's package; the
/// working-directory walk is the fallback for a bare `zkvm-test-runner <elf>` invocation.
fn resource_roots(elf_path: Option<&Path>, package_root: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = package_root.into_iter().map(Path::to_path_buf).collect();

    let Ok(cwd) = std::env::current_dir() else {
        return roots;
    };
    let names = elf_path
        .and_then(|e| e.file_name())
        .and_then(|n| n.to_str())
        .map(package_dir_candidates)
        .unwrap_or_default();

    for dir in std::iter::successors(Some(cwd.as_path()), |d| d.parent()) {
        roots.push(dir.to_path_buf());
        for base in [dir.join("crates"), dir.to_path_buf()] {
            roots.extend(names.iter().map(|name| base.join(name)));
        }
    }
    roots
}

fn resolve_resource_path(path_str: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let p = Path::new(path_str);
    if p.exists() {
        return Some(p.to_path_buf());
    }
    roots.iter().map(|root| root.join(p)).find(|candidate| candidate.exists())
}

fn resolve_and_read_fixture(path_str: &str, roots: &[PathBuf]) -> Result<Vec<u8>, String> {
    let resolved = resolve_resource_path(path_str, roots)
        .ok_or_else(|| format!("fixture file '{path_str}' not found on host filesystem"))?;
    std::fs::read(&resolved).map_err(|e| format!("cannot read {}: {e}", resolved.display()))
}

/// Unlike a fixture, a snapshot may legitimately not exist yet, so an unresolvable
/// relative path falls back to the most specific root rather than failing.
fn resolve_snapshot_path(path_str: &str, roots: &[PathBuf]) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    resolve_resource_path(path_str, roots)
        .or_else(|| roots.first().map(|root| root.join(p)))
        .unwrap_or_else(|| p.to_path_buf())
}

fn verify_or_update_snapshot(
    snapshot_path: &str,
    committed: &[u8],
    accept_snapshots: bool,
    roots: &[PathBuf],
) -> Result<(), String> {
    let resolved = resolve_snapshot_path(snapshot_path, roots);
    match zkvm_test::snapshot::verify(&resolved, committed, accept_snapshots)? {
        zkvm_test::snapshot::Outcome::Matched => {},
        written => println!("  [SNAPSHOT {}: {}]", written.as_str().to_uppercase(), resolved.display()),
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

/// A `should_panic(expected = "...")` test asserts the text of a guest panic, which the
/// panic handler writes to stderr. A test that also declares an exit code is asserting a
/// `halt` syscall instead, which by construction carries no message, so there the exit
/// code is the assertion and the text stays a host-side one.
fn panic_message_verdict(test: &TestDesc, stderr: &str) -> Option<String> {
    let expected = test.expected_message.as_deref()?;
    if test.expected_exit_code.is_some() || stderr.contains(expected) {
        return None;
    }
    // The failure report prints the captured stderr right below this, so quoting it
    // here as well would only print the panic twice.
    Some(match stderr.trim() {
        "" => format!("test failed with no guest stderr, so the expected panic text {expected:?} could not be matched"),
        _ => format!("guest stderr did not contain the expected panic text {expected:?}"),
    })
}

/// Whether the guest's exit code is the one the test declared.
fn exit_code_verdict(test: &TestDesc, exit_code: u64) -> Option<String> {
    match test.expected_exit_code {
        Some(expected) if exit_code == u64::from(expected) => None,
        Some(expected) => Some(format!("guest exited with code {exit_code}, expected {expected}")),
        None if test.should_panic => (exit_code == 0)
            .then(|| "test did not panic as expected (guest exited with code 0)".to_string()),
        None if exit_code == 0 => None,
        None => Some(format!("guest exited with code {exit_code}")),
    }
}

/// What one execution of the guest produced. `failure` is `None` while the test is
/// still passing; the declarative checks in `run` fill it in afterwards.
#[derive(Default)]
struct ExecOutput {
    cycles:        u64,
    record_bytes:  u64,
    cycle_tracker: std::collections::BTreeMap<String, u64>,
    invocations:   std::collections::BTreeMap<String, u64>,
    public_values: Vec<u8>,
    breakdown:     Option<InstructionBreakdown>,
    top_syscalls:  Vec<(String, u64)>,
    failure:       Option<String>,
}

impl ExecOutput {
    fn from_result(
        test: &TestDesc,
        res: Result<(sp1_sdk::SP1PublicValues, sp1_sdk::ExecutionReport), sp1_core_executor::ExecutionError>,
    ) -> Self {
        let (pv, report) = match res {
            Ok(ok) => ok,
            Err(err) => {
                return Self {
                    failure: Some(format!("executor error: {err}")),
                    ..Self::default()
                };
            },
        };

        let mut top_syscalls: Vec<(String, u64)> = report
            .syscall_counts
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(code, count)| (format!("{code:?}"), *count))
            .collect();
        top_syscalls.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        Self {
            cycles: report.total_instruction_count(),
            record_bytes: report.total_record_size(),
            breakdown: Some(InstructionBreakdown::from_report(&report)),
            top_syscalls,
            failure: exit_code_verdict(test, report.exit_code),
            public_values: pv.to_vec(),
            cycle_tracker: report.cycle_tracker.into_iter().collect(),
            invocations: report.invocation_tracker.into_iter().collect(),
        }
    }
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
    pub duration:              std::time::Duration,
}

impl TestOutcome {
    fn failed(desc: TestDesc, reason: String, duration: std::time::Duration) -> Self {
        Self {
            desc,
            passed: false,
            cycles: 0,
            failure_reason: Some(reason),
            stdout: String::new(),
            stderr: String::new(),
            record_bytes: 0,
            cycle_tracker: std::collections::BTreeMap::new(),
            invocations: std::collections::BTreeMap::new(),
            instruction_breakdown: None,
            duration,
        }
    }
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
        // The prover backends share no object-safe trait, so the body is monomorphised
        // per variant rather than written out three times.
        macro_rules! prove_with {
            ($prover:expr) => {{
                let prover = $prover;
                let req = prover.prove(pk, stdin);
                let proof = match mode {
                    ProofMode::None => return Err("cannot prove in mode 'none'".to_string()),
                    ProofMode::Core => req.core().await,
                    ProofMode::Compressed => req.compressed().await,
                    ProofMode::Plonk => req.plonk().await,
                    ProofMode::Groth16 => req.groth16().await,
                }
                .map_err(|e| format!("prove {mode:?} error: {e}"))?;
                prover
                    .verify(&proof, pk.verifying_key(), None)
                    .map_err(|e| format!("proof verification error: {e}"))?;
                Ok(proof)
            }};
        }

        match self {
            AnyProver::Mock(p) => prove_with!(p),
            AnyProver::Cpu(p) => prove_with!(p),
            #[cfg(feature = "cuda")]
            AnyProver::Cuda(p) => prove_with!(p),
        }
    }
}

pub fn format_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let offset = s.len() % 3;
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 3 == offset {
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

fn tree_branch(index: usize, len: usize) -> &'static str {
    if index + 1 >= len { "└── " } else { "├── " }
}

fn percent_of(count: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (count as f64 / total as f64) * 100.0
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
        let calls = match invocations.get(*label) {
            Some(1) => " [1 call]".to_string(),
            Some(n) => format!(" [{n} calls]"),
            None => String::new(),
        };
        println!(
            "        {}{label}: {} cycles ({:.1}%){calls}",
            tree_branch(i, entries.len()),
            format_commas(**count),
            percent_of(**count, total_cycles)
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
    let non_zero: Vec<_> = categories.into_iter().filter(|(_, count)| *count > 0).collect();
    for (i, (name, count)) in non_zero.iter().enumerate() {
        println!(
            "        {}{name:24} : {:>10} instructions ({:>5.1}%)",
            tree_branch(i, non_zero.len()),
            format_commas(*count),
            percent_of(*count, total_cycles)
        );
    }
}

fn print_top_syscalls(syscalls: &[(String, u64)]) {
    let top = &syscalls[..syscalls.len().min(TOP_SYSCALL_COUNT)];
    if top.is_empty() {
        return;
    }
    println!("    Syscalls & Precompiles:");
    for (i, (name, count)) in top.iter().enumerate() {
        println!(
            "        {}{name:24} : {:>10} invocations",
            tree_branch(i, top.len()),
            format_commas(*count)
        );
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
        _ => Catalog::Absent,
    };

    let all_tests = match test_catalog {
        Catalog::Tests(tests) => tests,
        Catalog::Corrupt => {
            eprintln!(
                "zkvm-test-runner: {} answered the test-list command with a catalog this runner \
                 cannot decode. The zkvm-test version it was built against and this runner \
                 disagree about the wire format; rebuild both from the same workspace.",
                args.elf_path
            );
            return 2;
        },
        Catalog::Absent => {
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

    let raw_elf_name = Path::new(&args.elf_path)
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|name| name.split('-').next())
        .unwrap_or("");

    let pkg_name = args.target_name.clone().unwrap_or_else(|| raw_elf_name.to_string());

    let package_root = args.package_graph.as_ref().zip(args.package_id.as_ref()).and_then(|(graph, id)| {
        let pkg = graph.metadata(&guppy::PackageId::new(id.clone())).ok()?;
        Path::new(pkg.manifest_path().as_str()).parent().map(Path::to_path_buf)
    });

    // A filter expression is evaluated against the binary the ELF was built from, so it
    // needs the package graph; the plugin passes one down, a bare invocation queries it.
    let test_filter = match args.filter_expr {
        Some(ref expr_str) => {
            let graph = match args.package_graph.clone() {
                Some(graph) => graph,
                None => match guppy::graph::PackageGraph::from_command(&mut guppy::MetadataCommand::new()) {
                    Ok(graph) => Arc::new(graph),
                    Err(err) => {
                        eprintln!("zkvm-test-runner: failed to load package graph: {err}");
                        return 2;
                    },
                },
            };

            let filterset = match crate::filter::parse(expr_str, &graph) {
                Ok(filterset) => filterset,
                Err(err) => {
                    eprintln!("zkvm-test-runner: {err}");
                    return 2;
                },
            };

            let pkg = match args.package_id {
                Some(ref pid) => graph.metadata(&guppy::PackageId::new(pid.clone())).ok(),
                None => graph
                    .workspace()
                    .iter()
                    .find(|p| p.name() == raw_elf_name || p.name().replace('-', "_") == raw_elf_name),
            };
            let Some(pkg) = pkg else {
                eprintln!("zkvm-test-runner: failed to resolve package for ELF '{}'", args.elf_path);
                return 2;
            };

            let kind = nextest_metadata::RustTestBinaryKind::new(
                args.target_kind.clone().unwrap_or_else(|| crate::filter::DEFAULT_TARGET_KIND.to_string()),
            );
            let binary = crate::filter::Binary::new(pkg.name(), pkg.id().clone(), pkg_name.clone(), kind);
            Some((filterset, binary))
        },
        None => None,
    };

    // 2. Filter tests
    let matches_filter = |name: &str| -> bool {
        let name_match = if args.filters.is_empty() {
            true
        } else if args.exact {
            args.filters.iter().any(|f| f == name)
        } else {
            args.filters.iter().any(|f| name.contains(f))
        };

        name_match
            && test_filter
                .as_ref()
                .is_none_or(|(filterset, binary)| binary.matches_test(filterset, name))
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
    let resource_roots = Arc::new(resource_roots(Some(Path::new(&args.elf_path)), package_root.as_deref()));

    for test in tests_to_run {
        let sem = semaphore.clone();
        let prover = prover.clone();
        let pk = pk.clone();
        let elf_bytes = elf_bytes.clone();
        let roots = resource_roots.clone();
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

            let mut fixture_bytes = None;
            if let Some(ref fix_path) = test.fixture {
                match resolve_and_read_fixture(fix_path, &roots) {
                    Ok(bytes) => {
                        run_stdin.write_slice(&bytes);
                        fixture_bytes = Some(bytes);
                    },
                    Err(err) => {
                        return TestOutcome::failed(
                            test,
                            format!("failed to load fixture: {err}"),
                            test_start.elapsed(),
                        );
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
            let out = ExecOutput::from_result(&test, res);

            // Declarative checks, in declaration order; the first failure stops the rest.
            let mut failure = out.failure;

            if failure.is_none()
                && let Some(reason) = panic_message_verdict(&test, &stderr)
            {
                failure = Some(reason);
            }

            if failure.is_none()
                && let Some(max) = test.max_cycles
                && out.cycles > max
            {
                failure = Some(format!(
                    "cycle budget exceeded: {} cycles > budget of {}",
                    format_commas(out.cycles),
                    format_commas(max)
                ));
            }

            if failure.is_none()
                && let Some(ref snap_path) = test.snapshot
            {
                failure =
                    verify_or_update_snapshot(snap_path, &out.public_values, accept_snapshots, &roots).err();
            }

            let mut proof_duration = None;
            if failure.is_none()
                && let Some(ref proving_key) = pk
            {
                let mut prove_stdin = SP1Stdin::new();
                prove_stdin.write_slice(&payload);
                if let Some(ref bytes) = fixture_bytes {
                    prove_stdin.write_slice(bytes);
                }
                let started = Instant::now();
                match prover.prove_test(proving_key, prove_stdin, proof_mode).await {
                    Ok(_) => proof_duration = Some(started.elapsed()),
                    Err(err) => {
                        failure = Some(format!(
                            "proof generation/verification failed in mode {proof_mode:?}: {err}"
                        ));
                    },
                }
            }

            if !quiet {
                match failure {
                    None => {
                        let mut meta_items = Vec::new();
                        if let Some(expected) = test.expected_exit_code {
                            meta_items.push(format!("exit code: {expected}"));
                        } else if test.should_panic {
                            meta_items.push("should panic".to_string());
                        }
                        if let Some(max) = test.max_cycles {
                            meta_items.push(format!("budget: {}", format_commas(max)));
                        }
                        meta_items.push(format!("{} cycles", format_commas(out.cycles)));
                        if out.record_bytes > 0 {
                            meta_items.push(format!("trace: {}", format_bytes(out.record_bytes)));
                        }
                        if let Some(dur) = proof_duration {
                            meta_items.push(format!("{proof_mode:?} proof: {:.2}s", dur.as_secs_f64()));
                        }
                        println!("test {} ... ok ({})", test.name, meta_items.join(", "));

                        if profile {
                            print_profile_breakdown(&out.cycle_tracker, &out.invocations, out.cycles);
                        }
                        if profile || instructions {
                            if let Some(ref breakdown) = out.breakdown {
                                print_instruction_breakdown(breakdown, out.cycles);
                            }
                            print_top_syscalls(&out.top_syscalls);
                        }
                    },
                    Some(_) => println!("test {} ... FAILED", test.name),
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
                passed: failure.is_none(),
                cycles: out.cycles,
                failure_reason: failure,
                stdout,
                stderr,
                record_bytes: out.record_bytes,
                cycle_tracker: out.cycle_tracker,
                invocations: out.invocations,
                instruction_breakdown: out.breakdown,
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
    } else if Path::new(&default_baseline_path).exists() {
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

        let p = Path::new(junit_path);
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

        let parsed = parse_args_from(args).expect("valid arguments");
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
        let nocap_parsed = parse_args_from(nocap_args).expect("valid arguments");
        assert!(nocap_parsed.nocapture);
        assert_eq!(nocap_parsed.test_threads, 1);
    }

    #[test]
    fn test_decode_test_list_valid() {
        let mut bytes = MAGIC.to_vec();
        // Count: 3 tests
        bytes.extend_from_slice(&3u32.to_le_bytes());

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

        // Test 3: expects a panic carrying a specific message
        bytes.push(FLAG_SHOULD_PANIC | FLAG_EXPECTED_MESSAGE);
        let message = "median hint missing";
        bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
        bytes.extend_from_slice(message.as_bytes());
        let name3 = "tests::message_test";
        bytes.extend_from_slice(&(name3.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name3.as_bytes());

        let Catalog::Tests(decoded) = decode_test_list(&bytes) else {
            panic!("should decode valid list");
        };
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[2].name, "tests::message_test");
        assert!(decoded[2].should_panic);
        assert_eq!(decoded[2].expected_message.as_deref(), Some("median hint missing"));
        assert_eq!(decoded[2].expected_exit_code, None);

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
        assert_eq!(decoded[0].expected_message, None);
        assert_eq!(decoded[1].expected_message, None);
    }

    #[test]
    fn test_decode_test_list_distinguishes_absent_from_corrupt() {
        // A stock libtest ELF ignores the command and commits nothing recognizable,
        // which must stay distinguishable from a reply this runner cannot read.
        assert!(matches!(decode_test_list(&[]), Catalog::Absent));
        assert!(matches!(decode_test_list(b"not a zkvm-test binary"), Catalog::Absent));

        // Tagged, but truncated: says 5 tests and ends
        let mut truncated = MAGIC.to_vec();
        truncated.extend_from_slice(&5u32.to_le_bytes());
        assert!(matches!(decode_test_list(&truncated), Catalog::Corrupt));

        // Tagged, but not even a count
        assert!(matches!(decode_test_list(MAGIC.as_slice()), Catalog::Corrupt));

        let mut empty_suite = MAGIC.to_vec();
        empty_suite.extend_from_slice(&0u32.to_le_bytes());
        assert!(matches!(decode_test_list(&empty_suite), Catalog::Tests(tests) if tests.is_empty()));
    }

    #[test]
    fn test_parse_args_rejects_bad_values() {
        let err = |args: &[&str]| {
            parse_args_from(args.iter().map(|s| s.to_string())).expect_err("should be rejected")
        };

        assert!(err(&[]).contains("usage: zkvm-test-runner <elf-path>"));
        assert!(err(&["elf", "--test-threads=abc"]).contains("--test-threads"));
        assert!(err(&["elf", "--test-threads=0"]).contains("positive integer"));
        assert!(err(&["elf", "--fail-on-regression=ten"]).contains("--fail-on-regression"));
        assert!(err(&["elf", "--prover"]).contains("--prover expects a value"));
        assert!(err(&["elf", "--mode"]).contains("--mode expects a value"));
    }

    #[test]
    fn test_parse_args_accepts_flag_aliases() {
        let parse = |args: &[&str]| {
            parse_args_from(args.iter().map(|s| s.to_string())).expect("valid arguments")
        };

        // `--subcycles` is the alias the cargo plugin advertises for `--profile`
        assert!(parse(&["elf", "--subcycles"]).profile);
        assert!(parse(&["elf", "--opcodes"]).instructions);
        assert!(parse(&["elf", "--no-capture"]).nocapture);
        assert!(parse(&["elf", "--update-snapshots"]).accept_snapshots);
        assert_eq!(parse(&["elf", "--fail-on-regression", "10%"]).fail_on_regression, Some(10.0));

        // An unknown flag is ignored, but a positional is a test-name filter
        let parsed = parse(&["elf", "--brand-new-cargo-flag", "some_test"]);
        assert_eq!(parsed.filters, vec!["some_test"]);

        // `--save-baseline` takes an optional name
        assert_eq!(parse(&["elf", "--save-baseline"]).save_baseline.as_deref(), Some("default"));
        assert_eq!(parse(&["elf", "--save-baseline", "run1"]).save_baseline.as_deref(), Some("run1"));
        let standalone = parse(&["elf", "--save-baseline", "--quiet"]);
        assert_eq!(standalone.save_baseline.as_deref(), Some("default"));
        assert!(standalone.quiet);
    }

    fn desc(should_panic: bool, expected_exit_code: Option<u32>) -> TestDesc {
        TestDesc {
            name: "t".to_string(),
            ignore: false,
            should_panic,
            expected_message: None,
            expected_exit_code,
            max_cycles: None,
            fixture: None,
            snapshot: None,
        }
    }

    #[test]
    fn test_exit_code_verdict() {
        assert_eq!(exit_code_verdict(&desc(false, None), 0), None);
        assert!(exit_code_verdict(&desc(false, None), 1).unwrap().contains("exited with code 1"));

        // should_panic accepts any non-zero exit, and requires one
        assert_eq!(exit_code_verdict(&desc(true, None), 3), None);
        assert!(exit_code_verdict(&desc(true, None), 0).unwrap().contains("did not panic"));

        // A declared exit code must match exactly, even a non-zero one
        assert_eq!(exit_code_verdict(&desc(true, Some(3)), 3), None);
        let mismatch = exit_code_verdict(&desc(true, Some(3)), 1).unwrap();
        assert!(mismatch.contains("code 1"), "{mismatch}");
        assert!(mismatch.contains("expected 3"), "{mismatch}");
        assert!(exit_code_verdict(&desc(true, Some(3)), 0).is_some());
    }

    #[test]
    fn test_package_dir_candidates() {
        assert_eq!(
            package_dir_candidates("zkpow_sha256-1a2b3c4d"),
            vec!["zkpow_sha256", "zkpow-sha256", "sha256"]
        );
        assert_eq!(package_dir_candidates("core-9f"), vec!["core"]);
        assert!(package_dir_candidates("").is_empty());
    }

    #[test]
    fn test_resolve_resource_path_prefers_specific_root() {
        let temp = tempfile::tempdir().unwrap();
        let specific = temp.path().join("crates/sha256");
        let general = temp.path();
        std::fs::create_dir_all(&specific).unwrap();
        std::fs::write(general.join("fixture.bin"), b"general").unwrap();
        std::fs::write(specific.join("fixture.bin"), b"specific").unwrap();

        let roots = vec![specific.clone(), general.to_path_buf()];
        let found = resolve_resource_path("fixture.bin", &roots).unwrap();
        assert_eq!(std::fs::read(found).unwrap(), b"specific");

        assert!(resolve_resource_path("missing.bin", &roots).is_none());
    }

    #[test]
    fn test_resolve_snapshot_path_falls_back_to_first_root() {
        let temp = tempfile::tempdir().unwrap();
        let roots = vec![temp.path().to_path_buf()];
        assert_eq!(
            resolve_snapshot_path("snapshots/new.hex", &roots),
            temp.path().join("snapshots/new.hex")
        );
    }

    #[test]
    fn test_verify_or_update_snapshot_creates_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let snap = temp.path().join("nested/golden.hex");

        let err = verify_or_update_snapshot(snap.to_str().unwrap(), &[0xab], false, &[]).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
        assert!(!snap.exists());

        verify_or_update_snapshot(snap.to_str().unwrap(), &[0xab], true, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&snap).unwrap(), "ab\n");
    }

    #[test]
    fn test_verify_or_update_snapshot_binary_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let snap = temp.path().join("golden.bin");
        std::fs::write(&snap, [1u8, 2, 3]).unwrap();

        let err = verify_or_update_snapshot(snap.to_str().unwrap(), &[1, 2, 4], false, &[]).unwrap_err();
        assert!(err.contains("snapshot mismatch"), "{err}");
        assert!(err.contains("--accept-snapshots"), "{err}");

        verify_or_update_snapshot(snap.to_str().unwrap(), &[1, 2, 4], true, &[]).unwrap();
        assert_eq!(std::fs::read(&snap).unwrap(), vec![1, 2, 4]);
    }

    #[test]
    fn test_decode_test_list_rejects_absurd_count() {
        // A corrupt catalog must not be trusted into a huge allocation
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.push(0);
        assert!(matches!(decode_test_list(&bytes), Catalog::Corrupt));
    }

    #[test]
    fn test_panic_message_verdict() {
        let with_message = |expected_exit_code| TestDesc {
            expected_message: Some("median hint missing".to_string()),
            ..desc(true, expected_exit_code)
        };

        // A panicking test asserts the text the guest panic handler wrote to stderr
        assert_eq!(
            panic_message_verdict(&with_message(None), "panicked at: median hint missing\n"),
            None
        );
        let failed = panic_message_verdict(&with_message(None), "panicked at: something else").unwrap();
        assert!(failed.contains("median hint missing"), "{failed}");

        // Nothing on stderr is called out, since there is no stderr section to read
        let silent = panic_message_verdict(&with_message(None), "  \n ").unwrap();
        assert!(silent.contains("no guest stderr"), "{silent}");

        // A declared exit code means a halt syscall, which carries no message
        assert_eq!(panic_message_verdict(&with_message(Some(3)), ""), None);

        // No declared message, nothing to check
        assert_eq!(panic_message_verdict(&desc(true, None), ""), None);
    }

    #[test]
    fn test_tree_branch_and_percent() {
        assert_eq!(tree_branch(0, 2), "\u{251c}\u{2500}\u{2500} ");
        assert_eq!(tree_branch(1, 2), "\u{2514}\u{2500}\u{2500} ");
        // An out-of-range index must not underflow
        assert_eq!(tree_branch(0, 0), "\u{2514}\u{2500}\u{2500} ");

        assert_eq!(percent_of(0, 0), 0.0);
        assert_eq!(percent_of(50, 200), 25.0);
    }

    #[test]
    fn test_print_top_syscalls_handles_empty() {
        print_top_syscalls(&[]);
        print_top_syscalls(&[("HALT".to_string(), 1)]);
    }

    #[test]
    fn test_verify_or_update_snapshot_matching() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snap_file = temp_dir.path().join("golden.hex");
        std::fs::write(&snap_file, "deadbeef\n").unwrap();

        let committed = vec![0xde, 0xad, 0xbe, 0xef];
        let res = verify_or_update_snapshot(snap_file.to_str().unwrap(), &committed, false, &[]);
        assert!(res.is_ok());
    }

    #[test]
    fn test_verify_or_update_snapshot_mismatch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snap_file = temp_dir.path().join("golden.hex");
        std::fs::write(&snap_file, "deadbeef\n").unwrap();

        let committed = vec![0xca, 0xfe, 0xba, 0xbe];
        let res = verify_or_update_snapshot(snap_file.to_str().unwrap(), &committed, false, &[]);
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
        let res = verify_or_update_snapshot(snap_file.to_str().unwrap(), &committed, true, &[]);
        assert!(res.is_ok());

        // Verify updated file on disk
        let updated = std::fs::read_to_string(&snap_file).unwrap();
        assert_eq!(updated.trim(), "cafebabe");
    }
}

