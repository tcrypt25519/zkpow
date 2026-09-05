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
use guppy::{graph::cargo::BuildPlatform, graph::PackageGraph, MetadataCommand};
use nextest_filtering::{
    BinaryQuery, CompiledExpr, EvalContext, Filterset, FiltersetKind, KnownGroups, ParseContext,
};
use nextest_metadata::{RustBinaryId, RustTestBinaryKind};
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
pub const TARGET_VENDOR_SUCCINCT: &str = "succinct";
pub const TARGET_VENDOR_RISC0: &str = "risc0";

pub const DEFAULT_SP1_TARGET: &str = "riscv64im-succinct-zkvm-elf";
pub const DEFAULT_RISC0_TARGET: &str = "riscv32im-risc0-zkvm-elf";

pub const TOOLCHAIN_SUCCINCT: &str = "+succinct";
pub const TOOLCHAIN_RISC0: &str = "+risc0";

pub const REASON_COMPILER_ARTIFACT: &str = "compiler-artifact";
pub const DEFAULT_TARGET_NAME: &str = "test";
pub const DEFAULT_TARGET_KIND: &str = "test";

pub const ZKVM_TEST_CRATE: &str = "zkvm-test";
pub const ZKVM_TEST_CRATE_ALT: &str = "zkvm_test";
pub const ZKVM_TEST_MACROS_CRATE: &str = "zkvm-test-macros";
pub const ZKVM_TEST_RUNNER_CRATE: &str = "zkvm-test-runner";
pub const ZKVM_SUBDIR: &str = "zkvm";
pub const DEFAULT_TARGET_DIR: &str = "target/zkvm";
pub const METADATA_KEY_ZKVM_TEST: &str = "zkvm-test";
pub const METADATA_KEY_ZKVM: &str = "zkvm";

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toolchain {
    Risc0      = 0,
    Succinct32 = 1,
    Succinct64 = 2,
}

impl Toolchain {
    pub const fn selector(&self) -> &'static str {
        match self {
            Self::Risc0 => TOOLCHAIN_RISC0,
            Self::Succinct32 => "+succinct32",
            Self::Succinct64 => TOOLCHAIN_SUCCINCT,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Risc0    = 0,
    Succinct = 1,
}

impl Vendor {
    const RISC0_TOOLCHAINS: [Toolchain; 1] = [Toolchain::Risc0];
    const SUCCINCT_TOOLCHAINS: [Toolchain; 2] = [Toolchain::Succinct32, Toolchain::Succinct64];

    pub const fn toolchains(&self) -> &'static [Toolchain] {
        match self {
            Self::Risc0 => &Self::RISC0_TOOLCHAINS,
            Self::Succinct => &Self::SUCCINCT_TOOLCHAINS,
        }
    }

    pub const fn default_toolchain(&self) -> Toolchain {
        match self {
            Self::Risc0 => Toolchain::Risc0,
            Self::Succinct => Toolchain::Succinct64,
        }
    }

    pub const fn default_target(&self) -> &'static str {
        match self {
            Self::Risc0 => DEFAULT_RISC0_TARGET,
            Self::Succinct => DEFAULT_SP1_TARGET,
        }
    }

    pub fn from_target(target: &str) -> Option<Self> {
        if target.contains(TARGET_VENDOR_SUCCINCT) {
            Some(Self::Succinct)
        } else if target.contains(TARGET_VENDOR_RISC0) {
            Some(Self::Risc0)
        } else {
            None
        }
    }
}

pub fn default_toolchain_for_target(vendor: Vendor) -> Toolchain {
    vendor.default_toolchain()
}

pub fn default_toolchain_for_target_str(target: &str) -> Option<&'static str> {
    Vendor::from_target(target).map(|v| default_toolchain_for_target(v).selector())
}

pub const RUNNER_BOOLEAN_FLAGS: &[&str] = &[
    "--nocapture",
    "--no-capture",
    "--exact",
    "--list",
    "--ignored",
    "--include-ignored",
    "--subcycles",
    "--instructions",
    "--opcodes",
    "--accept-snapshots",
    "--update-snapshots",
    "-q",
    "--quiet",
];

pub const RUNNER_VAL_FLAGS: &[&str] = &[
    "--test-threads",
    "--prover",
    "--compare-baseline",
    "--fail-on-regression",
    "--mode",
    "--proof-mode",
    "-E",
    "--expr",
    "--filter-expr",
    "--junit-path",
];

