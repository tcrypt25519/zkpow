//! `cargo-zkvm-test` CLI plugin
//!
//! Provides a seamless `cargo test`-like experience for SP1 zkVM tests.
//! Compiles the requested packages/tests for `riscv64im-succinct-zkvm-elf`,
//! discovers the test ELFs, and runs them with full per-test isolation,
//! concurrency, cycle counts, and guest output capture.

use std::{
    io::BufRead,
    process::{Command, Stdio},
};

use serde::Deserialize;
use zkvm_test_runner::runner::run;

#[derive(Debug, Deserialize)]
struct Artifact {
    reason: String,
    target: Option<ArtifactTarget>,
    executable: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ArtifactTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetaPackage>,
    workspace_members: Vec<String>,
    workspace_root: String,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct MetaPackage {
    id: String,
    name: String,
    manifest_path: String,
    #[serde(default)]
    dependencies: Vec<MetaDependency>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct MetaDependency {
    name: String,
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

const RUNNER_BOOLEAN_FLAGS: &[&str] = &[
    "--nocapture",
    "--no-capture",
    "--exact",
    "--list",
    "--ignored",
    "--include-ignored",
    "--subcycles",
    "--profile",
    "--instructions",
    "--opcodes",
    "--accept-snapshots",
    "--update-snapshots",
    "-q",
    "--quiet",
];

const RUNNER_VAL_FLAGS: &[&str] = &[
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

const CARGO_VAL_FLAGS: &[&str] = &[
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

pub fn default_toolchain_for_target(target: &str) -> Option<&'static str> {
    if target.contains(TARGET_VENDOR_SUCCINCT) {
        Some(TOOLCHAIN_SUCCINCT)
    } else if target.contains(TARGET_VENDOR_RISC0) {
        Some(TOOLCHAIN_RISC0)
    } else {
        None
    }
}

fn query_cargo_metadata() -> Option<CargoMetadata> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version=1"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn resolve_default_packages(metadata: &CargoMetadata, cli_excludes: &[String]) -> Vec<String> {
    const FRAMEWORK_CRATES: &[&str] = &["zkvm-test", "zkvm-test-macros", "zkvm-test-runner"];

    // 1. If running from within a specific package directory (not workspace root), target that package.
    let current_dir = std::env::current_dir().unwrap_or_default();
    let is_workspace_root = current_dir == std::path::Path::new(&metadata.workspace_root);

    if !is_workspace_root {
        if let Some(pkg) = metadata.packages.iter().find(|p| {
            if let Some(parent) = std::path::Path::new(&p.manifest_path).parent() {
                current_dir == parent || current_dir.starts_with(parent)
            } else {
                false
            }
        }) {
            if !cli_excludes.contains(&pkg.name) {
                return vec![pkg.name.clone()];
            }
        }
    }

    // 2. Check [workspace.metadata.zkvm-test] or [workspace.metadata.zkvm]
    let ws_zkvm_meta = metadata
        .metadata
        .as_ref()
        .and_then(|m| m.get("zkvm-test").or_else(|| m.get("zkvm")));

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

    // 3. Automatic detection across workspace members
    let mut selected = Vec::new();
    for pkg in &metadata.packages {
        let is_member = metadata.workspace_members.iter().any(|id| {
            id == &pkg.id || id.starts_with(&format!("{}#", pkg.id)) || id.contains(&pkg.name)
        });
        if !is_member {
            continue;
        }

        if FRAMEWORK_CRATES.contains(&pkg.name.as_str()) {
            continue;
        }

        if ws_excludes.contains(&pkg.name) || cli_excludes.contains(&pkg.name) {
            continue;
        }

        // Check package-level metadata
        let pkg_zkvm_meta = pkg
            .metadata
            .as_ref()
            .and_then(|m| m.get("zkvm-test").or_else(|| m.get("zkvm")));

        let explicit_enable = pkg_zkvm_meta.and_then(|m| {
            m.get("enable")
                .or_else(|| m.get("enabled"))
                .or_else(|| m.get("test"))
                .and_then(|v| v.as_bool())
        });

        if let Some(false) = explicit_enable {
            continue;
        }

        let depends_on_zkvm_test = pkg.dependencies.iter().any(|d| d.name == "zkvm-test");

        if explicit_enable == Some(true) || depends_on_zkvm_test {
            selected.push(pkg.name.clone());
        }
    }

    selected.sort();
    selected.dedup();
    selected
}

fn print_help() {
    println!(
        r#"{BIN_NAME}: Nextest-grade test runner for zkVM binaries

USAGE:
    cargo zkvm-test [CARGO_OPTIONS] [--] [RUNNER_OPTIONS] [FILTER]

OPTIONS:
    -p, --package <SPEC>      Package to run tests for
    --workspace, --all        Test all zkVM packages in workspace (default when run from root)
    --exclude <SPEC>          Exclude packages from the test run
    --lib                     Test only this package's library unit tests
    --bin <NAME>              Test only the specified binary
    --test <NAME>             Test only the specified integration test
    --all-targets             Test all targets
    --release                 Build artifacts in release mode
    --target <TARGET>         Override target triple (default: {DEFAULT_SP1_TARGET})

RUNNER OPTIONS:
    --nocapture, --no-capture Do not capture guest stdout/stderr; print immediately
    --exact                   Filter tests by exact name match
    --list                    List all matching tests without running them
    --ignored                 Run only ignored tests
    --include-ignored         Run both regular and ignored tests
    --profile, --subcycles    Display hierarchical sub-routine cycle breakdown
    --instructions, --opcodes Display instruction category breakdown (ALU, Memory, Control, Math, Syscalls)
    --mode <MODE>             Proof mode: none (default), core, compressed, plonk, groth16
    -E, --expr <EXPR>         Nextest-style boolean filter expression (e.g. 'package(sha256) & test(header)')
    --junit-path <PATH>       Export test results to JUnit XML format for CI/CD
    --save-baseline [NAME]    Save execution metrics as a named baseline (default: "default")
    --compare-baseline <NAME> Compare execution metrics against a named baseline
    --fail-on-regression <N%> Fail if any test regresses by more than N percent
    --accept-snapshots        Accept/update golden snapshot files for tests with #[test(snapshot = "...")]
    --test-threads <N>        Number of test instances to execute concurrently
    --prover <PROVER>         Prover backend: mock (default), cpu, cuda
    -q, --quiet               Display one character per test instead of one line
    -h, --help                Print help information
"#
    );
}

fn partition_args(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut cargo_part = Vec::new();
    let mut runner_part = Vec::new();
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        let (key, inline_val) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };

        if RUNNER_BOOLEAN_FLAGS.contains(&key) {
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
        } else if key == "--profile" {
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
        } else if !key.starts_with('-') {
            // Positional filter
            runner_part.push(arg);
        } else {
            cargo_part.push(arg);
        }
    }
    (cargo_part, runner_part)
}

#[tokio::main]
async fn main() {
    let mut raw_args: Vec<String> = std::env::args().skip(1).collect();

    // If invoked as `cargo zkvm-test`, cargo passes "zkvm-test" as the first argument.
    if raw_args.first().is_some_and(|s| s == "zkvm-test") {
        raw_args.remove(0);
    }

    if raw_args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return;
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
    let mut i = 0;
    while i < cargo_filtered.len() {
        if cargo_filtered[i] == "--exclude" {
            if i + 1 < cargo_filtered.len() {
                cli_excludes.push(cargo_filtered[i + 1].clone());
            }
        } else if let Some(pkg) = cargo_filtered[i].strip_prefix("--exclude=") {
            cli_excludes.push(pkg.to_string());
        }
        i += 1;
    }

    let cargo_meta = query_cargo_metadata();

    let mut auto_packages = Vec::new();
    if !has_explicit_packages && !has_explicit_targets {
        if let Some(ref meta) = cargo_meta {
            auto_packages = resolve_default_packages(meta, &cli_excludes);
            if auto_packages.is_empty() {
                eprintln!("{BIN_NAME}: no zkVM test packages found in workspace.");
                eprintln!("hint: add `zkvm-test` to [dev-dependencies] or configure in Cargo.toml:");
                eprintln!("    [workspace.metadata.zkvm-test]");
                eprintln!("    packages = [\"my-guest-crate\"]");
                std::process::exit(1);
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
    } else if let Some(default_tc) = default_toolchain_for_target(&target) {
        cargo_cmd.arg(default_tc);
    }

    cargo_cmd.arg("test");
    cargo_cmd.arg("--no-run");
    if !cargo_filtered.iter().any(|a| a == "--target" || a.starts_with("--target=")) {
        cargo_cmd.arg("--target").arg(&target);
    }

    // Isolate zkVM build artifacts to target/zkvm by default to avoid cache
    // invalidation collisions with the host Rust toolchain.
    let has_explicit_target_dir = cargo_filtered.iter().any(|a| {
        a == "--target-dir" || a.starts_with("--target-dir=")
    }) || std::env::var_os("CARGO_TARGET_DIR").is_some();

    if !has_explicit_target_dir {
        let default_target_dir = cargo_meta
            .as_ref()
            .map(|m| format!("{}/target/zkvm", m.workspace_root))
            .unwrap_or_else(|| "target/zkvm".to_string());
        cargo_cmd.arg("--target-dir").arg(default_target_dir);
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
            std::process::exit(1);
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
            .map(|t| t.name)
            .unwrap_or_else(|| DEFAULT_TARGET_NAME.to_string());
        test_executables.push((target_name, path));
    }

    let status = match child.wait() {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{BIN_NAME}: error waiting for cargo: {err}");
            std::process::exit(1);
        },
    };

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    if test_executables.is_empty() {
        println!("{BIN_NAME}: no test binaries compiled for zkVM target");
        return;
    }

    // If --junit-path is provided, remove any pre-existing file from previous runs so this run starts fresh
    let mut i = 0;
    while i < runner_raw.len() {
        if runner_raw[i] == "--junit-path" {
            if let Some(path) = runner_raw.get(i + 1) {
                let _ = std::fs::remove_file(path);
            }
        } else if let Some(path) = runner_raw[i].strip_prefix("--junit-path=") {
            let _ = std::fs::remove_file(path);
        }
        i += 1;
    }

    let mut overall_exit_code = 0;
    for (target_name, elf_path) in test_executables {
        println!("\n     Running zkVM tests for `{target_name}` ({elf_path})");

        let mut args_list = vec![elf_path];
        args_list.extend(runner_raw.clone());

        let runner_args = zkvm_test_runner::runner::parse_args_from(args_list);
        let code = run(runner_args).await;
        if code != 0 {
            overall_exit_code = code;
        }
    }

    if overall_exit_code != 0 {
        std::process::exit(overall_exit_code);
    }
}
