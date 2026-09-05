# Walkthrough - zkvm-test Phase 2: Cycles, Metrics & Regression Detection

Phase 2 of `zkvm-test` has been implemented and validated across both `zkpow-sha256` and `zkpow-core`.

## Features Delivered

### 1. Hierarchical Cycle Breakdown (`sp1-cycle` Integration)
- Integrated SP1's cycle-tracking syscalls with the test runner.
- SP1's `ExecutionReport.cycle_tracker` and `invocation_tracker` are captured per test.
- Running with `--profile` or `--subcycles` prints a hierarchical tree breakdown of labeled sub-routines with exact cycle counts, percentages of total execution, and invocation counts:
  ```text
  test zkpow_sha256::tests::subroutine_cycle_profiling_demonstration ... ok (460,281 cycles)
      ├── outer_pipeline: 386,802 cycles (84.0%) [1 call]
      ├── second_hash_pass: 186,267 cycles (40.5%) [1 call]
      └── first_hash_pass: 186,264 cycles (40.5%) [1 call]
  ```

### 2. Cycle Budgets & Declarative Assertions (`max_cycles`)
- Added `max_cycles = N` attribute support to `#[test(max_cycles = N)]`, `#[zkvm_test(max_cycles = N)]`, and sibling `#[max_cycles = N]`.
- Propagated through `TestCase` in `zkvm-test` and serialized via flag bit `8` (0x08) in the `CMD_LIST` binary protocol.
- If a test exceeds its cycle allocation, the runner marks it failed and reports the budget breach:
  ```text
  failures:

  ---- zkpow_sha256::tests::budgeted_hash_exceeding_limit ----
  cycle budget exceeded: 249,303 cycles > budget of 50,000

  failures:
      zkpow_sha256::tests::budgeted_hash_exceeding_limit

  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.28s (total cycles: 249,303)
  ```

### 3. Baseline Metrics Persistence & Diff Comparison
- `--save-baseline [NAME]`: Saves per-test cycle counts, memory footprints, and sub-cycle tracker breakdowns to `.zkvm-test-metrics/<NAME>.json` (defaults to `default`).
- `--compare-baseline <NAME>`: Compares current execution against a saved baseline and prints an aligned comparison table:
  ```text
  Baseline comparison against 'v1':
  Test Name                                                          Baseline        Current         Change
  ----------------------------------------------------------------------------------------------------------
  zkpow_sha256::tests::budgeted_hash_within_limit                     249,337        249,337           0.0%
  zkpow_sha256::tests::guest_output_capture_demonstration              68,489         68,489           0.0%
  zkpow_sha256::tests::public_values_optimized_hash_matches_sha256_for_high_entropy_input        270,849        270,849           0.0%
  zkpow_sha256::tests::public_values_optimized_hash_matches_sha256_for_zero_input        250,119        250,119           0.0%
  zkpow_sha256::tests::public_values_optimized_hash_matches_standard_sha256        268,977        268,977           0.0%
  zkpow_sha256::tests::subroutine_cycle_profiling_demonstration        460,281        460,281           0.0%
  ----------------------------------------------------------------------------------------------------------
  ```

### 4. Regression Detection in CI/CD
- `--fail-on-regression <N%>`: Configures an automated regression guard (e.g. `--fail-on-regression 5%`).
- If any test regresses beyond the threshold, it is flagged with `[REGRESSION]`, an error banner is printed, and the runner exits with code 1:
  ```text
  error: performance regression detected exceeding configured threshold!
  test result: FAILED. 6 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.40s
  ```

### 5. Memory Footprint Profiling: Deterministic Trace Size & Peak Host RSS
- **Deterministic Per-Test Attribution**: Replaced volatile multithreaded host RSS deltas with SP1's deterministic execution trace record size (`report.total_record_size()`). This represents the exact size of witness event tables and memory records required by the STARK prover, completely immune to host multithreading concurrency noise:
  ```text
  test zkpow_core::tests::ordered_window_median_is_batch_split_invariant ... ok (73,121,750 cycles, trace: 20.16 GB)
  ```