pub const CARGO_VAL_FLAGS: &[&str] = &[
    "-p",
    "--package",
    "--exclude",
    "--bin",
    "--test",
    "--example",
    "--bench",
    "--manifest-path",
    "--features",
    "--target",
    "--target-dir",
    "--color",
    "--message-format",
    "--config",
];

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

    if !is_workspace_root {
        if let Some(pkg) = graph.workspace().iter().find(|p| {
            if let Some(parent) = Path::new(p.manifest_path().as_str()).parent() {
                current_dir == parent || current_dir.starts_with(parent)
            } else {
                false
            }
        }) {
            if !cli_excludes.contains(&pkg.name().to_string()) && is_zkvm_pkg(&pkg) {
                return vec![pkg.name().to_string()];
            }
        }
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

pub fn partition_args(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut cargo_part = Vec::new();
    let mut runner_part = Vec::new();
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        let (key, inline_val) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };

        if key == "--profile" {
            // Disambiguate cargo `--profile <name>` vs runner `--profile` flag
            let is_cargo_profile = iter.peek().is_some_and(|next| {
                !next.starts_with('-') && ["dev", "test", "release", "bench"].contains(&next.as_str())
            });
            if is_cargo_profile {
                cargo_part.push(arg);
                cargo_part.push(iter.next().unwrap());
            } else {
                runner_part.push(arg);
            }
        } else if RUNNER_BOOLEAN_FLAGS.contains(&key) {
            runner_part.push(arg);
        } else if RUNNER_VAL_FLAGS.contains(&key) {
            runner_part.push(arg);
            if inline_val.is_none() {
                if let Some(val) = iter.next() {
                    runner_part.push(val);
                }
            }
        } else if CARGO_VAL_FLAGS.contains(&key) {
            cargo_part.push(arg);
            if inline_val.is_none() {
                if let Some(val) = iter.next() {
                    cargo_part.push(val);
                }
            }
        } else if key == "--save-baseline" {
            runner_part.push(arg);
            if inline_val.is_none() {
                if let Some(next) = iter.peek() {
                    if !next.starts_with('-') {
                        runner_part.push(iter.next().unwrap());
                    }
                }
            }
        } else if !key.starts_with('-') {
            // Positional filter
            runner_part.push(arg);
        } else {
            cargo_part.push(arg);
        }
    }
    (cargo_part, runner_part)
}

pub fn prune_packages_by_filter(
    packages: &mut Vec<String>,
    graph: &PackageGraph,
    filterset: &Filterset,
) {
    let eval_cx = EvalContext {
        default_filter: &CompiledExpr::ALL,
    };
    packages.retain(|name| {
        if let Some(pkg) = graph.workspace().iter().find(|p| p.name() == name) {
            pkg.build_targets().any(|bt| {
                let kind_str = match bt.id() {
                    guppy::graph::BuildTargetId::Library => "lib",
                    guppy::graph::BuildTargetId::BuildScript => "custom-build",
                    guppy::graph::BuildTargetId::Binary(_) => "bin",
                    guppy::graph::BuildTargetId::Example(_) => "example",
                    guppy::graph::BuildTargetId::Test(_) => "test",
                    guppy::graph::BuildTargetId::Benchmark(_) => "bench",
                    _ => "test",
                };
                let kind = RustTestBinaryKind::new(kind_str);
                let binary_id = RustBinaryId::from_parts(pkg.name(), &kind, bt.name());
                let query = BinaryQuery {
                    package_id: pkg.id(),
                    binary_id: &binary_id,
                    binary_name: bt.name(),
                    kind: &kind,
                    platform: BuildPlatform::Target,
                };
                filterset.matches_binary(&query, &eval_cx) != Some(false)
            })
        } else {
            true
        }
    });
}

