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
   - [High-Resolution Fault Diagnostics](#high-resolution-fault-diagnostics)
   - [Custom Guest Exit Codes](#custom-guest-exit-codes)
   - [Declarative Cycle Limits](#declarative-cycle-limits)
   - [Hierarchical Sub-Routine Cycle Profiling](#hierarchical-sub-routine-cycle-profiling)
   - [Guest stdout/stderr Output Capture](#guest-stdoutstderr-output-capture)
   - [Host-Injected Test Fixtures (`fixture = "..."`)](#host-injected-test-fixtures-fixture--)
   - [Public Values & Output Snapshot Testing (`snapshot = "..."`)](#public-values--output-snapshot-testing-snapshot--)
5. [CLI Reference & Options](#5-cli-reference--options)
   - [Automatic Build Cache Isolation (`target/zkvm`)](#automatic-build-cache-isolation-targetzkvm)
   - [Workspace Defaults & Package Auto-Detection](#workspace-defaults--package-auto-detection)
6. [Baselines & CI Regression Gating](#6-baselines--ci-regression-gating)
   - [Saving a Baseline](#saving-a-baseline)
   - [Comparing Against a Baseline](#comparing-against-a-baseline)
   - [Automated CI Failure on Regressions](#automated-ci-failure-on-regressions)
   - [GitHub Actions Workflow Example](#github-actions-workflow-example)
7. [CI Integrations: JUnit XML & Filter Expressions](#7-ci-integrations-junit-xml--filter-expressions)
   - [JUnit XML Export (`--junit-path`)](#junit-xml-export---junit-path)
   - [Nextest-Style Filter Expressions (`-E`, `--expr`)](#nextest-style-filter-expressions--e---expr)
8. [Prover Backends & Proof Modes](#8-prover-backends--proof-modes)
   - [Prover Backends: Mock, CPU, and CUDA](#prover-backends-mock-cpu-and-cuda)
   - [Proof Mode Control (`--mode`)](#proof-mode-control---mode)
   - [The Prover Daemon & Network Prover Roadmap](#the-prover-daemon--network-prover-roadmap)
9. [Instruction Breakdown & Profiling](#9-instruction-breakdown--profiling)
   - [Instruction Categories Table](#instruction-categories-table)
   - [Precompile & Syscall Tracking](#precompile--syscall-tracking)
10. [Dual Compilation & Host Execution (`--host`)](#10-dual-compilation--host-execution---host)
11. [Troubleshooting & FAQs](#11-troubleshooting--faqs)

---

## 1. Overview

`zkvm-test` replaces the slow, brittle, and silent standard zkVM test workflow with a modern test runner:

- **Isolated VM Executions**: Tests run in their own emulator instances. A panic in one test never kills the suite.
- **Multithreaded Concurrency**: Runs tests in parallel across your available CPU cores.
- **Cycle & Prover Gas Metrics**: Displays exact VM cycle counts, prover gas estimations (`calculate_gas`), and STARK execution trace sizes per test.
- **High-Resolution Fault Diagnostics**: Inspects memory faults with target addresses and opcodes, illegal instruction alignment, unimplemented syscall IDs, and memory watchdog monitor kills.
- **Output Streaming**: Cleanly captures guest `println!` and `eprintln!` output without terminal line interleaving using bounded snapshots.
- **Nextest Ergonomics**: Supports test filtering, `--exact`, `--list`, `--ignored`, `--nocapture`, and Nextest-compatible boolean expressions (`-E`).
- **Dual Target Execution**: Seamlessly executes zkVM tests on the RISC-V target or native host tests via `--host` within the same workflow.
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

You will see the test suite compile for `riscv64im-succinct-zkvm-elf` and execute with live cycle counts, prover gas metrics, and trace memory stats:

```text
     Running zkVM tests for `my_zkvm_program` (target/riscv64im-succinct-zkvm-elf/.../my_zkvm_program-xyz)

running 1 test (mode: mock, threads: 16)
test tests::test_addition_in_vm ... ok (4,210 cycles, 95 gas, trace: 1.20 MB)

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s (total cycles: 4,210, total gas: 95, peak host rss: 210.4 MB)
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

// Or assert on expected panic text in guest stderr:
#[test(should_panic(expected = "index out of bounds"))]
fn test_specific_panic() {
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

### High-Resolution Fault Diagnostics

When a guest program crashes due to a virtual machine fault, standard zkVM test runners typically abort abruptly or print generic "executor failed" messages. `zkvm-test` intercepts and decodes low-level `ExecutionError` variants into actionable, pinpoint diagnostics:

| Fault Type | Diagnostic Output Example | Root Cause |
| :--- | :--- | :--- |
| **Illegal Memory Access** | `memory fault: invalid memory access for opcode LD at address 0x12345678` | Reading or writing unmapped or out-of-bounds guest memory. Identifies both the offending RISC-V opcode (`LD`, `SW`, etc.) and the exact target address. |
| **Unaligned Instruction Fetch** | `alignment fault: unaligned instruction fetch at address 0x80001002 (must be 4-byte aligned)` | PC jumped to an unaligned address in untrusted code. |
| **Unimplemented Syscall** | `syscall fault: unimplemented syscall 0x00000042 (66)` | Guest attempted an unknown or unconfigured runtime system call. |
| **Watchdog RSS Monitor Kill** | `memory limit exceeded: child executor killed by watchdog monitor (measured 24576 MB)` | Host runner memory monitor terminated the executor process after exceeding host RSS memory thresholds. |
| **OS / Cgroup OOM Killer** | `child executor killed externally (possible OS / cgroup OOM-killer)` | Linux cgroup or host OS killed the child worker due to host memory starvation. |
| **Cycle Limit Exceeded (Watchdog)** | `cycle limit exceeded: program exceeded configured limit of 1,000,000 cycles` | Execution exceeded the maximum instruction count threshold configured for the VM engine watchdog. |
| **Unconstrained Mode Violations** | `unconstrained mode error: program halted while still in unconstrained execution block` | Guest invoked invalid syscalls or exited while running in unconstrained hint execution. |

These diagnostics appear directly under the test failure report in both CLI output and exported JUnit XML reports.

### Custom Guest Exit Codes

If your guest calls `sp1_zkvm::io::exit(code)` or panics with a specific exit code:

```rust
#[test(exit_code = 42)]
fn test_custom_exit() {
    sp1_zkvm::io::exit(42);
}
```

### Declarative Cycle Limits

Prevent code bloat or algorithmic complexity regressions from creeping in by enforcing strict per-test cycle limits:

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

> [!NOTE]
> **Post-hoc Assertion vs. Engine Watchdog**:
> - **`#[test(max_cycles = N)]`** is an **exact post-hoc assertion** evaluated against the completed run's `report.total_instruction_count()`. Even when a guest finishes normally with exit code 0, if the total cycle count exceeds your declared threshold, the test is marked failed with the exact cycle delta.
> - **`ExecutionError::ExceededCycleLimit`** is SP1's **runtime runaway watchdog**. It halts VM execution mid-run if an unbounded loop runs away past the coarse engine limit. Declarative `max_cycles` operates at single-cycle resolution and does not require guessing coarse VM chunk boundaries.

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

> [!TIP]
> **How Cycle Tracking Works Under the Hood**:
> - `sp1_cycle::run_track("label", ...)` writes structured boundary markers directly to guest `stdout` (`fd 1`):
>   `cycle-tracker-report-start: <label>` and `cycle-tracker-report-end: <label>`.
> - **The JIT vs. Profiling Trade-off**: In upstream SP1, populating `report.cycle_tracker` is gated on `feature = "profiling"`. Crucially, when `feature = "profiling"` is enabled in `sp1-core-executor`, the build script (`detect_executor`) automatically disables the fast native x86_64 JIT executor on Linux (`not(feature = "profiling")`), falling back to the 5–10× slower portable interpreter across the entire test suite!
> - **Zero-Overhead Runner Parsing**: Because `zkvm-test` captures guest `stdout` directly via tokio watch channels, the runner can parse start/end boundary markers independently from captured stdout or instruction deltas, completely sidestepping `feature = "profiling"` and keeping the high-speed JIT active for all tests.

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

### Host-Injected Test Fixtures (`fixture = "..."`)

Baking large test vectors (such as Bitcoin blocks, transaction batches, or state proofs) into test binaries using `include_bytes!` inflates the test ELF size, slows down link times, and increases zkVM emulator loading overhead.

With `zkvm-test`, tests can declare external fixture files:

```rust
#[test(fixture = "tests/fixtures/blocks_100k.bin")]
fn test_verify_block_headers() {
    // Reads bytes streamed by the host runner (zkVM) or read from disk (host test)
    let fixture_str = zkvm_test::fixture_str();
    // Or for raw binary bytes:
    // let fixture_bytes = zkvm_test::fixture();

    for line in fixture_str.lines() {
        // Parse and assert in the zkVM without baking bytes into the ELF!
    }
}
```

- **In the zkVM (`cargo zkvm-test`)**: In Phase 1, the test binary announces its fixture requirements to the runner. In Phase 2, the host runner loads the file from the host filesystem and streams it into `SP1Stdin` before executing the guest.
- **On the Host (`cargo test`)**: The macro automatically binds the fixture path via thread-local context so `zkvm_test::fixture()` seamlessly loads the file from disk, giving you complete test execution parity between host and guest.

### Public Values & Output Snapshot Testing (`snapshot = "..."`)

zkVM applications frequently produce public values (such as root state commitments, header hashes, or cryptographic summaries) by calling `sp1_zkvm::io::commit_slice` or `zkvm_test::commit_slice`.

Similar to `insta` for standard Rust code, `zkvm-test` provides automated snapshot testing for committed zkVM outputs:

```rust
#[test(snapshot = "snapshots/genesis_header_hash.hex")]
fn test_genesis_header_hash_snapshot() {
    let hash = compute_genesis_hash();
    zkvm_test::commit_slice(&hash);
}
```

#### Snapshot Formats
- **Hexadecimal (`.hex`)**: Formats committed bytes as clean hexadecimal. Useful for cryptographic digests, block hashes, and serialized state keys.
- **Binary (`.bin`)**: Saves and compares raw binary bytes directly.

#### Generating & Updating Snapshots
When adding a new snapshot test or updating expected values, pass `--accept-snapshots` (or `--update-snapshots`):

```bash
# Accept/record snapshot values
cargo zkvm-test --accept-snapshots -p my-zkvm-program

# Or on host:
ZKVM_ACCEPT_SNAPSHOTS=1 cargo test -p my-zkvm-program
```

#### Snapshot Diff Formatting
If a committed output does not match the recorded golden snapshot, the runner fails with a clear, color-coded hex diff:

```text
test tests::test_genesis_header_hash_snapshot ... FAILED

failures:

---- tests::test_genesis_header_hash_snapshot ----
snapshot mismatch for '/path/to/snapshots/genesis_header_hash.hex':
  - expected (32 bytes):
    e2c63d39d066016fca487f769be448fe277c36f206e169462664f92a2f4f55f3
  + actual (32 bytes):
    f2c63d39d066016fca487f769be448fe277c36f206e169462664f92a2f4f55f3
To accept these changes, run with '--accept-snapshots'.
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
| `--host` | Cargo | Execute tests on native host target instead of zkVM |
| `-p, --package <SPEC>` | Cargo | Package to compile and test |
| `--workspace, --all` | Cargo | Test all zkVM packages in workspace (default when run from root) |
| `--exclude <SPEC>` | Cargo | Exclude packages from the test run |
| `--lib` | Cargo | Test only this package's library unit tests |
| `--bin <NAME>` | Cargo | Test only the specified binary |
| `--test <NAME>` | Cargo | Test only the specified integration test |
| `--release` | Cargo | Build test ELFs in release mode |
| `--target <TRIPLE>` | Cargo | Override target (default: `riscv64im-succinct-zkvm-elf`) |
| `--target-dir <DIR>` | Cargo | Directory for all generated artifacts (default: `target/zkvm`) |
| `--color <WHEN>` | Cargo | Coloring: `auto`, `always`, `never` |
| `[FILTER]` | Runner | Filter tests by name substring (e.g. `cargo zkvm-test sha256`) |
| `--exact` | Runner | Filter tests by exact name match |
| `--list` | Runner | List all matching tests without running them |
| `--ignored` | Runner | Run only tests marked with `#[ignore]` |
| `--include-ignored` | Runner | Run both regular and ignored tests |
| `--nocapture, --no-capture` | Runner | Stream stdout and stderr immediately without buffering |
| `--profile, --subcycles` | Runner | Display hierarchical sub-routine cycle breakdown tree and instruction breakdown |
| `--instructions, --opcodes` | Runner | Display granular instruction breakdown by category (ALU, Memory, Control, Math, Syscalls) |
| `--mode, --proof-mode <MODE>` | Runner | ZK proof mode to generate and verify: `none` (default), `core`, `compressed`, `plonk`, `groth16` |
| `-E, --expr <EXPR>` | Runner | Nextest-style boolean filter expression (e.g. `'package(sha256) & test(header)'`) |
| `--junit-path <PATH>` | Runner | Export test results to standard JUnit XML format |
| `--accept-snapshots` | Runner | Record/update golden snapshot files for tests with `#[test(snapshot = "...")]` |
| `--test-threads <N>` | Runner | Number of test instances to execute concurrently |
| `--prover <PROVER>` | Runner | Prover backend: `mock` (default), `cpu`, `cuda` |
| `--save-baseline [NAME]` | Metrics | Save run metrics (cycles, prover gas, trace sizes, subcycles) as a named baseline (default: `default`) |
| `--compare-baseline <NAME>`| Metrics | Compare current run against a saved baseline (auto-compares if `default.json` exists) |
| `--fail-on-regression <N%>` | Metrics | Exit with code 1 if any test's cycles increase by > N% |
| `-q, --quiet` | Runner | Minimal output format |
| `-h, --help` | General | Print help information |

### Automatic Build Cache Isolation (`target/zkvm`)

Running standard `cargo test` (host toolchain) and `cargo zkvm-test` (custom zkVM toolchain, e.g. `+succinct`) back-to-back can cause severe incremental cache thrashing if both write to the root `target/` directory.

To provide seamless, instant back-to-back testing out of the box:
- `cargo-zkvm-test` **automatically defaults the build output directory to `<workspace_root>/target/zkvm`**.
- The host toolchain's `target/` cache remains completely untouched.
- Custom target directories can still be specified at any time via `--target-dir <DIR>` or the standard `CARGO_TARGET_DIR` environment variable.

### Workspace Defaults & Package Auto-Detection

In multi-crate workspaces, running `cargo test` on bare-metal zkVM targets fails because host crates (CLI binaries, network daemons, database drivers) cannot compile for RISC-V zkVM targets.

`cargo-zkvm-test` automatically solves this:
- **Running from Repository Root**: Running `cargo zkvm-test` scans the workspace and runs tests for **all zkVM crates** (crates that have `zkvm-test` in their dependencies), skipping host crates automatically.
- **Running from a Crate Subdirectory**: `cd crates/sha256 && cargo zkvm-test` tests only `zkpow-sha256`.
- **Excluding Packages**: `cargo zkvm-test --exclude zkpow-core` excludes specific crates.
- **Explicit Workspace Config (`Cargo.toml`)**:
  ```toml
  [workspace.metadata.zkvm-test]
  packages = ["zkpow-core", "zkpow-sha256"]
  # or:
  exclude = ["zkpow-cli", "zkpow-host"]
  ```
- **Explicit Package Config (`Cargo.toml`)**:
  ```toml
  [package.metadata.zkvm-test]
  enable = false # Exclude from default runs
  ```

### High-Concurrency & Shared Memory (`/dev/shm`) in Docker/CI

When executing tests with prover gas estimation enabled (`calculate_gas(true)`), SP1's gas estimator allocates trace buffers and shared memory segments for child execution workers.

In multi-threaded test runs on Linux (e.g. defaulting to `--test-threads 16`), concurrent worker threads can collectively allocate up to **4.4 GiB** of shared memory.

> [!WARNING]
> **Docker Shared Memory Limits (`/dev/shm`)**:
> Standard Docker containers default to a restrictive 64 MiB `/dev/shm` size. Running concurrent zkVM tests inside Docker without configuring shared memory will result in mysterious `SIGBUS`, `bus error`, or truncated trace errors.
>
> **Best Practices for Containerized CI**:
> 1. **Allocate Sufficient Shared Memory**: When starting your Docker container, specify `--shm-size=8g` (or at least `--shm-size=4g`):
>    ```bash
>    docker run --shm-size=8g ...
>    ```
> 2. **Throttle Concurrency in Constrained Environments**: If running in memory-constrained runners or VMs without large `/dev/shm`, limit concurrent test threads:
>    ```bash
>    cargo zkvm-test --test-threads 2
>    # or set the environment variable:
>    export ZKVM_TEST_THREADS=2
>    ```

---

## 6. Baselines & CI Regression Gating

One of `zkvm-test`'s most powerful features is detecting performance regressions in CI before PRs are merged.

### Saving a Baseline

On your `main` or stable branch, run tests and save a baseline:

```bash
cargo zkvm-test --save-baseline main
```

This writes `.zkvm-test-metrics/main.json` containing exact cycle counts, prover gas estimations, trace sizes, and sub-routine statistics for every test.

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

## 7. CI Integrations: JUnit XML & Filter Expressions

### JUnit XML Export (`--junit-path`)

For seamless integration with CI/CD platforms (GitHub Actions, GitLab CI, CircleCI, Jenkins, Buildkite), `zkvm-test` exports standard JUnit XML test reports:

```bash
cargo zkvm-test --junit-path target/junit.xml
```

#### What's in the Report?
- Root `<testsuites>` element aggregating global counts (`tests`, `failures`, `errors`, `skipped`, `time`).
- One `<testsuite>` per compiled package / test target.
- Individual `<testcase>` elements with actual execution wall-clock time.
- Custom zkVM metrics attached to each testcase:
  ```xml
  <properties>
    <property name="cycles" value="81144"/>
    <property name="trace_bytes" value="24030768"/>
  </properties>
  ```
- Full failure diagnostics, panic backtraces, and exit code explanations inside `<failure message="...">`.
- Skipped / ignored tests denoted with `<skipped/>`.
- Guest `stdout` and `stderr` logs inside `<system-out>` and `<system-err>`.

#### Multi-Target Merging
When running across multiple workspace packages, `cargo zkvm-test` automatically merges the results from all test ELFs into the designated `--junit-path` file, providing a single consolidated report for your CI runner.

#### GitHub Actions Example:
```yaml
- name: Run zkVM Tests with JUnit Report
  run: cargo zkvm-test --junit-path target/junit.xml

- name: Publish Test Report
  uses: dorny/test-reporter@v1
  if: always()
  with:
    name: zkVM Test Results
    path: target/junit.xml
    reporter: java-junit
```

---

### Nextest Filter Expressions (`-E`, `--expr`)

`zkvm-test` embeds the official `nextest-filtering` engine directly, giving you complete, 100% semantic compatibility with `cargo-nextest`'s filterset domain language:

```bash
cargo zkvm-test -E 'package(zkpow-sha256) & test(genesis)'
```

#### Upfront Package & Binary Pre-Pass Pruning
Unlike naive test runners that compile every crate and spin up every ELF before filtering:
- **Package-Level Pruning**: `cargo-zkvm-test` evaluates the filter expression against the workspace dependency graph (`guppy`) before building. Packages that cannot possibly match the expression are pruned from `-p` arguments upfront, saving substantial compilation time.
- **Binary-Level Pruning**: When compiler artifacts are emitted, `matches_binary` checks whether any test in the ELF could match. If `Some(false)`, the ELF is completely skipped—avoiding VM initialization, decoding, and enumeration overhead.

#### Supported Predicates

| Predicate | Syntax | Description | Example |
|---|---|---|---|
| **Test Name** | `test(pattern)` | Match test name path | `test(genesis)` |
| **Package** | `package(pattern)` or `p(pattern)` | Match package or crate name | `package(zkpow-sha256)` |
| **Dependencies** | `deps(pattern)` | Match packages and their direct/transitive dependencies | `deps(zkpow-types)` |
| **Reverse Deps** | `rdeps(pattern)` | Match packages and everything depending on them | `rdeps(zkpow-types)` |
| **Target Kind** | `kind(pattern)` | Match build target kind (`lib`, `test`, `bin`, etc.) | `kind(test)` |
| **Binary ID** | `binary(pattern)` or `b(pattern)` | Match binary ID | `binary(zkpow_sha256)` |
| **Default** | `default()` | Match default test set | `default()` |
| **All** | `all()` | Match all tests | `all()` |
| **None** | `none()` | Match no tests | `none()` |

#### Pattern Matching Styles

- **Substring**: `test(header)` matches any test containing `"header"`.
- **Exact**: `test(=zkpow_sha256::tests::test_genesis_header_hash_snapshot)` matches only exact identifier matches.
- **Regex**: `test(/.*_snapshot$/)` evaluates against a Rust regular expression.
- **Glob / Wildcards**: `test(*fixture*)` or `test(test_?_hash)`.

#### Operators & Grouping

- **Logical NOT**: `!` or `not` (e.g. `!test(slow)`)
- **Logical AND**: `&` or `and` (e.g. `package(core) & test(work)`)
- **Logical OR**: `|` or `or` (e.g. `test(genesis) | test(fixture)`)
- **Set Difference**: `-` (e.g. `deps(zkpow-types) - package(zkpow-types)`)
- **Parentheses**: `( ... )` to group conditions (e.g. `package(sha256) & (test(hash) | test(snapshot))`)

---

## 8. Prover Backends & Proof Modes

### Prover Backends: Mock, CPU, and CUDA

By default, `zkvm-test` runs with the **`mock`** prover backend. The mock prover simulates instruction execution, counts cycles, and captures output without generating expensive STARK proofs—completing hundreds of tests in seconds.

When you want to verify actual STARK proof generation:

#### CPU Prover

Generates full STARK proofs using multi-core host CPU:

```bash
cargo zkvm-test -p my-zkvm-program --prover cpu
```

#### CUDA Prover (NVIDIA GPUs)

For systems with NVIDIA GPUs and CUDA installed:

```bash
cargo zkvm-test -p my-zkvm-program --prover cuda
```

#### Global Environment Variable

You can set the prover globally without passing CLI flags:

```bash
export ZKVM_PROVER=cpu
cargo zkvm-test
```

### Proof Mode Control (`--mode`)

By default (`--mode none`), `zkvm-test` simulates guest execution, validating correctness, tracking cycles, and asserting snapshots with zero proof generation overhead.

When testing cryptographic proof generation and on-chain / recursive verifiability, pass `--mode <MODE>` (or `--proof-mode <MODE>`):

| Mode | Flag | Description | Typical Use Case |
|---|---|---|---|
| **None** (Default) | `--mode none` | Fast simulation only (no STARK/SNARK generated). | Local unit testing, CI cycle tracking, and rapid iteration. |
| **Core** | `--mode core` | Generates and cryptographically verifies an SP1 Core STARK proof. | Verifying trace polynomial constraints and proving soundness. |
| **Compressed** | `--mode compressed` | Recursively compresses the proof into a single compact STARK proof. | Preparing proofs for recursive aggregation or light clients. |
| **Plonk** | `--mode plonk` | Recursively wraps the proof into a PLONK SNARK with EVM verifier compatibility. | Testing end-to-end EVM on-chain verification. |
| **Groth16** | `--mode groth16` | Generates a constant-size Groth16 SNARK (~260 bytes). | Production L1 on-chain verification with minimal gas costs. |

#### Automatic Key Setup Optimization

When running a test suite with an active proof mode (e.g. `--prover cpu --mode core`), the runner automatically compiles the program's proving key (`pk`) **once per ELF** upfront before scheduling tests:

```text
Setting up proving key for Core mode... Proving key ready in 0.57s
running 3 tests (prover: cpu, mode: Core, threads: 16)
test zkpow_sha256::tests::test_single_header_hash ... ok (12,410 cycles, trace: 4.2 MB, Core proof: 0.01s)
test zkpow_sha256::tests::test_batch_header_hashes ... ok (48,920 cycles, trace: 16.8 MB, Core proof: 0.02s)
```

> [!NOTE]
> **Prover & Mode Banner Semantics**:
> - **Mock Execution (`--prover mock`)**: Under the default mock prover, no cryptographic proofs are constructed. The runner formats the banner as:
>   `running 3 tests (mode: mock, threads: 16)`
>   The redundant `prover` field is omitted so test output is never misleading.
> - **Real Proving (`--prover cpu|cuda|network`)**: When an actual prover backend is selected with `--mode`, the banner reflects both:
>   `running 3 tests (prover: cpu, mode: Core, threads: 16)`.

The proving key is shared across all concurrent worker threads in an `Arc<SP1ProvingKey>`, avoiding redundant key generation overhead.

Furthermore, execution correctness and test assertions run **first**. If a test fails or panics, the runner immediately halts that test and surfaces the exact error diagnostics, skipping expensive proof generation for failed runs.

### The Prover Daemon & Network Prover Roadmap

When running large test suites with proof generation enabled (`--mode core`, `--mode compressed`, or `--mode groth16`), creating a fresh `ProverClient` for every test introduces an expensive 10–20 second cold-start overhead per test.

To eliminate this bottleneck, `zkvm-test` incorporates an architectural roadmap for a persistent background **Prover Daemon** (`zkvm-prover-daemon`):
- **SP1 Network Prover Alignment**: Rather than building a custom IPC protocol, the daemon implements the official SP1 `ProverNetwork` gRPC service (`sp1_sdk::network::prover::NetworkProver`).
- **Zero Client Overhead**: Tests using `ProverClient::builder().network().hosted().rpc_url("http://127.0.0.1:50051").build()` interact with the local daemon out of the box with zero bespoke client wrappers.
- **Hybrid Data Plane**: Control calls and status streaming use standard gRPC, while large payloads (guest ELFs, proof artifacts, execution trace buffers) pass via `/dev/shm` shared memory on Linux or memory-mapped files.
- **Seamless Local ↔ Cloud Continuum**: The exact same test suite can run against the local daemon during development or against the decentralized Succinct Prover Network in CI simply by updating the RPC endpoint.

*(For full architecture diagrams, data flows, and trace mining specifications, see [prover-daemon-trace-mining-and-diagnostics.md](prover-daemon-trace-mining-and-diagnostics.md)).*

---

## 9. Instruction Breakdown & Profiling

While total cycle counts indicate overall compute cost, optimizing zkVM performance requires understanding *where* those cycles are spent.

Passing `--instructions` (or `--opcodes`, or `--profile`) prints a granular RISC-V opcode category breakdown and top system calls for each test:

```bash
cargo zkvm-test -p zkpow-sha256 test_block_headers_fixture_verification --instructions
```

Output:
```text
test tests::test_block_headers_fixture_verification ... ok (88,866 cycles, trace: 33.15 MB)
    ├── Instructions:
    │   ├── Memory:       48,154 (54.2%)
    │   ├── ALU:          24,082 (27.1%)
    │   ├── Control:      16,499 (18.6%)
    │   ├── Math:            125 (0.1%)
    │   └── System:            6 (0.0%)
    └── Syscalls:
        ├── SHA_EXTEND: 150 calls
        └── SHA_COMPRESS: 150 calls
```

### Instruction Categories Table

The runner categorizes all 53 SP1 RISC-V opcodes into 5 distinct execution categories:

| Category | Typical Opcodes | Description | Optimization Advice |
|---|---|---|---|
| **Memory** | `LW`, `SW`, `LB`, `SB`, `LH`, `SH`, `LBU`, `LHU`, `LWU`, `LD`, `SD` | Stack and heap loads/stores. | High memory % usually points to unvectorized memory copies, deep stack frames, or missing register reuse. |
| **ALU** | `ADD`, `SUB`, `XOR`, `OR`, `AND`, `SLL`, `SRL`, `SRA`, `SLT`, `SLTU` (and immediate `*I` variants) | Basic bitwise, shifting, and arithmetic operations. | Fast, native 1-cycle operations in zkVM. |
| **Control** | `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU`, `JAL`, `JALR` | Conditional branching and function jumps. | Unroll tight loops to eliminate branch and counter overhead. |
| **Math** | `MUL`, `MULH`, `MULHU`, `MULHSU`, `DIV`, `DIVU`, `REM`, `REMU` (and 32-bit `*W` variants) | Integer multiplication, division, and remainder. | Division and remainder are heavy in zkVM; prefer bit shifts when dividing by powers of 2. |
| **System** | `ECALL`, `EBREAK`, `FENCE` | System trap and environment calls. | Enters runtime precompiles and I/O handlers. |

### Precompile & Syscall Tracking

The runner automatically inspects `report.syscall_counts` to report hardware-accelerated precompiles:
- **`SHA_COMPRESS` & `SHA_EXTEND`**: SP1 cryptographic SHA-256 precompile invocations.
- **`KECCAK_PERMUTE`**: Keccak-f1600 permutation precompile invocations.
- **`ED_ADD` / `ED_DECOMPRESS` / `SECP256K1_ADD`**: Elliptic curve precompiles.
- **`SYS_WRITE` & `SYS_HALT`**: Host I/O and program termination.

This guarantees that developers can immediately spot whether their cryptographic operations are hitting hardware-accelerated precompiles or falling back to unaccelerated software implementations.

---

## 10. Dual Compilation & Host Execution (`--host`)

Tests written with `use zkvm_test::test;` compile cleanly and can execute seamlessly across both execution pathways:

### 1. zkVM Target Execution (Default)

Running `cargo zkvm-test` targets the RISC-V zkVM architecture (`riscv64im-succinct-zkvm-elf`):
```bash
cargo zkvm-test
```
- Tests run inside isolated virtual machine instances.
- Reports live cycle counts, prover gas (`calculate_gas`), trace memory footprints, and instruction breakdowns.
- Enforces declarative `#[test(max_cycles = ...)]` limits and handles `#[test(snapshot = "...")]` golden files.

### 2. Native Host Execution (`--host`)

Passing `--host` directs `cargo-zkvm-test` to compile and run tests on the host platform (e.g. `x86_64-unknown-linux-gnu` or `aarch64-apple-darwin`):
```bash
cargo zkvm-test --host
```
- Compiles via the host toolchain using `cargo test --no-run` and executes native test binaries directly on bare metal CPU.
- Preserves full Nextest-style formatting, test filtering (`-E`), progress reporting, and JUnit XML output without emulator overhead.
- Test attributes like `#[test(fixture = "...")]` automatically load fixture vectors directly from disk, maintaining 100% test logic parity with the zkVM.

### 3. Mixed Workspace Auto-Detection

In multi-package workspaces containing both zkVM guest programs and host-only infrastructure (such as CLI binaries, network listeners, or RPC services):
- Running `cargo zkvm-test` from the workspace root automatically inspects crate dependencies.
- Packages depending on `zkvm-test` are routed to the zkVM compilation and execution pipeline.
- Packages without `zkvm-test` (or configured with host metadata) are automatically identified and executed on the host native toolchain.
- You can test an entire heterogeneous workspace in a single invocation:
  ```bash
  cargo zkvm-test --workspace
  ```

---

## 11. Troubleshooting & FAQs

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
