# Introduction

`zkvm-test` brings standard Cargo testing workflows, cycle-accurate benchmarking, and trace profiling to code executing inside zero-knowledge virtual machines (zkVMs).

Testing zkVM guest code has traditionally been brittle, slow, and opaque. The common pattern requires developers to write bespoke guest programs for each scenario, wrap them in custom host runner scripts, execute them sequentially, and inspect only high-level exit codes or serialized public values. If an assertion fails, the entire program aborts, concealing which specific check broke; if performance degrades, developers lack function-level cycle attribution.

`zkvm-test` replaces this manual orchestration with a first-class test runner. Tests are written directly inside the crates they exercise using familiar Rust testing syntax:

```rust
use zkvm_test::test;

#[test]
fn test_sha256_public_values_round_trip() {
    let input = [0xAB; 64];
    let digest = compute_optimized_hash(&input);
    assert_eq!(digest, reference_sha256(&input));
}
```

Tests are executed with the `cargo-zkvm` plugin:

```console
$ cargo zkvm test -p zkpow-sha256
cargo-zkvm: testing 1 zkVM package: zkpow-sha256
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.08s

     Running zkVM tests for `zkpow_sha256` (target/zkvm/riscv64im-succinct-zkvm-elf/debug/deps/zkpow_sha256-51e892cc0b240f8e)
────────────
    Starting 12 tests across 1 binary (zkpow_sha256)
      Target config: (mode: mock, threads: 16)
        PASS zkpow_sha256::tests::guest_output_capture_demonstration
             [     68,529 cyc │     71,292 gas │   19.34 MB │   0.01s ]

        PASS zkpow_sha256::tests::test_genesis_header_hash_snapshot
             [     81,340 cyc │     82,726 gas │   22.97 MB │   0.01s ]

        PASS zkpow_sha256::tests::budgeted_hash_within_limit
             [    249,380 cyc │    216,647 gas │   70.41 MB │   0.02s ] [budget: 500,000, 49.9%]

        PASS zkpow_sha256::tests::public_values_bit_flip_sensitivity_test
             [ 16,063,837 cyc │ 12,829,690 gas │    4.43 GB │   0.45s ]

────────────
     Summary [  0.45s] 12 tests run: 12 passed, 0 failed, 1 ignored
             Total: 27,745,693 cycles │ 21,929,450 gas │ peak rss: 545.73 MB
test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.45s
```

## Key Capabilities

- **Isolated VM Executions**: Each test executes in an independent emulator instance. A panic, memory violation, or abnormal exit in one test fails only that test, never crashing the entire test suite.
- **Multithreaded Parallelism**: Dispatches test executions concurrently across host CPU cores, drastically cutting test turnaround time.
- **Cycle-Accurate Metrics**: Measures instruction cycles directly from the VM execution trace. Unlike wall-clock time, cycle counts are deterministic and identical across development laptops and CI servers.
- **Nextest Ergonomics**: Supports package selection (`-p`), workspace discovery, substring filters, `--exact`, `--list`, `--ignored`, and Nextest-compatible boolean filter expressions (`-E`).
- **Function-Level Trace Profiling**: Profiles cycle consumption down to individual guest functions with demangled symbols, invocation counts, math operations, and memory accesses (`--profile`).
- **Instruction Breakdown**: Categorizes executed opcodes into ALU, Memory, Control, Math, and Precompile/Syscall invocations (`--instructions`).
- **Regression Tracking**: Saves named performance baselines and enforces strict percentage regression gates in CI (`--fail-on-regression 5%`).
- **Dual-Target Execution**: Runs tests in the VM with `cargo zkvm test` or natively on the host with `cargo test` (or `cargo zkvm test --host`), utilizing mock backends for rapid iteration.

## Target Support

`zkvm-test` currently supports the **SP1** zkVM target (`riscv64im-succinct-zkvm-elf`). The toolchain integration and supervisor CLI automatically resolve the active `succinct` compiler, inject necessary low-level link and code generation flags, and isolate compilation caches inside `target/zkvm`.

The CLI also reserves support for the **RISC Zero** target (`riscv32im-risc0-zkvm-elf`), with runner backend execution planned on the roadmap.

## How to Read This Book

This book is modeled after the official Rust Book and Cargo Book:

1. **[Getting Started](getting-started/installation.md)**: Walks through installing the CLI tools, writing your first test, and understanding how the supervisor orchestrates compilation and execution.
2. **[Writing Tests](writing/test-attribute.md)**: Details the `#[zkvm_test::test]` attribute, failure assertions, cycle budgets, fixtures, snapshots, and guest output logging.
3. **[Running Tests](running/selecting-tests.md)**: Explains test selection, filtering expressions, output interpretation, dual host testing, and prover backends.
4. **[Measuring Performance](performance/profiling.md)**: Deep-dives into sub-routine cycle tracing, function profile tables, instruction categorization, and CI baseline regression gates.
5. **[Continuous Integration](ci/junit.md)**: Provides copy-pasteable GitHub Actions workflows and JUnit XML export configurations.
6. **[Reference](reference/cli.md)**: Complete, exhaustive documentation of all CLI options, macro attributes, configuration metadata keys, and wire protocols.
7. **[Troubleshooting](troubleshooting.md)**: Practical solutions for common compiler diagnostics, linker issues, and environment gotchas.