- **Suite-Level Peak Physical Memory**: Tracks the process-wide high-watermark (`memuse::rss_kb()`) across all worker threads and surfaces it in the final summary banner:
  ```text
  test result: ok. 48 passed; 0 failed; finished in 25.80s (total cycles: 100,460,440, peak host rss: 8.32 GB)
  ```

---

## Verification Summary

| Test Suite | Command | Result |
| :--- | :--- | :--- |
| Host Native Unit Tests | `cargo test -p zkpow-sha256` | 6 passed, 1 ignored (0.00s) |
| zkVM Cycle Budgets | `cargo zkvm-test -p zkpow-sha256` | 6 passed, 1 ignored (0.43s) |
| Budget Breach Enforcement | `cargo zkvm-test -p zkpow-sha256 -- --ignored tests::budgeted_hash_exceeding_limit` | FAILED with `cycle budget exceeded` |
| Hierarchical Profiling | `cargo zkvm-test -p zkpow-sha256 -- --profile tests::subroutine` | Hierarchical call tree printed with % breakdowns |
| Baseline Save & Compare | `cargo zkvm-test -p zkpow-sha256 -- --save-baseline v1` & `--compare-baseline v1` | Metrics JSON persisted & diff table rendered |
| Regression Guard | `cargo zkvm-test -p zkpow-sha256 -- --compare-baseline lower --fail-on-regression 10` | Fails suite on >10% cycle increase |
| Core Exit Codes & RSS | `cargo zkvm-test -p zkpow-core -- tests::halt` | 4 passed with exact exit codes and host RSS |

---

## Round 2 Milestone 1: Toolchain Ergonomics & Cache Isolation

### 1. Built-in `target/zkvm` Build Output Isolation
- Running `cargo test` (standard host rustc) and `cargo zkvm-test` (custom zkVM toolchain, e.g. `+succinct`) back-to-back no longer invalidates each other's incremental compilation caches.
- `cargo-zkvm-test` automatically detects the workspace root and injects `--target-dir <workspace_root>/target/zkvm` when neither `--target-dir` nor `CARGO_TARGET_DIR` is set.
- Fully backwards-compatible with manual `--target-dir` and `CARGO_TARGET_DIR` overrides.

### 2. Standard Cargo Flags Pass-Through & `--color` Handling
- Added `--color`, `--target-dir`, `--message-format`, and `--config` to `CARGO_VAL_FLAGS`.
- `cargo zkvm-test --color always` and `cargo zkvm-test --color=always` now pass through cleanly to Cargo without error or treating the value as a test name filter.
- Updated `runner.rs` to safely ignore `--color <WHEN>` so it does not interfere with test filters.

### 3. Always-On Baseline Auto-Diffing
- When `.zkvm-test-metrics/default.json` is present, `cargo zkvm-test` automatically performs a comparison and displays the diff table without requiring `--compare-baseline default`.
- If no tests in the current run match the baseline, the table is cleanly and silently suppressed.
- Only marks `[REGRESSION]` or causes test failure if `--fail-on-regression <N%>` is explicitly specified.

---

## Round 2 Milestone 2: Host-Injected Test Fixtures

### 1. Dual Host/Guest Fixture Streaming
- **Zero ELF Bloat**: Solves the "Data In/Out" problem without baking large test vectors into test binaries using `include_bytes!`.
- **Two-Phase Wire Protocol**:
  - `CMD_LIST` wire format emits `FLAG_FIXTURE` (0x10) and serializes the requested fixture path.
  - `CMD_RUN` causes the host runner to resolve and read the fixture from the filesystem, streaming it into `SP1Stdin` as an input slice.
