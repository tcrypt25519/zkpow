//! `cargo-zkvm-test` CLI plugin logic and helpers.
//!
//! Provides CLI argument partitioning, toolchain selection, workspace
//! inspection, Nextest filter pruning, and test execution orchestration.

use std::{
    collections::HashMap,
    io::BufRead,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, OnceLock},
};

use clap::{Args, CommandFactory, FromArgMatches, Parser};
use guppy::{MetadataCommand, graph::PackageGraph};
use nextest_filtering::Filterset;
use nextest_metadata::RustTestBinaryKind;
use serde::Deserialize;

use crate::runner::{RunnerArgs, parse_options_from, run};

#[derive(Debug, Deserialize)]
pub struct Artifact {
    pub reason:     String,
    #[serde(default)]
    pub package_id: String,
    pub target:     Option<ArtifactTarget>,
    pub profile:    Option<ArtifactProfile>,
    pub executable: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ArtifactProfile {
    #[serde(default)]
    pub test: bool,
}

#[derive(Debug, Deserialize)]
pub struct ArtifactTarget {
    pub name: String,
    pub kind: Vec<String>,
}

impl Artifact {
    fn into_test_executable(self) -> Option<TestExecutable> {
        if self.reason != REASON_COMPILER_ARTIFACT {
            return None;
        }
        let is_test = self.profile.is_some_and(|profile| profile.test)
            || self
                .target
                .as_ref()
                .is_some_and(|target| target.kind.iter().any(|kind| kind == "test"));
        if !is_test {
            return None;
        }

        let elf_path = PathBuf::from(self.executable?);
        let target_name = self
            .target
            .as_ref()
            .map(|target| target.name.clone())
            .unwrap_or_else(|| DEFAULT_TARGET_NAME.to_string());
        let target_kind = self
            .target
            .and_then(|target| target.kind.into_iter().next())
            .unwrap_or_else(|| crate::filter::DEFAULT_TARGET_KIND.to_string());

        Some(TestExecutable {
            target_name,
            target_kind: RustTestBinaryKind::new(target_kind),
            package_id: guppy::PackageId::new(self.package_id),
            elf_path,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TestExecutable {
    pub target_name: String,
    pub target_kind: RustTestBinaryKind,
    pub package_id:  guppy::PackageId,
    pub elf_path:    PathBuf,
}

pub const BIN_NAME: &str = "cargo-zkvm-test";

pub const DEFAULT_SP1_TARGET: &str = "riscv64im-succinct-zkvm-elf";
pub const DEFAULT_RISC0_TARGET: &str = "riscv32im-risc0-zkvm-elf";

pub const REASON_COMPILER_ARTIFACT: &str = "compiler-artifact";
pub const DEFAULT_TARGET_NAME: &str = "test";

pub const ZKVM_TEST_CRATE: &str = "zkvm-test";
pub const ZKVM_TEST_CRATE_ALT: &str = "zkvm_test";
pub const ZKVM_TEST_MACROS_CRATE: &str = "zkvm-test-macros";
pub const ZKVM_TEST_RUNNER_CRATE: &str = "zkvm-test-runner";
pub const ZKVM_SUBDIR: &str = "zkvm";
pub const METADATA_KEY_ZKVM_TEST: &str = "zkvm-test";
pub const METADATA_KEY_ZKVM: &str = "zkvm";

/// The zkVM a target triple names, which fixes both the rustup toolchain that can
/// build for it and the triple used when none is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Risc0,
    Succinct,
}

impl Vendor {
    pub const fn toolchain(&self) -> &'static str {
        match self {
            Self::Risc0 => "+risc0",
            Self::Succinct => "+succinct",
        }
    }

    pub const fn default_target(&self) -> &'static str {
        match self {
            Self::Risc0 => DEFAULT_RISC0_TARGET,
            Self::Succinct => DEFAULT_SP1_TARGET,
        }
    }

    pub fn from_target(target: &str) -> Option<Self> {
        if target.contains("succinct") {
            Some(Self::Succinct)
        } else if target.contains("risc0") {
            Some(Self::Risc0)
        } else {
            None
        }
    }
}

/// Cargo flags that take a value but are not modelled in `CargoOpts`.
const EXTRA_CARGO_VALUE_FLAGS: &[&str] = &["--message-format", "--jobs", "-j"];

const ARG_PACKAGE: &str = "package";
const ARG_WORKSPACE: &str = "workspace";
const ARG_EXCLUDE: &str = "exclude";
const ARG_ALL_TARGETS: &str = "all_targets";
const ARG_HOST: &str = "host";
const ARG_TARGET: &str = "target";
const ARG_TARGET_DIR: &str = "target_dir";

/// How an argument consumes the words that follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Follow {
    None,
    Required,
    Optional,
}

#[derive(Debug)]
struct FlagInfo {
    id:     String,
    follow: Follow,
}

#[derive(Debug, Default)]
pub struct FlagSpec {
    flags: HashMap<String, FlagInfo>,
}

impl FlagSpec {
    /// Read the spec off the CLI definition itself, so a flag cannot be advertised in
    /// `--help` and then mis-routed here.
    pub fn of<A: Args>() -> Self {
        let command = A::augment_args(clap::Command::new("spec"));
        let mut flags = HashMap::new();

        for arg in command.get_arguments().filter(|arg| !arg.is_positional()) {
            let num_args = arg.get_num_args();
            let takes_value = num_args.map_or(arg.get_action().takes_values(), |range| range.takes_values());
            let optional = num_args.is_some_and(|range| range.takes_values() && range.min_values() == 0);

            let follow = match (takes_value, optional) {
                (false, _) => Follow::None,
                (true, false) => Follow::Required,
                (true, true) => Follow::Optional,
            };
            let id = arg.get_id().as_str().to_string();
            let short_names = arg.get_short_and_visible_aliases().unwrap_or_default();
            let long_names = arg.get_long_and_visible_aliases().unwrap_or_default();
            let names = short_names
                .iter()
                .map(|short| format!("-{short}"))
                .chain(long_names.iter().map(|long| format!("--{long}")));
            for name in names {
                flags.insert(name, FlagInfo { id: id.clone(), follow });
            }
        }
        Self { flags }
    }