pub fn should_prune_binary(
    filterset: &Filterset,
    graph: &PackageGraph,
    pkg_id: &guppy::PackageId,
    target_name: &str,
    target_kind: &RustTestBinaryKind,
) -> bool {
    if let Ok(pkg) = graph.metadata(pkg_id) {
        let binary_id = RustBinaryId::from_parts(pkg.name(), target_kind, target_name);
        let query = BinaryQuery {
            package_id: pkg_id,
            binary_id: &binary_id,
            binary_name: target_name,
            kind: target_kind,
            platform: BuildPlatform::Target,
        };
        let eval_cx = EvalContext {
            default_filter: &CompiledExpr::ALL,
        };
        filterset.matches_binary(&query, &eval_cx) == Some(false)
    } else {
        false
    }
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

    // Check if packages or specific targets were explicitly provided
    let has_explicit_packages = cargo_filtered.iter().any(|a| {
        a == "-p" || a == "--package" || a.starts_with("-p=") || a.starts_with("--package=")
    });
    let has_explicit_targets = cargo_filtered.iter().any(|a| {
        a == "--bin"
            || a.starts_with("--bin=")
            || a == "--test"
            || a.starts_with("--test=")
            || a == "--example"
            || a.starts_with("--example=")
    });

    // Extract any CLI --exclude args
    let mut cli_excludes = Vec::new();
    let mut idx = 0;
    while idx < cargo_filtered.len() {
        if cargo_filtered[idx] == "--exclude" {
            if let Some(pkg) = cargo_filtered.get(idx + 1) {
                cli_excludes.push(pkg.clone());
            }
        } else if let Some(pkg) = cargo_filtered[idx].strip_prefix("--exclude=") {
            cli_excludes.push(pkg.to_string());
        }
        idx += 1;
    }

    let package_graph = query_package_graph();

    // Extract filter expression if present
    let filter_expr_str = runner_raw
        .iter()
        .position(|a| a == "-E" || a == "--expr" || a == "--filter-expr")
        .and_then(|i| runner_raw.get(i + 1).cloned())
        .or_else(|| {
            runner_raw.iter().find_map(|a| {
                a.strip_prefix("-E=")
                    .or_else(|| a.strip_prefix("--expr="))
                    .or_else(|| a.strip_prefix("--filter-expr="))
                    .map(str::to_string)
            })
        });

    let junit_path_opt = runner_raw
        .iter()
        .position(|a| a == "--junit-path")
        .and_then(|i| runner_raw.get(i + 1).map(PathBuf::from))
        .or_else(|| {
            runner_raw
                .iter()
                .find_map(|a| a.strip_prefix("--junit-path=").map(PathBuf::from))
        });

    let filterset = if let (Some(expr_str), Some(graph)) = (&filter_expr_str, &package_graph) {
        let parse_ctx = ParseContext::new(graph);
        let known_groups = KnownGroups::Known {
            custom_groups: std::collections::HashSet::new(),
        };
        match Filterset::parse(
            expr_str.clone(),
            &parse_ctx,
            FiltersetKind::Test,
            &known_groups,
        ) {
            Ok(f) => Some(f),
            Err(err) => {
                let err_msgs = err
                    .errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                eprintln!("{BIN_NAME}: invalid filter expression '{}': {err_msgs}", err.input);
                return 2;
            },
        }
    } else {
        None
    };

    let mut auto_packages = Vec::new();
    if !has_explicit_packages && !has_explicit_targets {
        if let Some(ref graph) = package_graph {
            auto_packages = resolve_default_packages(graph, &cli_excludes);

            // If a filterset is present, evaluate whether any binaries in each package could possibly match.
            // If matches_binary returns Some(false) for all targets in a package, prune it upfront!
            if let Some(ref fs) = filterset {
                prune_packages_by_filter(&mut auto_packages, graph, fs);

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

            let joined = auto_packages.join(", ");
            println!(
                "{BIN_NAME}: testing {} zkVM package{}: {joined}",
                auto_packages.len(),
                if auto_packages.len() == 1 { "" } else { "s" }
            );
        }
    }

    // Clean up cargo_filtered: remove --workspace, --all, and --exclude if we resolved specific packages
    if !auto_packages.is_empty() {
        let mut cleaned = Vec::new();
        let mut skip_next = false;
        for a in cargo_filtered {
            if skip_next {
                skip_next = false;
                continue;
            }
            if a == "--workspace" || a == "--all" {
                continue;
            }
            if a == "--exclude" {
                skip_next = true;
                continue;
            }
            if a.starts_with("--exclude=") {
                continue;
            }
            cleaned.push(a);
        }
        cargo_filtered = cleaned;
    }

    let target = cargo_filtered
        .iter()
        .position(|a| a == "--target")
        .and_then(|i| cargo_filtered.get(i + 1).cloned())
        .or_else(|| {
            cargo_filtered
                .iter()
                .find_map(|a| a.strip_prefix("--target=").map(str::to_string))
        })
        .unwrap_or_else(|| DEFAULT_SP1_TARGET.to_string());

    let mut cargo_cmd = Command::new("cargo");
    if let Some(tc) = toolchain {
        cargo_cmd.arg(tc);
    } else if let Some(vendor) = Vendor::from_target(&target) {
        cargo_cmd.arg(default_toolchain_for_target(vendor).selector());
    }

    cargo_cmd.arg("test");
    cargo_cmd.arg("--no-run");
    if !cargo_filtered.iter().any(|a| a == "--target" || a.starts_with("--target=")) {
        cargo_cmd.arg("--target").arg(&target);
    }

    // Isolate zkVM build artifacts to target/zkvm by default to avoid cache
    // invalidation collisions with the host Rust toolchain.
    let explicit_target_dir = cargo_filtered
        .iter()
        .position(|a| a == "--target-dir")
        .and_then(|i| cargo_filtered.get(i + 1).map(String::as_str))
        .or_else(|| {
            cargo_filtered
                .iter()
                .find_map(|a| a.strip_prefix("--target-dir="))
        });

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
            .unwrap_or_else(|| DEFAULT_TARGET_KIND.to_string());
        let target_kind = RustTestBinaryKind::new(target_kind_str);
        let package_id = guppy::PackageId::new(artifact.package_id);
        let elf_path = PathBuf::from(path);

        // Binary-level filter evaluation: skip executable if matches_binary is Some(false)
        if let (Some(fs), Some(graph)) = (&filterset, &package_graph) {
            if should_prune_binary(fs, graph, &package_id, &target_name, &target_kind) {
                continue;
            }
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

    // If --junit-path is provided, remove any pre-existing file from previous runs so this run starts fresh
    if let Some(ref junit_path) = junit_path_opt {
        if let Err(err) = remove_file_if_exists(junit_path) {
            eprintln!("{BIN_NAME}: warning: failed to remove existing JUnit file '{}': {err}", junit_path.display());
        }
    }

    let shared_graph = package_graph.map(Arc::new);
    let mut overall_exit_code = 0;
    for exe in test_executables {
        println!("\n     Running zkVM tests for `{}` ({})", exe.target_name, exe.elf_path.display());

        let mut args_list = vec![exe.elf_path.display().to_string()];
        args_list.extend(runner_raw.clone());

        let mut runner_args = crate::runner::parse_args_from(args_list);
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
    fn test_default_toolchain_for_target() {
        assert_eq!(
            Vendor::from_target("riscv64im-succinct-zkvm-elf").map(default_toolchain_for_target),
            Some(Toolchain::Succinct64)
        );
        assert_eq!(
            Vendor::from_target("riscv32im-risc0-zkvm-elf").map(default_toolchain_for_target),
            Some(Toolchain::Risc0)
        );
        assert_eq!(
            Vendor::from_target("x86_64-apple-darwin").map(default_toolchain_for_target),
            None
        );
        assert_eq!(
            default_toolchain_for_target_str("riscv64im-succinct-zkvm-elf"),
            Some("+succinct")
        );
        assert_eq!(
            default_toolchain_for_target_str("riscv32im-risc0-zkvm-elf"),
            Some("+risc0")
        );
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
        let raw = vec![
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
        let parse_ctx = ParseContext::new(&graph);
        let known_groups = KnownGroups::Known {
            custom_groups: std::collections::HashSet::new(),
        };

        // Filter selecting only zkpow-sha256
        let fs = Filterset::parse(
            "package(zkpow-sha256)".to_string(),
            &parse_ctx,
            FiltersetKind::Test,
            &known_groups,
        ).unwrap();

        let mut pkgs = vec!["zkpow-types".to_string(), "zkpow-sha256".to_string()];
        prune_packages_by_filter(&mut pkgs, &graph, &fs);

        assert_eq!(pkgs, vec!["zkpow-sha256".to_string()]);

        // Filter selecting only zkpow-types
        let fs_types = Filterset::parse(
            "package(zkpow-types)".to_string(),
            &parse_ctx,
            FiltersetKind::Test,
            &known_groups,
        ).unwrap();

        let mut pkgs2 = vec!["zkpow-types".to_string(), "zkpow-sha256".to_string()];
        prune_packages_by_filter(&mut pkgs2, &graph, &fs_types);

        assert_eq!(pkgs2, vec!["zkpow-types".to_string()]);
    }
}