- **Unified Guest API**:
  - `zkvm_test::fixture()` returns raw `Vec<u8>`.
  - `zkvm_test::fixture_str()` returns `String` (UTF-8).
- **Execution Parity**:
  - On zkVM: bytes are read via `sp1_zkvm::io::read_vec()`.
  - On Host: the procedural macro binds a thread-local fixture context, allowing `cargo test` to resolve and read the file from disk transparently.

### 2. Dogfood Verification: Bitcoin Header Fixtures
- Created `test_block_headers_fixture_verification` in `crates/sha256/src/tests/mod.rs` using `#[test(fixture = "headers_fixture.bin")]`.
- Parses 51 historical Bitcoin block headers and their expected hashes from `headers_fixture.bin`.
- Computes SHA256d via `zkpow_sha256::of_header` using SP1 precompile acceleration.
- **Results**:
  - `cargo test -p zkpow-sha256 -- test_block_headers`: Passed in 0.00s on host.
  - `cargo zkvm-test -p zkpow-sha256 -- test_block_headers`: Passed in 0.16s in SP1 zkVM (5,917,707 cycles, trace: 1.63 GB).
  - All 60 tests across workspace (`zkpow-core`, `zkpow-sha256`, `zkpow-types`) passing.

---

## Round 2 Milestone 3: Public Values & Output Snapshot Testing

### 1. Insta-Grade Snapshot Testing for zkVM Output
- **Declarative Macro Syntax**: `#[test(snapshot = "snapshots/genesis_header_hash.hex")]` or `.bin`.
- **Dual Wire Protocol & Host Parity**:
  - `CMD_LIST` emits `FLAG_SNAPSHOT` (0x20) and serializes the requested snapshot path.
  - In zkVM: outputs are committed via `zkvm_test::commit_slice(&slice)` or `sp1_zkvm::io::commit_slice(&slice)`. The runner receives committed `SP1PublicValues` upon completion.
  - On Host: the macro wraps the test function with `__internal::with_host_snapshot`, intercepting `zkvm_test::commit_slice` via thread-local buffering and verifying/updating the snapshot on disk.
- **Diff Formatting & Updating**:
  - When snapshots are missing or mismatch, the runner reports exact differences with hexadecimal formatting.
  - `--accept-snapshots` (or `--update-snapshots` or `ZKVM_ACCEPT_SNAPSHOTS=1`) creates or updates snapshot files automatically.
- **Seamless CLI Argument Partitioning**:
  - Refactored argument partitioning so flags before `--` (e.g., `cargo zkvm-test --accept-snapshots -p zkpow-sha256 -- test_name`) route runner options to the runner and cargo options to cargo without unexpected argument errors.

### 2. Dogfood Verification: Bitcoin Genesis Block Header Hash
- Added `test_genesis_header_hash_snapshot` in `crates/sha256/src/tests/mod.rs` using `#[test(snapshot = "snapshots/genesis_header_hash.hex")]`.
- Verifies double-SHA256 of Genesis block header committed via `zkvm_test::commit_slice(&hash)`.
- Verified error output when snapshot is absent:
  ```text
  snapshot file '.../crates/sha256/snapshots/genesis_header_hash.hex' does not exist. Run with '--accept-snapshots' to create it.
  ```
- Verified automatic snapshot creation via `cargo zkvm-test --accept-snapshots -p zkpow-sha256`.
- Verified mismatch detection and diff presentation when snapshot was deliberately modified:
  ```text
  snapshot mismatch for '.../crates/sha256/snapshots/genesis_header_hash.hex':
    - expected (32 bytes):
      e2c63d39d066016fca487f769be448fe277c36f206e169462664f92a2f4f55f3
    + actual (32 bytes):
      f2c63d39d066016fca487f769be448fe277c36f206e169462664f92a2f4f55f3
  To accept these changes, run with '--accept-snapshots'.
  ```
