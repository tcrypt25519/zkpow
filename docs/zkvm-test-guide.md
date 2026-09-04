# zkvm-test: User & Developer Guide

A practical, step-by-step guide for bringing Nextest-grade testing, cycle benchmarking, and performance regression tracking to your SP1 (and zkVM) applications.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Installation](#2-installation)
3. [Zero-to-Running in 5 Minutes](#3-zero-to-running-in-5-minutes)
4. [Writing zkVM Tests](#4-writing-zkvm-tests)
   - [Basic Tests & Assertions](#basic-tests--assertions)
   - [Testing Panics & Failure Paths](#testing-panics--failure-paths)
   - [Custom Guest Exit Codes](#custom-guest-exit-codes)
   - [Declarative Cycle Limits](#declarative-cycle-limits)
   - [Hierarchical Sub-Routine Cycle Profiling](#hierarchical-sub-routine-cycle-profiling)
   - [Guest stdout/stderr Output Capture](#guest-stdoutstderr-output-capture)
5. [CLI Reference & Options](#5-cli-reference--options)
6. [Baselines & CI Regression Gating](#6-baselines--ci-regression-gating)
   - [Saving a Baseline](#saving-a-baseline)
   - [Comparing Against a Baseline](#comparing-against-a-baseline)
   - [Automated CI Failure on Regressions](#automated-ci-failure-on-regressions)
   - [GitHub Actions Workflow Example](#github-actions-workflow-example)
7. [Prover Backends: Mock, CPU, and CUDA](#7-prover-backends-mock-cpu-and-cuda)
8. [Dual Compilation (Host + zkVM)](#8-dual-compilation-host--zkvm)
9. [Troubleshooting & FAQs](#9-troubleshooting--faqs)

---

## 1. Overview

`zkvm-test` replaces the slow, brittle, and silent standard zkVM test workflow with a modern test runner:

- **Isolated VM Executions**: Tests run in their own emulator instances. A panic in one test never kills the suite.
- **Multithreaded Concurrency**: Runs tests in parallel across your available CPU cores.
- **Cycle & Memory Metrics**: Displays exact VM cycle counts and STARK execution trace sizes per test.
- **Output Streaming**: Cleanly captures guest `println!` and `eprintln!` output without terminal line interleaving.
- **Nextest Ergonomics**: Supports test filtering, `--exact`, `--list`, `--ignored`, and `--nocapture`.
- **Regression Detection**: Catch performance regressions directly in CI with `--fail-on-regression <N%>`.

---

## 2. Installation

Install the `cargo-zkvm-test` binary plugin and runner onto your system:

```bash
cargo install zkvm-test-runner
```

*(Or when developing in this repository: `cargo install --path crates/zkvm-test-runner --bin cargo-zkvm-test`)*

Verify that the tool is installed and accessible to Cargo:

```bash
cargo zkvm-test --help
```

---

## 3. Zero-to-Running in 5 Minutes

Here is how to take an existing SP1 crate with zero in-VM tests and get full zkVM testing running immediately.

### Step 1: Add Dependencies to `Cargo.toml`

Add `zkvm-test` to `[dev-dependencies]` in your crate's `Cargo.toml`:

```toml
[package]
name = "my-zkvm-program"
version = "0.1.0"
edition = "2021"

[dependencies]
sp1-zkvm = "6.6.0"

[dev-dependencies]
zkvm-test = { version = "0.1.0" }
# Optional, for sub-routine cycle profiling:
sp1-cycle = { version = "0.1.0" }
```

### Step 2: Configure the Test Runner in your Crate Root

In your `src/lib.rs` (or `src/main.rs`), enable `#![feature(custom_test_frameworks)]` for the zkVM target and hook `zkvm_test::test_runner`:

```rust
// In src/lib.rs
#![cfg_attr(target_os = "zkvm", feature(custom_test_frameworks))]
#![cfg_attr(target_os = "zkvm", test_runner(zkvm_test::test_runner))]
#![cfg_attr(target_os = "zkvm", reexport_test_harness_main = "test_main")]

// If writing a binary crate (main.rs) rather than a library:
#[cfg(target_os = "zkvm")]
fn main() {
    #[cfg(test)]
    test_main();
}
```

### Step 3: Write Your First zkVM Test

In your test module (e.g. `src/tests.rs` or `tests/integration.rs`), use `zkvm_test::test`:

```rust
#[cfg(test)]
mod tests {
    use zkvm_test::test;

    #[test]
    fn test_addition_in_vm() {
        let a = 20;
        let b = 22;
        assert_eq!(a + b, 42);
    }
}
```

### Step 4: Run Your Tests

Run tests using the cargo plugin:

```bash
cargo zkvm-test -p my-zkvm-program
```

You will see the test suite compile for `riscv64im-succinct-zkvm-elf` and execute with live cycle counts and trace memory stats:

```text
     Running zkVM tests for `my_zkvm_program` (target/riscv64im-succinct-zkvm-elf/.../my_zkvm_program-xyz)

running 1 test (prover: mock, threads: 16)
test tests::test_addition_in_vm ... ok (4,210 cycles, trace: 1.20 MB)

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s (total cycles: 4,210, peak host rss: 210.4 MB)
```

---

## 4. Writing zkVM Tests

`zkvm-test` provides attributes that work seamlessly both inside the zkVM and on the host.

### Basic Tests & Assertions

Use `#[test]` (aliased from `zkvm_test::test`) or `#[zkvm_test]`:

```rust
use zkvm_test::test;

#[test]
fn test_hashing() {
    let digest = sha256::hash(b"hello world");
    assert_eq!(digest[0], 0xb9);
}
```

### Testing Panics & Failure Paths

In standard zkVM testing, a panic aborts the runner. With `zkvm-test`, you can test assertions and expected panics safely:

```rust
// Accepts any panic
#[test(should_panic)]
fn test_out_of_bounds_panics() {
    let empty: Vec<u8> = Vec::new();
    let _ = empty[0];
}

// Or with standard Rust attribute syntax:
#[test]
#[should_panic]
fn test_assertion_failure() {
    assert_eq!(1 + 1, 3);
}
```

### Custom Guest Exit Codes

If your guest calls `sp1_zkvm::io::exit(code)` or panics with a specific exit code:

```rust
#[test(exit_code = 42)]
fn test_custom_exit() {
    sp1_zkvm::io::exit(42);
}
```

### Declarative Cycle Limits

Prevent code bloat or algorithmic complexity bugs from creeping in by enforcing strict per-test cycle limits:

```rust
// Fails if this test consumes more than 250,000 instruction cycles:
#[test(max_cycles = 250_000)]
fn test_optimized_hashing_budget() {
    let data = [0u8; 1024];
    let _ = sha256::hash(&data);
}
```

If the test exceeds the budget, the runner fails with a clear explanation:

```text
test tests::test_optimized_hashing_budget ... FAILED
---- tests::test_optimized_hashing_budget ----
cycle budget exceeded: 312,450 cycles > budget of 250,000
```

### Hierarchical Sub-Routine Cycle Profiling

Wrap sections of your code with `sp1_cycle::run_track` to profile where cycles are spent:

```rust
#[test]
fn test_pipeline_breakdown() {
    sp1_cycle::run_track("outer_pipeline", || {
        let state = sp1_cycle::run_track("parse_input", || {
            parse_large_payload()
        });

        sp1_cycle::run_track("execute_crypto", || {
            verify_signatures(&state);
        });
    });
}
```

Run with `--profile` to see the hierarchical tree:

```bash
cargo zkvm-test -- --profile
```

Output:

```text
test tests::test_pipeline_breakdown ... ok (460,283 cycles, trace: 129.96 MB)
    ├── outer_pipeline: 386,802 cycles (84.0%) [1 call]
    ├── execute_crypto: 250,110 cycles (54.3%) [1 call]
    └── parse_input: 136,692 cycles (29.7%) [1 call]
```

### Guest stdout/stderr Output Capture

You can use standard `println!`, `eprintln!`, or `sp1_zkvm::io::write(1, ...)` / `write(2, ...)` inside tests:

```rust
#[test]
fn test_with_logging() {
    println!("Step 1: In-VM state initialized");
    eprintln!("Warning: non-critical notice");
}
```

- **Default**: Output is hidden for passing tests to keep logs concise.
- **On Failure**: If the test fails, stdout and stderr are printed under diagnostic headers.
- **Live Output**: Pass `--nocapture` (or `--no-capture`) to print all logs in real time:

```bash
cargo zkvm-test -p my-zkvm-program -- --nocapture
```

---

## 5. CLI Reference & Options

`cargo zkvm-test` accepts both Cargo compilation flags and runner flags:

```bash
cargo zkvm-test [CARGO_OPTIONS] [--] [RUNNER_OPTIONS] [FILTER]
```

*(Note: The `--` delimiter is optional. The runner automatically partitions cargo options from test options).*

| Option | Category | Description |
|---|---|---|
| `-p, --package <SPEC>` | Cargo | Package to compile and test |
| `--lib` | Cargo | Test only this package's library unit tests |
| `--bin <NAME>` | Cargo | Test only the specified binary |
| `--test <NAME>` | Cargo | Test only the specified integration test |
| `--release` | Cargo | Build test ELFs in release mode |
| `--target <TRIPLE>` | Cargo | Override target (default: `riscv64im-succinct-zkvm-elf`) |
| `[FILTER]` | Runner | Filter tests by name substring (e.g. `cargo zkvm-test sha256`) |
| `--exact` | Runner | Filter tests by exact name match |
| `--list` | Runner | List all matching tests without running them |
| `--ignored` | Runner | Run only tests marked with `#[ignore]` |
| `--include-ignored` | Runner | Run both regular and ignored tests |
| `--nocapture, --no-capture` | Runner | Stream stdout and stderr immediately without buffering |
| `--profile, --subcycles` | Runner | Display hierarchical sub-routine cycle breakdown tree |
| `--test-threads <N>` | Runner | Number of test instances to execute concurrently |
| `--prover <PROVER>` | Runner | Prover backend: `mock` (default), `cpu`, `cuda` |
| `--save-baseline [NAME]` | Metrics | Save run metrics as a named baseline (default: `default`) |
| `--compare-baseline <NAME>`| Metrics | Compare current run against a saved baseline |
| `--fail-on-regression <N%>` | Metrics | Exit with code 1 if any test's cycles increase by > N% |
| `-q, --quiet` | Runner | Minimal output format |
| `-h, --help` | General | Print help information |

---

## 6. Baselines & CI Regression Gating

One of `zkvm-test`'s most powerful features is detecting performance regressions in CI before PRs are merged.

### Saving a Baseline

On your `main` or stable branch, run tests and save a baseline:

```bash
cargo zkvm-test --save-baseline main
```

This writes `.zkvm-test-metrics/main.json` containing exact cycle counts, trace sizes, and sub-routine statistics for every test.

### Comparing Against a Baseline

On your feature branch, run tests against that baseline:

```bash
cargo zkvm-test --compare-baseline main
```

The runner displays a formatted diff table:

```text
Baseline comparison against 'main':
Test Name                                                          Baseline        Current         Change
----------------------------------------------------------------------------------------------------------
zkpow_core::tests::apply_headers_from_genesis                       184,201        184,201           0.0%
zkpow_sha256::tests::public_values_optimized_hash                   268,979        241,100          -10.4%
zkpow_sha256::tests::subroutine_cycle_profiling_demonstration       460,283        460,283           0.0%
----------------------------------------------------------------------------------------------------------
```

### Automated CI Failure on Regressions

To prevent any PR from increasing cycle counts by more than e.g. `3%`:

```bash
cargo zkvm-test --compare-baseline main --fail-on-regression 3%
```

If any test regresses beyond `3%`, the table highlights `[REGRESSION]` and exits with non-zero exit code `1`:

```text
Baseline comparison against 'main':
Test Name                                                          Baseline        Current         Change
----------------------------------------------------------------------------------------------------------
zkpow_core::tests::process_blocks_pipeline                       18,420,102     19,550,000          +6.1% [REGRESSION]
----------------------------------------------------------------------------------------------------------

error: performance regression detected exceeding configured threshold!
test result: FAILED. 6 passed; 0 failed; finished in 0.38s
```

### GitHub Actions Workflow Example

Here is a ready-to-use GitHub Actions workflow (`.github/workflows/zkvm-tests.yml`):

```yaml
name: zkVM Performance & Test Suite

on:
  pull_request:
  push:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        run: |
          rustup default nightly
          curl -L https://sp1.succinct.xyz | bash
          ~/.sp1/bin/sp1up

      - name: Install zkvm-test
        run: cargo install zkvm-test-runner

      # On main branch: save baseline
      - name: Run tests & save baseline (main)
        if: github.ref == 'refs/heads/main'
        run: cargo zkvm-test --all-targets --save-baseline main

      # On pull requests: compare against main baseline
      - name: Run tests & verify performance (PR)
        if: github.event_name == 'pull_request'
        run: |
          # Fetch main baseline if stored as an artifact or in cache
          cargo zkvm-test --all-targets --compare-baseline main --fail-on-regression 3%
```

---

## 7. Prover Backends: Mock, CPU, and CUDA

By default, `zkvm-test` runs with the **`mock`** prover backend. The mock prover simulates instruction execution, counts cycles, and captures output without generating expensive STARK proofs—completing hundreds of tests in seconds.

When you want to verify actual STARK proof generation:

### CPU Prover

Generates full STARK proofs using multi-core host CPU:

```bash
cargo zkvm-test -p my-zkvm-program -- --prover cpu
```

### CUDA Prover (NVIDIA GPUs)

For systems with NVIDIA GPUs and CUDA installed:

```bash
cargo zkvm-test -p my-zkvm-program -- --prover cuda
```

### Global Environment Variable

You can set the prover globally without passing CLI flags:

```bash
export ZKVM_PROVER=cpu
cargo zkvm-test
```

---

## 8. Dual Compilation (Host + zkVM)

Tests written with `use zkvm_test::test;` compile cleanly on **both**:

- **Target OS zkVM**: `cargo zkvm-test` runs them in the virtual machine.
- **Host OS**: Standard `cargo test` runs them as native Rust host unit tests.

This allows developers to run standard host unit tests during active feature development, and run `cargo zkvm-test` for cycle verification, memory profiling, and CI gating!

---

## 9. Troubleshooting & FAQs

### Q: `cargo: error: no such subcommand: 'zkvm-test'`

Make sure `cargo-zkvm-test` was installed to `~/.cargo/bin` and that `~/.cargo/bin` is in your system `$PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
which cargo-zkvm-test
```

### Q: `error[E0463]: can't find crate for 'core' with target 'riscv64im-succinct-zkvm-elf'`

The SP1 target toolchain is not installed. Run:

```bash
sp1up
```

Or install the target with your custom succinct toolchain.

### Q: Why do my cycles slightly differ between runs?

Cycle counts in `zkvm-test` represent exact deterministic instruction executions. If your cycle counts change, inspect whether your code:

- Uses non-deterministic data (e.g. hashing `HashMap` iteration orders; switch to `BTreeMap` or `IndexMap`).
- Uses random number generators without fixed seeds.
- Changes allocations based on variable memory addresses.