    fn get(&self, key: &str) -> Option<&FlagInfo> {
        self.flags.get(key)
    }

    fn follow(&self, key: &str) -> Option<Follow> {
        self.get(key).map(|info| info.follow)
    }
}

fn cargo_flags() -> &'static FlagSpec {
    static FLAGS: OnceLock<FlagSpec> = OnceLock::new();
    FLAGS.get_or_init(FlagSpec::of::<CargoOpts>)
}

fn runner_flags() -> &'static FlagSpec {
    static FLAGS: OnceLock<FlagSpec> = OnceLock::new();
    FLAGS.get_or_init(FlagSpec::of::<RunnerOpts>)
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "cargo-zkvm-test",
    bin_name = "cargo zkvm-test",
    before_help = "cargo-zkvm-test: Nextest-grade test runner for zkVM binaries",
    about = "Nextest-grade test runner for zkVM binaries"
)]
pub struct CargoPluginCli {
    #[command(flatten)]
    pub cargo: CargoOpts,

    #[command(flatten)]
    pub runner: RunnerOpts,
}

#[derive(Args, Debug, Clone, Default)]
pub struct CargoOpts {
    /// Package to run tests for
    #[arg(short = 'p', long = "package", value_name = "SPEC")]
    pub package: Vec<String>,

    /// Test all zkVM packages in workspace (default when run from root)
    #[arg(long, visible_alias = "all")]
    pub workspace: bool,

    /// Exclude packages from the test run
    #[arg(long, value_name = "SPEC")]
    pub exclude: Vec<String>,

    /// Test only this package's library unit tests
    #[arg(long)]
    pub lib: bool,

    /// Test only the specified binary
    #[arg(long, value_name = "NAME")]
    pub bin: Vec<String>,

    /// Test only the specified integration test
    #[arg(long, value_name = "NAME")]
    pub test: Vec<String>,

    /// Test only the specified example
    #[arg(long, value_name = "NAME")]
    pub example: Vec<String>,

    /// Test only the specified benchmark
    #[arg(long, value_name = "NAME")]
    pub bench: Vec<String>,

    /// Test all targets (both host and zkVM)
    #[arg(long)]
    pub all_targets: bool,

    /// Run native host tests
    #[arg(long)]
    pub host: bool,

    /// Build artifacts in release mode
    #[arg(long)]
    pub release: bool,

    /// Cargo compilation profile (e.g. dev, test, release, bench)
    #[arg(long = "cargo-profile", value_name = "NAME")]
    pub cargo_profile: Option<String>,

    /// Override target triple (default: riscv64im-succinct-zkvm-elf)
    #[arg(long, value_name = "TARGET")]
    pub target: Option<String>,

    /// Directory for all generated artifacts
    #[arg(long, value_name = "DIR")]
    pub target_dir: Option<PathBuf>,

    /// Space or comma separated list of features to activate
    #[arg(long, value_delimiter = ',', value_name = "FEATURES")]
    pub features: Vec<String>,

    /// Activate all available features
    #[arg(long)]
    pub all_features: bool,

    /// Do not activate the `default` feature
    #[arg(long)]
    pub no_default_features: bool,

    /// Path to Cargo.toml
    #[arg(long, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// Coloring: auto, always, never
    #[arg(long, value_name = "WHEN")]
    pub color: Option<String>,