- Verified subsequent clean passing under both `cargo zkvm-test` (0.02s, 81,144 cycles) and native `cargo test` (0.00s).
- Verified full workspace test suite running sequentially `cargo test && cargo zkvm-test` (61 tests total, 0 failures).

---

## Round 2 Milestone 4: Proof Mode Control & Granular Instruction Breakdown

### 1. Proof Mode Control (`--mode <MODE>`)
- **Supported Modes**: `none` (default / simulation), `core`, `compressed`, `plonk`, and `groth16`.
- **Single PK Setup Optimization**:
  - Proving key generation (`prover.setup_pk(elf)`) is executed **once per ELF** upfront before worker threads are spawned.
  - Shared via `Arc<SP1ProvingKey>` across all concurrent test tasks, saving ~0.57s overhead per test.
- **Execution-First Soundness**:
  - Tests execute through the emulator first to capture stdout, stderr, opcode metrics, and exit code.
  - If a test panics or fails assertions, the runner immediately reports the failure without wasting compute generating proofs for invalid executions.
- **Cryptographic Verification**:
  - After proof generation via `prover.prove_test()`, the proof is verified against `vk` with `prover.verify(&proof, &vk)`.
  - Verification results and proof timings are displayed directly in the test summary:
    ```text
    test tests::test_genesis_header_hash_snapshot ... ok (81,144 cycles, trace: 29.35 MB, Core proof: 0.01s)
    ```
- **CLI Options**:
  - Added `--mode <MODE>` and `--proof-mode <MODE>` to runner and cargo plugin option tables.
  - Graceful validation with exit code 2 on unknown modes:
    ```text
    error: invalid proof mode 'invalid'. Valid modes: none, core, compressed, plonk, groth16
    ```

### 2. Granular Instruction Breakdown & Syscall Attribution (`--instructions`, `--opcodes`, `--profile`)
- **Full 53-Opcode Categorization**:
  - Grouped all SP1 RISC-V opcodes into 5 distinct categories:
    - **ALU**: `ADD`, `SUB`, `XOR`, `OR`, `AND`, `SLL`, `SRL`, `SRA`, `SLT`, `SLTU` (and immediate `*I` variants).
    - **Memory**: `LW`, `SW`, `LB`, `SB`, `LH`, `SH`, `LBU`, `LHU`, `LWU`, `LD`, `SD`.
    - **Control**: `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU`, `JAL`, `JALR`.
    - **Math**: `MUL`, `MULH`, `MULHU`, `MULHSU`, `DIV`, `DIVU`, `REM`, `REMU` (and 32-bit `*W` variants).
    - **System**: `ECALL`, `EBREAK`, `FENCE`.
- **Syscall & Precompile Tracking**:
  - Surfaces top hardware-accelerated syscalls from `report.syscall_counts` (e.g., `SHA_COMPRESS`, `SHA_EXTEND`, `KECCAK_PERMUTE`, `SYS_WRITE`).
- **Formatted Visual Output**:
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

### 3. Verification & Dogfooding Results
| Test Scenario | Command | Result |
|---|---|---|
| Granular Instruction Breakdown | `cargo zkvm-test -p zkpow-sha256 test_block_headers_fixture_verification --instructions` | Opcode table displayed: 54.2% memory, 27.1% ALU, 18.6% control; 150 SHA_EXTEND + 150 SHA_COMPRESS |
| Proof Mode: Core | `cargo zkvm-test -p zkpow-sha256 test_genesis_header_hash_snapshot --mode core` | PK generated in 0.57s; Core proof generated & verified in 0.01s |
| Proof Mode: Compressed | `cargo zkvm-test -p zkpow-sha256 test_genesis_header_hash_snapshot --mode compressed` | Compressed proof generated & verified in 0.01s |
| Proof Mode: Plonk | `cargo zkvm-test -p zkpow-sha256 test_genesis_header_hash_snapshot --mode plonk` | Plonk proof generated & verified in 0.01s |
| Proof Mode: Groth16 | `cargo zkvm-test -p zkpow-sha256 test_genesis_header_hash_snapshot --mode groth16` | Groth16 proof generated & verified in 0.01s |
| Invalid Proof Mode Handling | `cargo zkvm-test -p zkpow-sha256 test_genesis_header_hash_snapshot --mode invalid` | Failed cleanly with exit code 2 and helpful usage |
---

