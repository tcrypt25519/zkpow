# Reading the Output

`cargo-zkvm` provides a rich, terminal-formatted test report designed to give immediate visibility into test correctness, cycle consumption, prover gas costs, and memory footprints.

---

## Anatomy of a Test Result

Each test result is displayed using a two-line layout:

```console
        PASS zkpow_sha256::tests::test_genesis_header_hash_snapshot
             [     81,340 cyc │     82,726 gas │   22.97 MB │   0.01s ]
```

### Line 1: Status Badge & Test Path
- **`PASS`** (Green): Test executed to completion and all assertions held.
- **`FAIL`** (Red): Assertion failed, unexpected panic occurred, or cycle budget was exceeded.
- **`SKIP`** (Yellow): Test was marked `#[ignore]` and omitted from execution.
- **Test Path**: The fully qualified path of the test function (`crate::module::test_name`).

### Line 2: Metric Pill
Aligned directly beneath the test name with fixed indentation:
- **Cycles (`81,340 cyc`)**: The exact count of RISC-V instructions executed by the VM core.
- **Prover Gas (`82,726 gas`)**: The prover execution cost calculated from constraint counts and precompile invocations.
- **Trace Size (`22.97 MB`)**: The memory required to hold the execution trace for proof generation.
- **Duration (`0.01s`)**: Real wall-clock elapsed time on the host machine.

### Context Badges
When specific attributes are active, additional context badges appear at the end of the line:
- `[budget: 500,000, 49.9%]`: Declared cycle limit and percentage consumed.
- `[exit: 3]`: Expected guest exit code.
- `[snapshot updated]`: Golden snapshot file created or overwritten.

---

## The Summary Footer

At the completion of a test run, `cargo-zkvm` prints an aggregated summary footer:

```console
────────────
     Summary [  0.45s] 12 tests run: 12 passed, 0 failed, 1 ignored
             Total: 27,745,693 cycles │ 21,929,450 gas │ peak rss: 545.73 MB
test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.45s
```

- **Duration**: Total time spent across compilation and execution.
- **Passed / Failed / Ignored**: Test outcome totals.
- **Total Cycles**: Sum of all cycles executed across the test suite.
- **Total Gas**: Cumulative prover gas estimate.
- **Peak Host RSS**: Maximum physical memory consumed by the host supervisor and emulator workers.

---

## Output Formats (`--format`)

You can select alternative output formatting styles via `--format`:
- **`--format nextest`** (Default): Multi-line aligned metric pills, progress banners, and clean grouping.
- **`--format cargo`**: Emulates standard `cargo test` single-line output:
  ```console
  test zkpow_sha256::tests::test_genesis_header_hash_snapshot ... ok (81,340 cycles)
  ```

---

## Quiet Mode (`-q`, `--quiet`)

For minimal CI logging or fast local checks, pass `-q` or `--quiet`:

```bash
cargo zkvm test -q
```

In quiet mode, each test outputs a single character (`.` for pass, `F` for fail, `S` for skip), followed by the summary footer.