    /// Override a Cargo configuration value
    #[arg(long, value_name = "KEY=VALUE")]
    pub config: Vec<String>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct RunnerOpts {
    /// Do not capture guest stdout/stderr; print immediately
    #[arg(long, visible_alias = "no-capture")]
    pub nocapture: bool,

    /// Filter tests by exact name match
    #[arg(long)]
    pub exact: bool,

    /// List all matching tests without running them
    #[arg(long)]
    pub list: bool,

    /// Run only ignored tests
    #[arg(long)]
    pub ignored: bool,

    /// Run both regular and ignored tests
    #[arg(long)]
    pub include_ignored: bool,

    /// Display hierarchical sub-routine cycle breakdown
    #[arg(long, visible_alias = "subcycles")]
    pub profile: bool,

    /// Display instruction category breakdown (ALU, Memory, Control, Math, Syscalls)
    #[arg(long, visible_alias = "opcodes")]
    pub instructions: bool,

    /// Proof mode: none (default), core, compressed, plonk, groth16
    #[arg(long, visible_alias = "proof-mode", value_name = "MODE")]
    pub mode: Option<String>,

    /// Nextest-style boolean filter expression (e.g. 'package(sha256) & test(header)')
    #[arg(short = 'E', long = "expr", visible_alias = "filter-expr", value_name = "EXPR")]
    pub filter_expr: Option<String>,

    /// Export test results to JUnit XML format for CI/CD
    #[arg(long, value_name = "PATH")]
    pub junit_path: Option<PathBuf>,

    /// Save execution metrics as a named baseline (default: "default")
    #[arg(long, num_args = 0..=1, default_missing_value = "default", value_name = "NAME")]
    pub save_baseline: Option<String>,

    /// Compare execution metrics against a named baseline
    #[arg(long, value_name = "NAME")]
    pub compare_baseline: Option<String>,

    /// Fail if any test regresses by more than N percent
    #[arg(long, value_name = "PCT")]
    #[arg(value_parser = parse_percentage)]
    pub fail_on_regression: Option<f64>,

    /// Accept/update golden snapshot files for tests with #[test(snapshot = "...")]
    #[arg(long, visible_alias = "update-snapshots")]
    pub accept_snapshots: bool,

    /// Number of test instances to execute concurrently
    #[arg(long, value_name = "N")]
    #[arg(value_parser = parse_positive_usize)]
    pub test_threads: Option<usize>,

    /// Prover backend: mock (default), cpu, cuda
    #[arg(long, value_name = "PROVER")]
    pub prover: Option<String>,

    /// Display one character per test instead of one line
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Positional test name filter patterns
    #[arg(value_name = "FILTER")]
    pub filters: Vec<String>,

    #[arg(long, hide = true)]
    pub package_id: Option<String>,

    #[arg(long, hide = true)]
    pub target_name: Option<String>,

    #[arg(long, hide = true)]
    pub target_kind: Option<String>,
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    match value.parse() {
        Ok(value) if value > 0 => Ok(value),
        _ => Err(format!("--test-threads expects a positive integer, got '{value}'")),
    }
}

fn parse_percentage(value: &str) -> Result<f64, String> {
    match value.trim_end_matches('%').parse() {
        Ok(value) if f64::is_finite(value) => Ok(value),
        _ => Err(format!("--fail-on-regression expects a percentage, got '{value}'")),
    }
}

pub fn query_package_graph() -> Option<PackageGraph> {
    let mut cmd = MetadataCommand::new();
    PackageGraph::from_command(&mut cmd).ok()
}

fn is_zkvm_pkg(pkg: &guppy::graph::PackageMetadata) -> bool {
    const FRAMEWORK_CRATES: &[&str] = &[ZKVM_TEST_CRATE, ZKVM_TEST_MACROS_CRATE, ZKVM_TEST_RUNNER_CRATE];
    if FRAMEWORK_CRATES.contains(&pkg.name()) {
        return false;
    }

    let pkg_meta = pkg.metadata_table();
    let pkg_zkvm_meta = pkg_meta.get(METADATA_KEY_ZKVM_TEST).or_else(|| pkg_meta.get(METADATA_KEY_ZKVM));

    let explicit_enable = pkg_zkvm_meta.and_then(|m| {
        m.get("enable")
            .or_else(|| m.get("enabled"))
            .or_else(|| m.get("test"))
            .and_then(|v| v.as_bool())
    });

    if let Some(explicit) = explicit_enable {
        return explicit;
    }

    pkg.direct_links()
        .any(|link| link.to().name() == ZKVM_TEST_CRATE || link.to().name() == ZKVM_TEST_CRATE_ALT)
}

fn contains_name(names: &[String], candidate: &str) -> bool {
    names.iter().any(|name| name == candidate)
}

pub fn resolve_default_packages(graph: &PackageGraph, cli_excludes: &[String]) -> Vec<String> {
    let current_dir = std::env::current_dir().unwrap_or_default();
    resolve_default_packages_from_dir(graph, cli_excludes, &current_dir)
}

pub fn resolve_default_packages_from_dir(
    graph: &PackageGraph,
    cli_excludes: &[String],
    current_dir: &Path,
) -> Vec<String> {
    let is_workspace_root = current_dir == Path::new(graph.workspace().root().as_str());

    if !is_workspace_root
        && let Some(pkg) = graph.workspace().iter().find(|p| {
            Path::new(p.manifest_path().as_str())
                .parent()
                .is_some_and(|parent| current_dir.starts_with(parent))
        })
        && !contains_name(cli_excludes, pkg.name())
        && is_zkvm_pkg(&pkg)
    {
        return vec![pkg.name().to_string()];
    }

    let ws_metadata = graph.workspace().metadata_table();
    let ws_zkvm_meta = ws_metadata
        .get(METADATA_KEY_ZKVM_TEST)
        .or_else(|| ws_metadata.get(METADATA_KEY_ZKVM));

    let ws_explicit_packages: Option<Vec<String>> = ws_zkvm_meta.and_then(|m| {
        m.get("packages")
            .and_then(|p| p.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
    });

    let ws_excludes: Vec<String> = ws_zkvm_meta
        .and_then(|m| m.get("exclude").and_then(|p| p.as_array()))
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    if let Some(explicit) = ws_explicit_packages {
        return explicit
            .into_iter()
            .filter(|p| !ws_excludes.contains(p) && !cli_excludes.contains(p))
            .collect();
    }

    let mut selected = Vec::new();
    for pkg in graph.workspace().iter() {
        if contains_name(&ws_excludes, pkg.name()) || contains_name(cli_excludes, pkg.name()) {
            continue;
        }
        if is_zkvm_pkg(&pkg) {
            selected.push(pkg.name().to_string());
        }
    }

    selected.sort();
    selected.dedup();
    selected
}

fn workspace_packages(graph: &PackageGraph, excludes: &[String]) -> Vec<String> {
    graph
        .workspace()
        .iter()
        .filter(|package| !excludes.iter().any(|exclude| exclude == package.name()))
        .map(|package| package.name().to_string())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageMode {
    Zkvm,
    Host,
    Both,
}

fn package_mode(host: bool, all_targets: bool, is_zkvm: bool) -> PackageMode {
    match (host, all_targets, is_zkvm) {
        (_, true, true) => PackageMode::Both,
        (true, ..) | (false, true, false) => PackageMode::Host,
        (false, false, true) => PackageMode::Zkvm,
        (false, false, false) => PackageMode::Host,
    }
}

fn print_packages(kind: &str, packages: &[String]) {
    if packages.is_empty() {
        return;
    }
    let plural = if packages.len() == 1 { "" } else { "s" };
    println!(
        "{BIN_NAME}: testing {} {kind} package{plural}: {}",
        packages.len(),
        packages.join(", ")
    );
}

pub fn print_help() {
    let _ = CargoPluginCli::command().print_help();
    println!();
}

pub fn resolve_target_dir(explicit_target_dir: Option<&str>, workspace_root: Option<&str>) -> PathBuf {
    if let Some(explicit) = explicit_target_dir {
        return PathBuf::from(explicit);
    }
    if let Ok(env_target) = std::env::var("CARGO_TARGET_DIR") {
        return Path::new(&env_target).join(ZKVM_SUBDIR);
    }
    match workspace_root {
        Some(root) => Path::new(root).join("target").join(ZKVM_SUBDIR),
        None => Path::new("target").join(ZKVM_SUBDIR),
    }
}

pub fn remove_file_if_exists(path: impl AsRef<Path>) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Split a command line into the part cargo builds with and the part the runner filters
/// with. Both halves are driven by the CLI definition rather than a hand-kept table.
pub fn partition_args(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let runner = runner_flags();
    let cargo = cargo_flags();

    let mut cargo_part = Vec::new();
    let mut runner_part = Vec::new();
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        let (key, has_inline_value) = match arg.split_once('=') {
            Some((key, _)) => (key.to_string(), true),
            None => (arg.clone(), false),
        };

        // `--profile` means the cargo profile when followed by a profile name and the
        // runner's cycle breakdown otherwise.
        let is_cargo_profile = key == "--profile"
            && !has_inline_value
            && iter.peek().is_some_and(|next| CARGO_PROFILE_NAMES.contains(&next.as_str()));

        let (part, follow) = if is_cargo_profile {
            (&mut cargo_part, Follow::Required)
        } else if let Some(follow) = runner.follow(&key) {
            (&mut runner_part, follow)
        } else if let Some(follow) = cargo.follow(&key) {
            (&mut cargo_part, follow)
        } else if EXTRA_CARGO_VALUE_FLAGS.contains(&key.as_str()) {
            (&mut cargo_part, Follow::Required)
        } else if !key.starts_with('-') {
            // Positional test-name filter
            (&mut runner_part, Follow::None)
        } else {
            (&mut cargo_part, Follow::None)
        };

        part.push(arg);
        if has_inline_value {
            continue;
        }
        match follow {
            Follow::None => {},
            Follow::Required => part.extend(iter.next()),
            Follow::Optional => part.extend(iter.next_if(|next| !next.starts_with('-'))),
        }
    }
    (cargo_part, runner_part)
}

/// Cargo profile names that disambiguate `--profile <name>` from the runner's `--profile`.
const CARGO_PROFILE_NAMES: &[&str] = &["dev", "test", "release", "bench"];

fn recognized_args(args: &[String], spec: &FlagSpec, include_positionals: bool) -> Result<Vec<String>, String> {
    let mut recognized = vec![BIN_NAME.to_string()];
    let mut iter = args.iter().peekable();

    while let Some(arg) = iter.next() {
        let key = arg.split_once('=').map_or(arg.as_str(), |(key, _)| key);
        let Some(follow) = spec.follow(key) else {
            if include_positionals && !key.starts_with('-') {
                recognized.push(arg.clone());
            }
            continue;
        };

        recognized.push(arg.clone());
        if arg.contains('=') {
            continue;
        }
        match follow {
            Follow::None => {},
            Follow::Required => {
                let value = iter.next().ok_or_else(|| format!("{key} expects a value"))?;
                recognized.push(value.clone());
            },
            Follow::Optional => {
                recognized.extend(iter.next_if(|next| !next.starts_with('-')).cloned());
            },
        }
    }
    Ok(recognized)
}

fn parse_options<A>(args: &[String], spec: &FlagSpec, include_positionals: bool) -> Result<A, String>
where
    A: Args + FromArgMatches,
{
    let recognized = recognized_args(args, spec, include_positionals)?;
    let command = A::augment_args(clap::Command::new(BIN_NAME));
    let matches = command.try_get_matches_from(recognized).map_err(|err| err.to_string())?;
    A::from_arg_matches(&matches).map_err(|err| err.to_string())
}

fn parse_cargo_options(args: &[String]) -> Result<CargoOpts, String> {
    parse_options(args, cargo_flags(), false)
}

pub(crate) fn parse_runner_options(args: &[String]) -> Result<RunnerOpts, String> {
    parse_options(args, runner_flags(), true)
}

fn append_cargo_args(command: &mut Command, args: &[String], omitted_ids: &[&str]) {
    let spec = cargo_flags();
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        let key = arg.split_once('=').map_or(arg.as_str(), |(key, _)| key);
        let info = spec.get(key);
        let omit = info.is_some_and(|info| omitted_ids.contains(&info.id.as_str()));

        if !omit {
            command.arg(arg);
        }
        if arg.contains('=') || !info.is_some_and(|info| info.follow == Follow::Required) {
            continue;
        }
        if let Some(value) = iter.next()
            && !omit
        {
            command.arg(value);
        }
    }
}

/// Drop packages whose every build target the filterset rules out, so they are never
/// compiled in the first place.
pub fn prune_packages_by_filter(packages: &mut Vec<String>, graph: &PackageGraph, filterset: &Filterset) {
    packages.retain(|name| {
        let Some(pkg) = graph.workspace().iter().find(|p| p.name() == name) else {
            return true;
        };
        pkg.build_targets().any(|bt| {
            let kind = RustTestBinaryKind::new(build_target_kind(&bt.id()));
            let binary = crate::filter::Binary::new(pkg.name(), pkg.id().clone(), bt.name().to_string(), kind);
            binary.can_match(filterset)
        })
    });
}

fn build_target_kind(id: &guppy::graph::BuildTargetId) -> &'static str {
    match id {
        guppy::graph::BuildTargetId::Library => "lib",
        guppy::graph::BuildTargetId::BuildScript => "custom-build",
        guppy::graph::BuildTargetId::Binary(_) => "bin",
        guppy::graph::BuildTargetId::Example(_) => "example",
        guppy::graph::BuildTargetId::Benchmark(_) => "bench",
        _ => crate::filter::DEFAULT_TARGET_KIND,
    }
}

