//! `cargo-zkvm-test` CLI plugin logic and helpers.
//!
//! Provides CLI argument partitioning, toolchain selection, workspace
//! inspection, Nextest filter pruning, and test execution orchestration.

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use clap::{Args, CommandFactory, Parser};
use guppy::{MetadataCommand, graph::PackageGraph};
use nextest_filtering::Filterset;
use nextest_metadata::RustTestBinaryKind;
use serde::Deserialize;

use crate::runner::run;

#[derive(Debug, Deserialize)]
pub struct Artifact {
    pub reason:     String,
    #[serde(default)]
    pub package_id: String,
    pub target:     Option<ArtifactTarget>,
    pub executable: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ArtifactTarget {
    pub name: String,
    pub kind: Vec<String>,
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

/// Cargo flags that take a value but are not modelled in `CargoOpts`, either because
/// this plugin sets them itself or because they only ever get passed straight through.
pub const EXTRA_CARGO_VAL_FLAGS: &[&str] = &["--message-format", "--jobs", "-j"];

/// How an argument consumes the words that follow it.
#[derive(Debug, Default)]
pub struct FlagSpec {
    /// Flags that stand alone.
    pub booleans: Vec<String>,
    /// Flags that always consume the next word.
    pub valued:   Vec<String>,
    /// Flags whose value may be omitted, so the next word is only theirs if it is not
    /// itself a flag.
    pub optional: Vec<String>,
}

impl FlagSpec {
    /// Read the spec off the CLI definition itself, so a flag cannot be advertised in
    /// `--help` and then mis-routed here.
    pub fn of<A: Args>() -> Self {
        let command = A::augment_args(clap::Command::new("spec"));
        let mut spec = Self::default();

        for arg in command.get_arguments().filter(|arg| !arg.is_positional()) {
            let names: Vec<String> = arg
                .get_short_and_visible_aliases()
                .unwrap_or_default()
                .iter()
                .map(|short| format!("-{short}"))
                .chain(
                    arg.get_long_and_visible_aliases()
                        .unwrap_or_default()
                        .iter()
                        .map(|long| format!("--{long}")),
                )
                .collect();

            let num_args = arg.get_num_args();
            let takes_value = num_args.map_or(arg.get_action().takes_values(), |range| range.takes_values());
            let optional = num_args.is_some_and(|range| range.takes_values() && range.min_values() == 0);

            match (takes_value, optional) {
                (false, _) => spec.booleans.extend(names),
                (true, false) => spec.valued.extend(names),
                (true, true) => spec.optional.extend(names),
            }
        }
        spec
    }

    fn contains(list: &[String], key: &str) -> bool {
        list.iter().any(|name| name == key)
    }
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

    /// Test all targets
    #[arg(long)]
    pub all_targets: bool,

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
    pub fail_on_regression: Option<String>,

    /// Accept/update golden snapshot files for tests with #[test(snapshot = "...")]
    #[arg(long, visible_alias = "update-snapshots")]
    pub accept_snapshots: bool,

    /// Number of test instances to execute concurrently
    #[arg(long, value_name = "N")]
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

    pkg.direct_links().any(|link| {
        link.to().name() == ZKVM_TEST_CRATE || link.to().name() == ZKVM_TEST_CRATE_ALT
    })
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
        && !cli_excludes.contains(&pkg.name().to_string())
        && is_zkvm_pkg(&pkg)
    {
        return vec![pkg.name().to_string()];
    }

    let ws_metadata = graph.workspace().metadata_table();
    let ws_zkvm_meta = ws_metadata
        .get(METADATA_KEY_ZKVM_TEST)
        .or_else(|| ws_metadata.get(METADATA_KEY_ZKVM));

    let ws_explicit_packages: Option<Vec<String>> = ws_zkvm_meta.and_then(|m| {
        m.get("packages").and_then(|p| p.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
        })
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
        if ws_excludes.contains(&pkg.name().to_string()) || cli_excludes.contains(&pkg.name().to_string()) {
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
    let runner = FlagSpec::of::<RunnerOpts>();
    let cargo = FlagSpec::of::<CargoOpts>();

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
        } else if FlagSpec::contains(&runner.booleans, &key) {
            (&mut runner_part, Follow::None)
        } else if FlagSpec::contains(&runner.valued, &key) {
            (&mut runner_part, Follow::Required)
        } else if FlagSpec::contains(&runner.optional, &key) {
            (&mut runner_part, Follow::Optional)
        } else if FlagSpec::contains(&cargo.valued, &key) || EXTRA_CARGO_VAL_FLAGS.contains(&key.as_str()) {
            (&mut cargo_part, Follow::Required)
        } else if FlagSpec::contains(&cargo.optional, &key) {
            (&mut cargo_part, Follow::Optional)
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

enum Follow {
    None,
    Required,
    Optional,
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
    let binary = crate::filter::Binary::new(
        pkg.name(),
        pkg_id.clone(),
        target_name.to_string(),
        target_kind.clone(),
    );
    !binary.can_match(filterset)
}

/// Every value given for `names`, accepting both `--flag value` and `--flag=value`.
fn flag_values<'a>(args: &'a [String], names: &[&str]) -> Vec<&'a str> {
    let mut values = Vec::new();
    for (idx, arg) in args.iter().enumerate() {
        match arg.split_once('=') {
            Some((key, value)) if names.contains(&key) => values.push(value),
            None if names.contains(&arg.as_str()) => values.extend(args.get(idx + 1).map(String::as_str)),
            _ => {},
        }
    }
    values
}

fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    flag_values(args, names).into_iter().next()
}

/// Whether any of `names` appears in `args`, with or without an inline value.
fn flag_present(args: &[String], names: &[&str]) -> bool {
    args.iter().any(|arg| {
        let key = arg.split_once('=').map_or(arg.as_str(), |(k, _)| k);
        names.contains(&key)
    })
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

    // Split args around `--` if present
    let (cargo_raw, runner_raw) = if let Some(sep_idx) = raw_args.iter().position(|a| a == "--") {
        let pre_sep = raw_args[..sep_idx].to_vec();
        let post_sep = raw_args[sep_idx + 1..].to_vec();
        let (cargo_part, mut runner_part) = partition_args(pre_sep);
        runner_part.extend(post_sep);
        (cargo_part, runner_part)
    } else {
        partition_args(raw_args)
    };

    // Determine toolchain and target
    let toolchain = cargo_raw.iter().find(|a| a.starts_with('+')).cloned();
    let mut cargo_filtered: Vec<String> = cargo_raw.into_iter().filter(|a| !a.starts_with('+')).collect();

    let has_explicit_packages = flag_present(&cargo_filtered, &["-p", "--package"]);
    let has_explicit_targets = flag_present(&cargo_filtered, &["--bin", "--test", "--example"]);
    let cli_excludes: Vec<String> = flag_values(&cargo_filtered, &["--exclude"])
        .into_iter()
        .map(str::to_string)
        .collect();

    let package_graph = query_package_graph();
    let filter_expr_str = flag_value(&runner_raw, &["-E", "--expr", "--filter-expr"]).map(str::to_string);
    let junit_path_opt = flag_value(&runner_raw, &["--junit-path"]).map(PathBuf::from);

    let filterset = match (&filter_expr_str, &package_graph) {
        (Some(expr), Some(graph)) => match crate::filter::parse(expr, graph) {
            Ok(filterset) => Some(filterset),
            Err(err) => {
                eprintln!("{BIN_NAME}: {err}");
                return 2;
            },
        },
        _ => None,
    };

    let mut auto_packages = Vec::new();
    if !has_explicit_packages
        && !has_explicit_targets
        && let Some(ref graph) = package_graph
    {
        auto_packages = resolve_default_packages(graph, &cli_excludes);

        // A package whose every target the filterset rules out is dropped before it is
        // ever compiled.
        if let Some(ref filterset) = filterset {
            prune_packages_by_filter(&mut auto_packages, graph, filterset);
            if auto_packages.is_empty() {
                println!("{BIN_NAME}: 0 packages matched filter expression");
                return 0;
            }
        }

        if auto_packages.is_empty() {
            eprintln!("{BIN_NAME}: no zkVM test packages found in workspace.");
            eprintln!("hint: add `zkvm-test` to [dev-dependencies] or configure in Cargo.toml:");
            eprintln!("    [workspace.metadata.zkvm-test]");
            eprintln!("    packages = [\"my-guest-crate\"]");
            return 1;
        }

        println!(
            "{BIN_NAME}: testing {} zkVM package{}: {}",
            auto_packages.len(),
            if auto_packages.len() == 1 { "" } else { "s" },
            auto_packages.join(", ")
        );
    }

    // The resolved package list supersedes the selection flags that produced it.
    if !auto_packages.is_empty() {
        let mut cleaned = Vec::new();
        let mut iter = cargo_filtered.into_iter();
        while let Some(arg) = iter.next() {
            let drops_value = {
                let key = arg.split_once('=').map_or(arg.as_str(), |(key, _)| key);
                match key {
                    "--workspace" | "--all" => Some(false),
                    "--exclude" => Some(!arg.contains('=')),
                    _ => None,
                }
            };
            match drops_value {
                Some(true) => {
                    let _ = iter.next();
                },
                Some(false) => {},
                None => cleaned.push(arg),
            }
        }
        cargo_filtered = cleaned;
    }

    // `--cargo-profile` exists only to disambiguate cargo's `--profile` from the runner's
    // flag of the same name; cargo itself takes `--profile`.
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

    let target = flag_value(&cargo_filtered, &["--target"])
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_SP1_TARGET.to_string());

    let mut cargo_cmd = Command::new("cargo");
    if let Some(tc) = toolchain {
        cargo_cmd.arg(tc);
    } else if let Some(vendor) = Vendor::from_target(&target) {
        cargo_cmd.arg(vendor.toolchain());
    }

    cargo_cmd.arg("test");
    cargo_cmd.arg("--no-run");
    if !flag_present(&cargo_filtered, &["--target"]) {
        cargo_cmd.arg("--target").arg(&target);
    }

    // Isolate zkVM build artifacts to target/zkvm by default to avoid cache
    // invalidation collisions with the host Rust toolchain.
    let explicit_target_dir = flag_value(&cargo_filtered, &["--target-dir"]);

    let ws_root = package_graph.as_ref().map(|g| g.workspace().root().as_str());
    let target_dir = resolve_target_dir(explicit_target_dir, ws_root);
    if explicit_target_dir.is_none() {
        cargo_cmd.arg("--target-dir").arg(&target_dir);
    }

    for pkg in &auto_packages {
        cargo_cmd.arg("-p").arg(pkg);
    }

    cargo_cmd.arg("--message-format=json");
    cargo_cmd.args(&cargo_filtered);

    cargo_cmd.stdout(Stdio::piped());
    cargo_cmd.stderr(Stdio::inherit());

    let mut child = match cargo_cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("{BIN_NAME}: failed to run cargo test: {err}");
            return 1;
        },
    };

    let stdout = child.stdout.take().expect("failed to open stdout");
    let reader = std::io::BufReader::new(stdout);

    let mut test_executables = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        let Ok(artifact) = serde_json::from_str::<Artifact>(&line) else {
            continue;
        };
        if artifact.reason != REASON_COMPILER_ARTIFACT {
            continue;
        }
        let Some(path) = artifact.executable else {
            continue;
        };
        let target_name = artifact
            .target
            .as_ref()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| DEFAULT_TARGET_NAME.to_string());
        let target_kind_str = artifact
            .target
            .as_ref()
            .and_then(|t| t.kind.first().cloned())
            .unwrap_or_else(|| crate::filter::DEFAULT_TARGET_KIND.to_string());
        let target_kind = RustTestBinaryKind::new(target_kind_str);
        let package_id = guppy::PackageId::new(artifact.package_id);
        let elf_path = PathBuf::from(path);

        // Skip an executable the filterset rules out entirely
        if let (Some(filterset), Some(graph)) = (&filterset, &package_graph)
            && should_prune_binary(filterset, graph, &package_id, &target_name, &target_kind)
        {
            continue;
        }

        test_executables.push(TestExecutable {
            target_name,
            target_kind,
            package_id,
            elf_path,
        });
    }

    let status = match child.wait() {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{BIN_NAME}: error waiting for cargo: {err}");
            return 1;
        },
    };

    if !status.success() {
        return status.code().unwrap_or(1);
    }

    if test_executables.is_empty() {
        println!("{BIN_NAME}: no test binaries compiled for zkVM target");
        return 0;
    }

    // Each run starts a fresh JUnit report rather than merging into the previous run's
    if let Some(ref junit_path) = junit_path_opt
        && let Err(err) = remove_file_if_exists(junit_path)
    {
        eprintln!("{BIN_NAME}: warning: failed to remove existing JUnit file '{}': {err}", junit_path.display());
    }

    let shared_graph = package_graph.map(Arc::new);
    let mut overall_exit_code = 0;
    for exe in test_executables {
        println!("\n     Running zkVM tests for `{}` ({})", exe.target_name, exe.elf_path.display());

        let mut args_list = vec![exe.elf_path.display().to_string()];
        args_list.extend(runner_raw.clone());

        let mut runner_args = match crate::runner::parse_args_from(args_list) {
            Ok(args) => args,
            Err(err) => {
                eprintln!("{BIN_NAME}: {err}");
                return 2;
            },
        };
        runner_args.package_id = Some(exe.package_id.repr().to_string());
        runner_args.target_name = Some(exe.target_name);
        runner_args.target_kind = Some(exe.target_kind.as_str().to_string());
        runner_args.package_graph = shared_graph.clone();

        let code = run(runner_args).await;
        if code != 0 {
            overall_exit_code = code;
        }
    }

    overall_exit_code
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

        for flag in ["--nocapture", "--no-capture", "--exact", "--list", "--profile", "--subcycles", "--opcodes", "-q", "--quiet"] {
            assert!(runner.booleans.iter().any(|f| f == flag), "{flag} should be a runner boolean");
        }
        for flag in ["--test-threads", "--prover", "--mode", "--proof-mode", "-E", "--expr", "--junit-path"] {
            assert!(runner.valued.iter().any(|f| f == flag), "{flag} should be a runner value flag");
        }
        assert_eq!(runner.optional, vec!["--save-baseline".to_string()]);

        for flag in ["-p", "--package", "--exclude", "--target", "--target-dir", "--cargo-profile", "--features"] {
            assert!(cargo.valued.iter().any(|f| f == flag), "{flag} should be a cargo value flag");
        }
        for flag in ["--release", "--workspace", "--all", "--lib", "--all-features"] {
            assert!(cargo.booleans.iter().any(|f| f == flag), "{flag} should be a cargo boolean");
        }

        // Nothing may be claimed by both sides
        for flag in runner.booleans.iter().chain(&runner.valued).chain(&runner.optional) {
            assert!(
                !cargo.booleans.contains(flag) && !cargo.valued.contains(flag),
                "{flag} is claimed by both CargoOpts and RunnerOpts"
            );
        }
    }

    #[test]
    fn test_partition_args_message_format_value() {
        let args = ["--message-format", "json", "test_name"].iter().map(|s| s.to_string()).collect();
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
        assert_eq!(
            runner_part,
            vec!["-E=test(bar)", "--mode=core", "--junit-path=/tmp/junit.xml"]
        );
    }

    #[test]
    fn test_partition_args_profile_disambiguation() {
        // Cargo profile: `--profile release`
        let args1 = vec!["--profile".to_string(), "release".to_string(), "-p".to_string(), "foo".to_string()];
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
    fn test_flag_value_forms() {
        let args: Vec<String> = ["--target", "riscv64", "--exclude=foo", "--exclude", "bar", "-q"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(flag_value(&args, &["--target"]), Some("riscv64"));
        assert_eq!(flag_values(&args, &["--exclude"]), vec!["foo", "bar"]);
        assert_eq!(flag_value(&args, &["--missing"]), None);

        assert!(flag_present(&args, &["--exclude"]));
        assert!(flag_present(&args, &["-q", "--quiet"]));
        assert!(!flag_present(&args, &["--bin"]));
    }

    #[test]
    fn test_partition_args_cargo_profile() {
        // `--cargo-profile` takes a value, which must not be mistaken for a test filter
        let args = ["--cargo-profile", "bench", "test_name"].iter().map(|s| s.to_string()).collect();
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
}
