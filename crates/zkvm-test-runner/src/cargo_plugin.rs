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
    "-q",
    "--quiet",
];

const RUNNER_VAL_FLAGS: &[&str] = &["--test-threads", "--prover", "--compare-baseline", "--fail-on-regression"];

const CARGO_VAL_FLAGS: &[&str] = &[
    "-p",
    "--package",
    "--bin",
    "--test",
    "--example",
    "--bench",
    "--manifest-path",
    "--features",
    "--target",
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

fn print_help() {
    println!(
        r#"{BIN_NAME}: Nextest-grade test runner for zkVM binaries

USAGE:
    cargo zkvm-test [CARGO_OPTIONS] [--] [RUNNER_OPTIONS] [FILTER]

OPTIONS:
    -p, --package <SPEC>      Package to run tests for
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
    --save-baseline [NAME]    Save execution metrics as a named baseline (default: "default")
    --compare-baseline <NAME> Compare execution metrics against a named baseline
    --fail-on-regression <N%> Fail if any test regresses by more than N percent
    --test-threads <N>        Number of test instances to execute concurrently
    --prover <PROVER>         Prover backend: mock (default), cpu, cuda
    -q, --quiet               Display one character per test instead of one line
    -h, --help                Print help information
"#
    );
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
        let cargo_part = raw_args[..sep_idx].to_vec();
        let runner_part = raw_args[sep_idx + 1..].to_vec();
        (cargo_part, runner_part)
    } else {
        // Automatically partition cargo build flags vs runner flags
        let mut cargo_part = Vec::new();
        let mut runner_part = Vec::new();
        let mut iter = raw_args.into_iter().peekable();

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
    };

    // Determine toolchain and target
    let toolchain = cargo_raw.iter().find(|a| a.starts_with('+')).cloned();
    let cargo_filtered: Vec<String> = cargo_raw.into_iter().filter(|a| !a.starts_with('+')).collect();

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