pub fn should_prune_binary(
    filterset: &Filterset,
    graph: &PackageGraph,
    pkg_id: &guppy::PackageId,
    target_name: &str,
    target_kind: &RustTestBinaryKind,
) -> bool {
    let Ok(pkg) = graph.metadata(pkg_id) else {
        return false;
    };
    let binary = crate::filter::Binary::new(pkg.name(), pkg_id.clone(), target_name.to_string(), target_kind.clone());
    !binary.can_match(filterset)
}

pub async fn run_cargo_plugin(mut raw_args: Vec<String>) -> i32 {
    // If invoked as `cargo zkvm-test`, cargo passes "zkvm-test" as the first argument.
    if raw_args.first().is_some_and(|s| s == "zkvm-test") {
        raw_args.remove(0);
    }

    if raw_args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return 0;
    }

    let (cargo_raw, runner_raw) = if let Some(separator) = raw_args.iter().position(|arg| arg == "--") {
        let trailing = raw_args.split_off(separator + 1);
        raw_args.pop();
        let (cargo_part, mut runner_part) = partition_args(raw_args);
        runner_part.extend(trailing);
        (cargo_part, runner_part)
    } else {
        partition_args(raw_args)
    };

    let cargo_options = match parse_cargo_options(&cargo_raw) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            return 2;
        },
    };
    let runner_options = match parse_options_from(runner_raw) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{BIN_NAME}: {err}");
            return 2;
        },
    };

    // Determine toolchain and target
    let toolchain = cargo_raw.iter().find(|a| a.starts_with('+')).cloned();
    let cargo_filtered: Vec<String> = cargo_raw.iter().filter(|a| !a.starts_with('+')).cloned().collect();

    let has_explicit_packages = !cargo_options.package.is_empty();
    let has_explicit_targets =
        !cargo_options.bin.is_empty() || !cargo_options.test.is_empty() || !cargo_options.example.is_empty();
    let cli_excludes = &cargo_options.exclude;

    let package_graph = query_package_graph();
    let junit_path = runner_options.junit_path.as_deref().map(Path::new);

    let filterset = match (&runner_options.filter_expr, &package_graph) {
        (Some(expr), Some(graph)) => match crate::filter::parse(expr, graph) {
            Ok(filterset) => Some(filterset),
            Err(err) => {
                eprintln!("{BIN_NAME}: {err}");
                return 2;
            },
        },
        _ => None,
    };

    let mut zkvm_packages = Vec::new();
    let mut host_packages = Vec::new();

    if has_explicit_packages {
        for pkg_name in &cargo_options.package {
            let is_zkvm = package_graph
                .as_ref()
                .and_then(|graph| graph.workspace().iter().find(|package| package.name() == pkg_name))
                .is_some_and(|p| is_zkvm_pkg(&p));

            if !is_zkvm && !cargo_options.host && !cargo_options.all_targets {
                println!("{BIN_NAME}: `{pkg_name}` does not target zkVM; running host tests");
            }
            match package_mode(cargo_options.host, cargo_options.all_targets, is_zkvm) {
                PackageMode::Zkvm => zkvm_packages.push(pkg_name.clone()),
                PackageMode::Host => host_packages.push(pkg_name.clone()),
                PackageMode::Both => {
                    zkvm_packages.push(pkg_name.clone());
                    host_packages.push(pkg_name.clone());
                },
            }
        }
    } else if !has_explicit_targets && let Some(ref graph) = package_graph {
        match (cargo_options.host, cargo_options.all_targets) {
            (true, false) => host_packages = workspace_packages(graph, cli_excludes),
            (_, true) => {
                zkvm_packages = resolve_default_packages(graph, cli_excludes);
                host_packages = workspace_packages(graph, cli_excludes);
            },
            (false, false) => zkvm_packages = resolve_default_packages(graph, cli_excludes),
        }
    }

    if let (Some(filterset), Some(graph)) = (&filterset, &package_graph) {
        prune_packages_by_filter(&mut zkvm_packages, graph, filterset);
        prune_packages_by_filter(&mut host_packages, graph, filterset);
    }

    if zkvm_packages.is_empty() && host_packages.is_empty() && !has_explicit_targets {
        if filterset.is_some() {
            println!("{BIN_NAME}: 0 packages matched filter expression");
            return 0;
        }
        eprintln!("{BIN_NAME}: no test packages found in workspace.");
        return 1;
    }

    print_packages("zkVM", &zkvm_packages);
    print_packages("host", &host_packages);

    // Disambiguate `--cargo-profile` to `--profile`
    let cargo_filtered: Vec<String> = cargo_filtered
        .into_iter()
        .map(|arg| {
            let rewritten = arg.strip_prefix("--cargo-profile").and_then(|rest| match rest {
                "" => Some("--profile".to_string()),
                value if value.starts_with('=') => Some(format!("--profile{value}")),
                _ => None,
            });
            rewritten.unwrap_or(arg)
        })
        .collect();

    // Start fresh JUnit report if requested
    if let Some(junit_path) = junit_path
        && let Err(err) = remove_file_if_exists(junit_path)
    {
        eprintln!(
            "{BIN_NAME}: warning: failed to remove existing JUnit file '{}': {err}",
            junit_path.display()
        );
    }

    let mut overall_exit_code = 0;

    // 1. Host Test Phase
    if !host_packages.is_empty() {
        match compile_host_tests(&host_packages, &cargo_filtered, filterset.as_ref(), package_graph.as_ref()) {
            Ok(host_executables) => {
                for exe in host_executables {
                    let code = run_host_test(
                        &exe,
                        &runner_options,
                        cargo_options.color.as_deref(),
                        junit_path,
                        package_graph.as_ref(),
                    );
                    if code != 0 {
                        overall_exit_code = code;
                    }
                }
            },
            Err(code) => {
                overall_exit_code = code;
            },
        }
    }

    // 2. zkVM Test Phase
    if !zkvm_packages.is_empty() {
        let target = cargo_options.target.clone().unwrap_or_else(|| DEFAULT_SP1_TARGET.to_string());

        match compile_zkvm_tests(
            &zkvm_packages,
            &cargo_filtered,
            ZkvmBuild {
                toolchain:          toolchain.as_deref(),
                target:             &target,
                target_is_explicit: cargo_options.target.is_some(),
                target_dir:         cargo_options.target_dir.as_deref(),
            },
            package_graph.as_ref(),
            filterset.as_ref(),
        ) {
            Ok(zkvm_executables) => {
                let shared_graph = package_graph.map(Arc::new);
                for exe in zkvm_executables {
                    println!(
                        "\n     Running zkVM tests for `{}` ({})",
                        exe.target_name,
                        exe.elf_path.display()
                    );

                    let mut runner_args = runner_options.clone();
                    runner_args.elf_path = exe.elf_path.display().to_string();
                    runner_args.package_id = Some(exe.package_id.repr().to_string());
                    runner_args.target_name = Some(exe.target_name);
                    runner_args.target_kind = Some(exe.target_kind.as_str().to_string());
                    runner_args.package_graph = shared_graph.clone();

                    let code = run(runner_args).await;
                    if code != 0 {
                        overall_exit_code = code;
                    }
                }
            },
            Err(code) => {
                overall_exit_code = code;
            },
        }
    }

    overall_exit_code
}