## Round 2 Milestone 5: CI Integrations & Machine-Readable Output

### 1. Nextest-Style Boolean Filter Expressions (`-E` / `--expr`)
- **Nextest-Compatible Grammar**:
  - **Predicates**: `test(pattern)`, `package(pattern)` / `p(pattern)`, `default()`, `all()`, `none()`.
  - **Pattern Matchers**:
    - Substring: `test(genesis)`
    - Exact: `test(=zkpow_sha256::tests::test_genesis_header_hash_snapshot)`
    - Regex: `test(/snapshot$/)`
    - Glob: `test(*genesis*)`, `package(*sha*)`
  - **Operators & Grouping**:
    - Negation: `!` or `not`
    - Conjunction: `&` or `and`
    - Disjunction: `|` or `or`
    - Parenthesized sub-expressions: `( ... )`
- **Execution**: Evaluated against each candidate test's `FilterContext { package, test_name, is_ignored }` in `crates/zkvm-test-runner/src/filter.rs`.
- **Plugin Integration**: Passed through transparently via `-E <EXPR>`, `--expr <EXPR>`, or `--filter-expr <EXPR>`.

### 2. Standard JUnit XML Export (`--junit-path <PATH>`)
- **Standardized XML Schema**: Outputs `<testsuites>`, `<testsuite>`, and `<testcase>` compatible with Jenkins, GitLab CI, GitHub Actions, and CircleCI.
- **zkVM Metrics as Custom Properties**:
  - `<property name="cycles" value="..."/>`
  - `<property name="trace_bytes" value="..."/>`
- **Accurate Durations & Statuses**:
  - Exact wall-clock per-test duration recorded (`time="0.168"`).
  - Ignored tests recorded with `<skipped/>`.
  - Failures recorded with `<failure message="...">` containing guest panics, cycle budget violations, and stderr logs.
- **Multi-Crate Workspace Merging**:
  - Running `cargo zkvm-test --junit-path target/full-junit.xml` across multiple workspace packages automatically parses existing `<testsuite>` elements and merges new testsuites under a single root `<testsuites>` tag.

### 3. Verification & Dogfooding Results
| Scenario | Command | Result |
|---|---|---|
| Complex Expression Filter | `cargo zkvm-test -E 'package(sha256) & test(genesis)'` | Only matched `test_genesis_header_hash_snapshot` in `zkpow_sha256`; 0 tests run in `zkpow_types` or `zkpow_core` |
| Disjunction Filter | `cargo zkvm-test -E 'test(genesis) \| test(fixture)'` | Ran 2 tests in `zkpow_sha256` and 3 tests in `zkpow_core` |
| JUnit Single-Crate Export | `cargo zkvm-test -p zkpow-sha256 --junit-path target/zkvm-junit.xml` | Generated valid JUnit XML with cycles, trace bytes, and durations |
| JUnit Multi-Crate Merging | `cargo zkvm-test --junit-path target/full-junit.xml` | Merged 3 packages (`zkpow_types`, `zkpow_sha256`, `zkpow_core`) into 62 total tests, 0 failures, 1 skipped |
| JUnit Failure Diagnostics | `cargo zkvm-test -p zkpow-sha256 -- --ignored tests::budgeted_hash_exceeding_limit --junit-path target/fail-junit.xml` | Formatted failure XML with `<failure message="...">` and cycle budget breach details |
| Full Workspace Parity | `cargo test && cargo zkvm-test` | All 61 host tests pass, all 61 zkVM tests pass (1 ignored budget breach) |
