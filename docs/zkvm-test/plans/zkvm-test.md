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

### Solution

`cargo zkvm-test` , like the title says. A toolkit for writing ZKT's. (Tests)

### Breakdown

#### 1. Orchestration: Target Runner

Relies on Cargo's built-in target runner configuration (e.g., setting
`runner = "zkvm-test-runner"` for the `riscv64im` target). The
`zkvm-test-runner` binary is invoked directly by Cargo with the path to the
compiled ELF. This keeps the ELF out of the host's memory until execution and
standardizes the build pipeline.

#### 2. Test Discovery: Two-Phase Protocol

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

#### 3. Guest Registry: Custom Test Frameworks

The `#[zkvm_test]` macro maps standard attributes into a statically defined
`TestCase` struct specific to the target OS. By applying the `#[test_case]`
attribute, it hooks into Rust's `#![custom_test_frameworks]` feature, which
aggregates these structs into a slice (`&[&TestCase]`) and passes them to the
root `test_runner` function.

#### 4. Concurrency and Error Handling

The runner fully manages concurrency using a Tokio semaphore and
`std::thread::available_parallelism`. Because each test runs in a completely
isolated VM instance, a panic in one guest test will never halt the execution of
other tests. Furthermore, it intercepts expected exit codes natively and
aggregates the exact cycle counts for both individual tests and the total suite
run.

That is all what exists today.

##### Improvements

In no particular order, here what should be next(ish):

- (Considering) Optional `test` alias for `zkvm_test` macro so entire files can
  switch without churning the attribute macro. (Unfortuatnely ignore and
  should_panic change synax so anything using those has to suffer the new name
  and syntaax.
- Adding things like test lists, bench, profiles, etc; everything useful for us
  from `cargo test` / `cargo nextest`.
- Make it a packaged cargo plugin.
- Add risc0 support
- Don't hardcode mock prover, enable choosing one via flag and/or env var.
- The runner only captures standard pass/fail metrics, exit codes, and panics.
  No output text. We should extract any output and weave it into the log.
- Use sp1-cycle crates for better cycle tracking.
- Use memuse crate for mem usage tracking.
- Store metrics somewhere accessible and enable regression detection