fn parse_libtest_line(line: &str) -> Option<(String, crate::junit::TestCaseStatus)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("test ") {
        return None;
    }
    let rest = &trimmed[5..];
    if let Some((name, status_str)) = rest.split_once(" ... ") {
        let name = name.trim().to_string();
        let status = match status_str.trim() {
            "ok" => crate::junit::TestCaseStatus::Passed,
            "FAILED" => crate::junit::TestCaseStatus::Failed {
                message: String::new(),
                details: String::new(),
            },
            "ignored" => crate::junit::TestCaseStatus::Skipped,
            _ => return None,
        };
        return Some((name, status));
    }
    None
}

fn run_host_test(
    exe: &TestExecutable,
    runner: &RunnerArgs,
    color: Option<&str>,
    junit_path: Option<&Path>,
    package_graph: Option<&PackageGraph>,
) -> i32 {
    println!(
        "\n     Running host tests for `{}` ({})",
        exe.target_name,
        exe.elf_path.display()
    );

    let mut cmd = Command::new(&exe.elf_path);

    if let Some(graph) = package_graph
        && let Ok(meta) = graph.metadata(&exe.package_id)
        && let Some(manifest_dir) = meta.manifest_path().parent()
    {
        cmd.current_dir(manifest_dir.as_std_path());
        cmd.env("CARGO_MANIFEST_DIR", manifest_dir.as_std_path());
    }

    if runner.nocapture {
        cmd.arg("--nocapture");
    }
    if runner.exact {
        cmd.arg("--exact");
    }
    if runner.ignored {
        cmd.arg("--ignored");
    }
    if runner.include_ignored {
        cmd.arg("--include-ignored");
    }
    if runner.quiet {
        cmd.arg("-q");
    }
    if runner.list {
        cmd.arg("--list");
    }
    cmd.arg("--test-threads").arg(runner.test_threads.to_string());
    if let Some(color) = color {
        cmd.arg("--color").arg(color);
    }
    cmd.args(&runner.filters);

    if let Some(junit_path) = junit_path {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{BIN_NAME}: failed to execute host test binary {}: {e}", exe.elf_path.display());
                return 1;
            },
        };

        let stdout = child.stdout.take().expect("failed to open stdout");
        let stderr = child.stderr.take().expect("failed to open stderr");

        let mut stdout_lines = Vec::new();
        let stdout_reader = std::io::BufReader::new(stdout);
        for line in stdout_reader.lines().map_while(Result::ok) {
            println!("{line}");
            stdout_lines.push(line);
        }

        let mut stderr_content = String::new();
        let stderr_reader = std::io::BufReader::new(stderr);
        for line in stderr_reader.lines().map_while(Result::ok) {
            eprintln!("{line}");
            stderr_content.push_str(&line);
            stderr_content.push('\n');
        }

        let status = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
        let elapsed = start.elapsed().as_secs_f64();

        let mut cases = Vec::new();
        for line in &stdout_lines {
            if let Some((name, status)) = parse_libtest_line(line) {
                let is_fail = matches!(status, crate::junit::TestCaseStatus::Failed { .. });
                cases.push(crate::junit::JUnitTestCase {
                    name,
                    classname: format!("host::{}", exe.target_name),
                    time: 0.0,
                    cycles: 0,
                    record_bytes: 0,
                    status,
                    stdout: String::new(),
                    stderr: if is_fail { stderr_content.clone() } else { String::new() },
                });
            }
        }

        let suite = crate::junit::JUnitTestSuite {
            name: format!("host::{}", exe.target_name),
            time: elapsed,
            cases,
        };

        if let Err(e) = crate::junit::write_or_merge_junit_xml(junit_path, suite) {
            eprintln!("{BIN_NAME}: warning: failed to write JUnit report: {e}");
        }

        status
    } else {
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        match cmd.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("{BIN_NAME}: failed to execute host test binary {}: {e}", exe.elf_path.display());
                1
            },
        }
    }
}

