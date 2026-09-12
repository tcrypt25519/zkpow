# Command-Line Options

This chapter provides an exhaustive reference for the `cargo zkvm test` command and the `cargo-zkvm-test` alias.

---

## Synopsis

```
cargo zkvm test [OPTIONS] [FILTER]...
cargo-zkvm-test [OPTIONS] [FILTER]...
```

---

## Arguments

| Argument | Description |
| :--- | :--- |
| `[FILTER]...` | Positional test name filter patterns. Tests whose path contains any of these substrings are executed. |

---

## Package Selection Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `-p`, `--package` | `SPEC` | Package to run tests for. Can be repeated. |
| `--workspace`, `--all` | Flag | Test all zkVM packages in the workspace (default when run from workspace root). |
| `--exclude` | `SPEC` | Exclude packages from the test run. Can be repeated. |

---

## Target Selection Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--lib` | Flag | Test only this package's library unit tests. |
| `--bin` | `NAME` | Test only the specified binary. |
| `--test` | `NAME` | Test only the specified integration test. |
| `--example` | `NAME` | Test only the specified example. |
| `--bench` | `NAME` | Test only the specified benchmark. |
| `--all-targets` | Flag | Test all targets (runs both host and zkVM test suites). |
| `--host` | Flag | Run native host tests instead of in-VM tests. |

---

## Build Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--release` | Flag | Build artifacts in release mode. |
| `--cargo-profile` | `NAME` | Specify Cargo compilation profile (e.g. `dev`, `test`, `release`, `bench`). |
| `--target` | `TARGET` | Override target triple (default: `riscv64im-succinct-zkvm-elf`). |
| `--target-dir` | `DIR` | Directory for generated artifacts (default: `target/zkvm`). |
| `--features` | `LIST` | Space or comma-separated list of features to activate. |
| `--all-features` | Flag | Activate all available features. |
| `--no-default-features` | Flag | Do not activate the `default` feature. |
| `--manifest-path` | `PATH` | Path to `Cargo.toml`. |
| `--config` | `KEY=VALUE` | Override a Cargo configuration value. |

---

## Test Selection and Filtering Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--exact` | Flag | Filter tests by exact name match instead of substring. |
| `-E`, `--expr` | `EXPR` | Nextest-style boolean filter expression (e.g. `package(sha256) & test(header)`). |
| `--list` | Flag | List all matching tests without running them. |
| `--ignored` | Flag | Run only ignored tests. |
| `--include-ignored` | Flag | Run both regular and ignored tests. |

---

## Execution and Output Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--nocapture`, `--no-capture` | Flag | Do not capture guest stdout/stderr; stream output immediately. |
| `-q`, `--quiet` | Flag | Display one character per test instead of multi-line metric blocks. |
| `--format` | `FORMAT` | Output format: `nextest` (default) or `cargo`. |
| `--color` | `WHEN` | Terminal coloring: `auto`, `always`, or `never`. |
| `--test-threads` | `N` | Number of test instances to execute concurrently (default: logical CPU count). |

---

## Performance and Profiling Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--profile`, `--subcycles` | Flag | Display hierarchical sub-routine cycle breakdown and function profile table. |
| `--profile-filter` | `PATTERN` | Filter profiled functions by substring pattern. |
| `--instructions`, `--opcodes` | Flag | Display instruction category breakdown (ALU, Memory, Control, Math, Syscalls). |
| `--save-baseline` | `[NAME]` | Save execution metrics as a named baseline (default: `"default"`). |
| `--compare-baseline` | `NAME` | Compare execution metrics against a previously saved baseline. |
| `--fail-on-regression` | `PERCENT` | Fail with exit code 1 if any test regresses by more than N percent. |

---

## Prover Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--prover` | `BACKEND` | Prover backend: `mock` (default), `cpu`, `cuda`. |
| `--mode`, `--proof-mode` | `MODE` | Proof verification mode: `none` (default), `core`, `compressed`, `plonk`, `groth16`. |

---

## Fixtures and Snapshots Options

| Flag | Value | Description |
| :--- | :--- | :--- |
| `--fixtures`, `--fixtures-dir` | `DIR` | Override the root directory to search for test fixtures. |
| `--accept-snapshots`, `--update-snapshots` | Flag | Accept and update golden snapshot files for tests declaring `#[test(snapshot = "...")]`. |
| `--junit-path` | `PATH` | Export test results to JUnit XML format for CI/CD integration. |

---

## Environment Variables

| Variable | Description |
| :--- | :--- |
| `CARGO_ENCODED_RUSTFLAGS` | Unit-separator (`\x1f`) encoded compiler flags. If set, `cargo-zkvm` inspects but does not overwrite these flags. |
| `CARGO_RUSTFLAGS` | Space-separated compiler flags. If set, inspected and preserved. |
| `CARGO_TARGET_DIR` | Target directory base path. Defaults to `target/zkvm` when unset. |
| `ZKVM_FIXTURES` | Default directory path for resolving test fixtures. |
| `RUST_LOG` | Configures logging verbosity for the supervisor and emulator. |

---

## Exit Codes

| Code | Meaning |
| :--- | :--- |
| `0` | All tests completed successfully, or `--list` completed. |
| `1` | One or more tests failed, panics did not match, cycle budget exceeded, or baseline regression detected. |
| `2` | Command-line argument parsing error, or incompatible wire protocol between runner and guest ELF. |
| `101` | Underlying compilation error emitted by Cargo/rustc. |
