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
    reason:     String,
    target:     Option<ArtifactTarget>,
    executable: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ArtifactTarget {
    name: String,
    kind: Vec<String>,
}

fn print_help() {
    println!(
        r#"cargo-zkvm-test: Nextest-grade test runner for zkVM binaries

USAGE:
    cargo zkvm-test [CARGO_OPTIONS] [--] [RUNNER_OPTIONS] [FILTER]

OPTIONS:
    -p, --package <SPEC>      Package to run tests for
    --lib                     Test only this package's library unit tests
    --bin <NAME>              Test only the specified binary
    --test <NAME>             Test only the specified integration test
    --all-targets             Test all targets
    --release                 Build artifacts in release mode
    --target <TARGET>         Override target triple (default: riscv64im-succinct-zkvm-elf)

RUNNER OPTIONS:
    --nocapture, --no-capture Do not capture guest stdout/stderr; print immediately
    --exact                   Filter tests by exact name match
    --list                    List all matching tests without running them
    --ignored                 Run only ignored tests
    --include-ignored         Run both regular and ignored tests
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

    // If invoked as `cargo zkvm-test`, the first argument is "zkvm-test".
    if raw_args.first().map(|s| s == "zkvm-test").unwrap_or(false) {
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
            match arg.as_str() {
                // Runner flags
                "--nocapture" | "--no-capture" | "--exact" | "--list" | "--ignored" | "--include-ignored"
                | "-q" | "--quiet" => {
                    runner_part.push(arg);
                },
                "--test-threads" | "--prover" => {
                    runner_part.push(arg);
                    if let Some(val) = iter.next() {
                        runner_part.push(val);
                    }
                },
                s if s.starts_with("--test-threads=") || s.starts_with("--prover=") => {
                    runner_part.push(s.to_string());
                },
                // Cargo value flags
                "-p" | "--package" | "--bin" | "--test" | "--example" | "--bench" | "--manifest-path"
                | "--features" | "--target" | "--profile" => {
                    cargo_part.push(arg);
                    if let Some(val) = iter.next() {
                        cargo_part.push(val);
                    }
                },
                // Positional filter or unknown flag
                s if !s.starts_with('-') => {
                    runner_part.push(s.to_string());
                },
                _ => {
                    cargo_part.push(arg);
                },
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
                .find(|a| a.starts_with("--target="))
                .map(|a| a["--target=".len()..].to_string())
        })
        .unwrap_or_else(|| "riscv64im-succinct-zkvm-elf".to_string());

    let mut cargo_cmd = Command::new("cargo");
    if let Some(tc) = toolchain {
        cargo_cmd.arg(tc);
    } else if target.contains("succinct") {
        cargo_cmd.arg("+succinct");
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
            eprintln!("cargo-zkvm-test: failed to run cargo test: {err}");
            std::process::exit(1);
        },
    };

    let stdout = child.stdout.take().expect("failed to open stdout");
    let reader = std::io::BufReader::new(stdout);

    let mut test_executables = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if let Ok(artifact) = serde_json::from_str::<Artifact>(&line) {
            if artifact.reason == "compiler-artifact" {
                if let Some(path) = artifact.executable {
                    let target_name = artifact.target.map(|t| t.name).unwrap_or_else(|| "test".to_string());
                    test_executables.push((target_name, path));
                }
            }
        }
    }

    let status = match child.wait() {
        Ok(s) => s,
        Err(err) => {
            eprintln!("cargo-zkvm-test: error waiting for cargo: {err}");
            std::process::exit(1);
        },
    };

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    if test_executables.is_empty() {
        println!("cargo-zkvm-test: no test binaries compiled for zkVM target");
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
