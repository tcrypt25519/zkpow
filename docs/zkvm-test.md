# zkvm-test

A `cargo nextest`-like test runner for SP1 and risc0.

## Problem

Writing tests for the zkvms is shitty. Difficult to run, tests are design to
panic which kills the entire vm process, (on SP1 at least) there's no way to
filter tests, you can't run them in parallel, no support for #[ignore] or
[should_panic], you can't harvest any useful addition info from them like
memory or cycle usage, there's no good way to get data in and out so your test
inputs generally need to be baked into the tests and the outputs gneerally need
to be panic or don't panic.

## Solution

`cargo zkvm-test` , like the title says. A toolkit for writing ZKT's. (Tests)

## Breakdown

### 1. Orchestration: Target Runner

Relies on Cargo's built-in target runner configuration (e.g., setting
`runner = "zkvm-test-runner"` for the `riscv64im` target). The
`zkvm-test-runner` binary is invoked directly by Cargo with the path to the
compiled ELF. This keeps the ELF out of the host's memory until execution and
standardizes the build pipeline.

### 2. Test Discovery: Two-Phase Protocol

**Uses a two-phase execution model over standard I/O:**

- **Phase 1 (Discovery):** The host runner spins up a mocked `ProverClient`,
  sending a `MAGIC` byte sequence (`b"ZKVT"`) and `CMD_LIST` (0) via `SP1Stdin`.
  The guest interce e eeeepts this and response by serializing its test catalog
  (including lengths, names, and flags like `ignore` or `should_panic`) into a
  byte buffer and committing it back to the host.
  - **Phase 2 (Execution):** The runner parses this binary catalog. It filters
    the tests (handling `--exact`, `--ignored`, or substring matching) and then
    spins up individual VM executions for each test using `CMD_RUN` (1) followed
    by the specific test name.

### 3. Guest Registry: Custom Test Frameworks

The `#[zkvm_test]` macro maps standard attributes into a statically defined
`TestCase` struct specific to the target OS. By applying the `#[test_case]`
attribute, it hooks into Rust's `#![custom_test_frameworks]` feature, which
aggregates these structs into a slice (`&[&TestCase]`) and passes them to the
root `test_runner` function.

### 4. Concurrency and Error Handling

The runner fully manages concurrency using a Tokio semaphore and
`std::thread::available_parallelism`. Because each test runs in a completely
isolated VM instance, a panic in one guest test will never halt the execution of
other tests. Furthermore, it intercepts expected exit codes natively and
aggregates the exact cycle counts for both individual tests and the total suite
run.

## Improvements Round 1

That is all what exists today. In no particular order, here what should be next(ish):

- [x] (Considering) Optional `test` alias for `zkvm_test` macro so entire files can
  switch without churning the attribute macro. (Unfortuatnely ignore and
  should_panic change synax so anything using those has to suffer the new name
  and syntaax.
- [x] Adding things like test lists, bench, profiles, etc; everything useful for us
  from `cargo test` / `cargo nextest`.
- [x] Make it a packaged cargo plugin.
- Add risc0 support
- [x] Don't hardcode mock prover, enable choosing one via flag and/or env var.
- [x] The runner only captures standard pass/fail metrics, exit codes, and panics. No output text. We should extract any output and weave it into the log.
- [x] Use sp1-cycle crates for better cycle tracking.
- [x] ~~Use memuse crate for mem usage tracking.~~ Ended up using a better method to get memory
- [x] Store metrics somewhere accessible and enable regression detection

## Improvements Round 2

### nextest style filtering

```sh
cargo nextest run -E 'package(core) & test(parse)'
```

### JUnit XML Output

Shown by Github Actions and Gitlab fake version of Actions to show test results
in the UI.

### Cycle-aware flamegraph

### Control proof mode

Allow: `core`, `compressed`, `groth16`, `plonk`, or `none` (default)

The addition of "none" as the default simplies the arg into being always active, but by default it dosen't do anything. Which also means --prover can default to cpu and always be active; it just doens't need to do anything when the default proof mode is the one set. so "cargo zkvm-test --mode core" would make it execute the prover wihtout having to specify cpu (which will be the value 99% of the time.)

### Show cycle/memory deltas if previous run is stored, even if not asserting against regressions

We show this if we're using the flag to fail on regressions, but even if we don't have that flag let's show the deltas if we have them.

### Snapshot-like tests for public values

We need a way to snapshot public values, store them, and assert against them in
subsequent tests.