fn compile_host_tests(
    packages: &[String],
    cargo_filtered: &[String],
    filterset: Option<&Filterset>,
    package_graph: Option<&PackageGraph>,
) -> Result<Vec<TestExecutable>, i32> {
    let mut cargo_cmd = Command::new("cargo");
    configure_cargo_test(&mut cargo_cmd, packages);
    append_cargo_args(&mut cargo_cmd, cargo_filtered, &[
        ARG_HOST,
        ARG_ALL_TARGETS,
        ARG_TARGET,
        ARG_TARGET_DIR,
        ARG_PACKAGE,
        ARG_WORKSPACE,
        ARG_EXCLUDE,
    ]);
    collect_test_executables(cargo_cmd, "host", filterset, package_graph)
}

fn compile_zkvm_tests(
    packages: &[String],
    cargo_filtered: &[String],
    build: ZkvmBuild<'_>,
    package_graph: Option<&PackageGraph>,
    filterset: Option<&Filterset>,
) -> Result<Vec<TestExecutable>, i32> {
    let mut cargo_cmd = Command::new("cargo");
    if let Some(tc) = build.toolchain {
        cargo_cmd.arg(tc);
    } else if let Some(vendor) = Vendor::from_target(build.target) {
        cargo_cmd.arg(vendor.toolchain());
    }

    configure_cargo_test(&mut cargo_cmd, packages);
    if !build.target_is_explicit {
        cargo_cmd.arg("--target").arg(build.target);
    }

    let ws_root = package_graph.map(|g| g.workspace().root().as_str());
    let target_dir = resolve_target_dir(build.target_dir.and_then(Path::to_str), ws_root);
    if build.target_dir.is_none() {
        cargo_cmd.arg("--target-dir").arg(&target_dir);
    }
    append_cargo_args(&mut cargo_cmd, cargo_filtered, &[
        ARG_HOST,
        ARG_ALL_TARGETS,
        ARG_PACKAGE,
        ARG_WORKSPACE,
        ARG_EXCLUDE,
    ]);
    collect_test_executables(cargo_cmd, "zkVM", filterset, package_graph)
}

