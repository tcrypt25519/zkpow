# Introduction

`zkvm-test` runs Rust `#[test]` functions inside a zkVM and reports what each one costs in instruction cycles.

Guest code is normally tested from the outside. You build a standalone program for each case, execute it from a host harness, and inspect an exit code or a committed value. That approach works, but it scales poorly. A single panic aborts the run, cases execute one at a time, and a failure tells you which program died rather than which assertion broke.

With `zkvm-test`, tests live in the crate they exercise and read like any other Rust test:

```rust,ignore
use zkvm_test::test;

#[test]
fn public_values_optimized_hash_matches_standard_sha256() {
    let input = [0xAB; 64];
    assert_eq!(optimized_hash(&input), reference_sha256(&input));
}
```

Run them with the cargo plugin:

```console
$ cargo zkvm-test -p zkpow-sha256
running 11 tests (prover: mock, threads: 16)
test zkpow_sha256::tests::guest_output_capture_demonstration ... ok (68,528 cycles, trace: 19.34 MB)
test zkpow_sha256::tests::test_genesis_header_hash_snapshot ... ok (81,339 cycles, trace: 22.97 MB)
test zkpow_sha256::tests::subroutine_cycle_profiling_demonstration ... ok (460,604 cycles, trace: 130.05 MB)

test result: ok. 11 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.45s (total cycles: 26,108,980, peak host rss: 547.3 MB)
```

## What the runner gives you

- Each test executes in its own VM instance, so a panic or a non-zero exit fails one test instead of the run.
- Tests execute in parallel across the cores you have.
- Results are reported in instruction cycles, which stay identical across machines, rather than in wall-clock time that measures your laptop.
- Test selection works the way `cargo test` and `cargo nextest` do, including substring filters, `--exact`, `--list`, `--ignored`, and `-E` filter expressions.
- The same test functions compile and run on the host under plain `cargo test`, so you can iterate natively and verify in the VM.

Beyond running tests, the runner records per-test metrics that you can save as a baseline and compare against later, which turns a cycle regression into a failed CI check instead of something you notice months afterward.

## Where it runs

The runner executes guest programs through SP1 and targets `riscv64im-succinct-zkvm-elf` by default. The cargo plugin also recognizes the RISC Zero toolchain and target triple, but execution against RISC Zero is not implemented yet.

`zkvm-test` is not published to crates.io yet. Until it is, install it from a source checkout.

## How this book is organized

Getting started takes you from an empty crate to a passing in-VM test. Writing tests covers the attributes that control how a test is treated, including expected panics, cycle budgets, fixtures, and snapshots. Running tests covers selection, output, and prover backends. Measuring performance covers cycle profiling and regression gates, and Continuous integration covers reporting results back to CI. Reference collects the flags, attributes, configuration keys, and wire format in one place.