struct ZkvmBuild<'a> {
    toolchain:          Option<&'a str>,
    target:             &'a str,
    target_is_explicit: bool,
    target_dir:         Option<&'a Path>,
}

fn configure_cargo_test(command: &mut Command, packages: &[String]) {
    command.args(["test", "--no-run", "--message-format=json"]);
    for package in packages {
        command.arg("-p").arg(package);
    }
}

fn collect_test_executables(
    mut command: Command,
    phase: &str,
    filterset: Option<&Filterset>,
    package_graph: Option<&PackageGraph>,
) -> Result<Vec<TestExecutable>, i32> {
    command.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut child = command.spawn().map_err(|err| {
        eprintln!("{BIN_NAME}: failed to run cargo test for {phase}: {err}");
        1
    })?;
    let stdout = child.stdout.take().expect("cargo stdout was configured as piped");

    let executables = std::io::BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Artifact>(&line).ok())
        .filter_map(Artifact::into_test_executable)
        .filter(|executable| {
            !matches!((filterset, package_graph), (Some(filterset), Some(graph))
            if should_prune_binary(
                filterset,
                graph,
                &executable.package_id,
                &executable.target_name,
                &executable.target_kind,
            ))
        })
        .collect();

    let status = child.wait().map_err(|err| {
        eprintln!("{BIN_NAME}: error waiting for {phase} cargo: {err}");
        1
    })?;
    if !status.success() {
        return Err(status.code().unwrap_or(1));
    }
    Ok(executables)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendor_from_target() {
        assert_eq!(Vendor::from_target("riscv64im-succinct-zkvm-elf"), Some(Vendor::Succinct));
        assert_eq!(Vendor::from_target("riscv32im-risc0-zkvm-elf"), Some(Vendor::Risc0));
        assert_eq!(Vendor::from_target("x86_64-apple-darwin"), None);

        assert_eq!(Vendor::Succinct.toolchain(), "+succinct");
        assert_eq!(Vendor::Risc0.toolchain(), "+risc0");
        assert_eq!(Vendor::Succinct.default_target(), DEFAULT_SP1_TARGET);
        assert_eq!(Vendor::Risc0.default_target(), DEFAULT_RISC0_TARGET);
    }

    #[test]
    fn test_flag_spec_matches_cli_definition() {
        let runner = FlagSpec::of::<RunnerOpts>();
        let cargo = FlagSpec::of::<CargoOpts>();

        for flag in [
            "--nocapture",
            "--no-capture",
            "--exact",
            "--list",
            "--profile",
            "--subcycles",
            "--opcodes",
            "-q",
            "--quiet",
        ] {
            assert_eq!(runner.follow(flag), Some(Follow::None), "{flag} should be a runner boolean");
        }
        for flag in [
            "--test-threads",
            "--prover",
            "--mode",
            "--proof-mode",
            "-E",
            "--expr",
            "--junit-path",
        ] {
            assert_eq!(
                runner.follow(flag),
                Some(Follow::Required),
                "{flag} should be a runner value flag"
            );
        }
        assert_eq!(runner.follow("--save-baseline"), Some(Follow::Optional));

        for flag in [
            "-p",
            "--package",
            "--exclude",
            "--target",
            "--target-dir",
            "--cargo-profile",
            "--features",
        ] {
            assert_eq!(
                cargo.follow(flag),
                Some(Follow::Required),
                "{flag} should be a cargo value flag"
            );
        }
        for flag in [
            "--release",
            "--workspace",
            "--all",
            "--lib",
            "--all-features",
            "--all-targets",
            "--host",
        ] {
            assert_eq!(cargo.follow(flag), Some(Follow::None), "{flag} should be a cargo boolean");
        }

        // Nothing may be claimed by both sides
        for flag in runner.flags.keys() {
            assert!(cargo.get(flag).is_none(), "{flag} is claimed by both CargoOpts and RunnerOpts");
        }
    }

    #[test]
    fn test_partition_args_message_format_value() {
        let args = ["--message-format", "json", "test_name"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (cargo_part, runner_part) = partition_args(args);
        assert_eq!(cargo_part, vec!["--message-format", "json"]);
        assert_eq!(runner_part, vec!["test_name"]);
    }

    #[test]
    fn test_partition_args_basic() {
        let args = vec![
            "-p".to_string(),
            "my-pkg".to_string(),
            "--nocapture".to_string(),
            "--release".to_string(),
            "test_something".to_string(),
        ];
        let (cargo_part, runner_part) = partition_args(args);
        assert_eq!(cargo_part, vec!["-p", "my-pkg", "--release"]);
        assert_eq!(runner_part, vec!["--nocapture", "test_something"]);
    }

    #[test]
    fn test_partition_args_inline_values() {
        let args = vec![
            "--package=foo".to_string(),
            "-E=test(bar)".to_string(),
            "--mode=core".to_string(),
            "--junit-path=/tmp/junit.xml".to_string(),
        ];
        let (cargo_part, runner_part) = partition_args(args);
        assert_eq!(cargo_part, vec!["--package=foo"]);
        assert_eq!(runner_part, vec!["-E=test(bar)", "--mode=core", "--junit-path=/tmp/junit.xml"]);
    }

    #[test]
    fn test_partition_args_profile_disambiguation() {
        // Cargo profile: `--profile release`
        let args1 = vec![
            "--profile".to_string(),
            "release".to_string(),
            "-p".to_string(),
            "foo".to_string(),
        ];
        let (c1, r1) = partition_args(args1);
        assert_eq!(c1, vec!["--profile", "release", "-p", "foo"]);
        assert!(r1.is_empty());

        // Runner profile: `--profile` (standalone)
        let args2 = vec!["-p".to_string(), "foo".to_string(), "--profile".to_string()];
        let (c2, r2) = partition_args(args2);
        assert_eq!(c2, vec!["-p", "foo"]);
        assert_eq!(r2, vec!["--profile"]);
    }

    #[test]
    fn test_partition_args_save_baseline() {
        // Optional value provided separately
        let args1 = vec!["--save-baseline".to_string(), "my_baseline".to_string()];
        let (_, r1) = partition_args(args1);
        assert_eq!(r1, vec!["--save-baseline", "my_baseline"]);

        // Inline value provided
        let args2 = vec!["--save-baseline=inline_base".to_string()];
        let (_, r2) = partition_args(args2);
        assert_eq!(r2, vec!["--save-baseline=inline_base"]);

        // Standalone before another flag
        let args3 = vec!["--save-baseline".to_string(), "--nocapture".to_string()];
        let (_, r3) = partition_args(args3);
        assert_eq!(r3, vec!["--save-baseline", "--nocapture"]);
    }

    #[test]
    fn test_partition_args_with_double_dash() {
        let raw = [
            "-p".to_string(),
            "sha256".to_string(),
            "--nocapture".to_string(),
            "--".to_string(),
            "--exact".to_string(),
            "test_foo".to_string(),
        ];
        let sep_idx = raw.iter().position(|a| a == "--").unwrap();
        let (cargo_part, mut runner_part) = partition_args(raw[..sep_idx].to_vec());
        runner_part.extend(raw[sep_idx + 1..].to_vec());

        assert_eq!(cargo_part, vec!["-p", "sha256"]);
        assert_eq!(runner_part, vec!["--nocapture", "--exact", "test_foo"]);
    }

    #[test]
    fn test_resolve_default_packages_in_workspace() {
        let graph = query_package_graph().expect("should resolve package graph");
        let ws_root = std::path::Path::new(graph.workspace().root().as_str());
        let pkgs = resolve_default_packages_from_dir(&graph, &[], ws_root);

        // Must include packages depending on zkvm-test
        assert!(pkgs.contains(&"zkpow-types".to_string()));
        assert!(pkgs.contains(&"zkpow-sha256".to_string()));

        // Must NOT include framework crates
        assert!(!pkgs.contains(&"zkvm-test".to_string()));
        assert!(!pkgs.contains(&"zkvm-test-macros".to_string()));
        assert!(!pkgs.contains(&"zkvm-test-runner".to_string()));
    }

    #[test]
    fn test_resolve_default_packages_with_excludes() {
        let graph = query_package_graph().expect("should resolve package graph");
        let ws_root = std::path::Path::new(graph.workspace().root().as_str());
        let pkgs = resolve_default_packages_from_dir(&graph, &["zkpow-types".to_string()], ws_root);

        assert!(!pkgs.contains(&"zkpow-types".to_string()));
        assert!(pkgs.contains(&"zkpow-sha256".to_string()));
    }

    #[test]
    fn test_prune_packages_by_filter() {
        let graph = query_package_graph().expect("should resolve package graph");
        let prune = |expr: &str| {
            let filterset = crate::filter::parse(expr, &graph).expect("valid expression");
            let mut pkgs = vec!["zkpow-types".to_string(), "zkpow-sha256".to_string()];
            prune_packages_by_filter(&mut pkgs, &graph, &filterset);
            pkgs
        };

        assert_eq!(prune("package(zkpow-sha256)"), vec!["zkpow-sha256".to_string()]);
        assert_eq!(prune("package(zkpow-types)"), vec!["zkpow-types".to_string()]);
        // No target in either package is a benchmark, so both are pruned upfront
        assert!(prune("kind(bench)").is_empty());
    }

    #[test]
    fn test_parse_typed_options() {
        let args: Vec<String> = ["--target", "riscv64", "--exclude=foo", "--exclude", "bar", "-q"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let (cargo_args, runner_args) = partition_args(args);
        let cargo = parse_cargo_options(&cargo_args).unwrap();
        let runner = parse_options_from(runner_args).unwrap();

        assert_eq!(cargo.target.as_deref(), Some("riscv64"));
        assert_eq!(cargo.exclude, vec!["foo", "bar"]);
        assert!(cargo.bin.is_empty());
        assert!(runner.quiet);
    }

    #[test]
    fn test_partition_args_cargo_profile() {
        // `--cargo-profile` takes a value, which must not be mistaken for a test filter
        let args = ["--cargo-profile", "bench", "test_name"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (cargo_part, runner_part) = partition_args(args);
        assert_eq!(cargo_part, vec!["--cargo-profile", "bench"]);
        assert_eq!(runner_part, vec!["test_name"]);
    }

    #[test]
    fn test_parse_invalid_filter_expression() {
        let graph = query_package_graph().expect("should resolve package graph");
        let err = crate::filter::parse("test(unclosed", &graph).unwrap_err();
        assert!(err.contains("invalid filter expression"), "{err}");
    }

    #[test]
    fn test_parse_libtest_line() {
        use crate::junit::TestCaseStatus;

        let pass_line = "test tests::my_test ... ok";
        let (name, status) = parse_libtest_line(pass_line).expect("should parse ok");
        assert_eq!(name, "tests::my_test");
        assert!(matches!(status, TestCaseStatus::Passed));

        let fail_line = "test tests::failing_test ... FAILED";
        let (name, status) = parse_libtest_line(fail_line).expect("should parse failed");
        assert_eq!(name, "tests::failing_test");
        assert!(matches!(status, TestCaseStatus::Failed { .. }));

        let ignored_line = "test tests::slow_test ... ignored";
        let (name, status) = parse_libtest_line(ignored_line).expect("should parse ignored");
        assert_eq!(name, "tests::slow_test");
        assert!(matches!(status, TestCaseStatus::Skipped));

        assert!(parse_libtest_line("running 3 tests").is_none());
        assert!(parse_libtest_line("test result: ok. 1 passed; 0 failed").is_none());
    }
}
