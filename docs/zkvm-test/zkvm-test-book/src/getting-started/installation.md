# Installation

`zkvm-test` consists of two primary components:
1. **The CLI Supervisor & Runner (`cargo-zkvm`)**: A standalone Cargo plugin installed onto your development machine that handles toolchain orchestration, compilation caching, and parallel VM test execution.
2. **The Guest Harness Library (`zkvm-test`)**: A lightweight `#![no_std]` Rust crate added to your project's `[dev-dependencies]` that provides the `#[test]` attribute and execution catalog.

---

## Prerequisites

Before installing `cargo-zkvm`, ensure your system has the standard Rust development environment and the SP1 zkVM toolchain installed:

1. **Standard Rust Toolchain**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **SP1 Toolchain (`succinct`)**:
   Install `sp1up` and fetch the latest SP1 toolchain:
   ```bash
   curl -L https://sp1.succinct.xyz | bash
   sp1up
   ```
   Verify that the `succinct` toolchain is registered in `rustup`:
   ```bash
   rustup toolchain list | grep succinct
   ```

---

## Installing the CLI Supervisor

Install the `cargo-zkvm` binary directly via `cargo install`:

```bash
cargo install zkvm-test-runner
```

When building or developing directly from the source repository:

```bash
cargo install --path crates/zkvm-test-runner --bin cargo-zkvm
```

*(Alternatively, run `make install` from the repository root to compile and install `cargo-zkvm`, the `cargo-zkvm-test` alias, and `zkvm-test-runner` into `~/.cargo/bin`.)*

---

## Verifying the Installation

Verify that the CLI plugin is recognized by Cargo by running:

```console
$ cargo zkvm --help
cargo-zkvm: Nextest-grade developer suite and test runner for zkVM binaries

Usage: cargo zkvm <COMMAND>

Commands:
  test    Run zkVM tests with cycle metering, hierarchical trace profiling, and Nextest filtering
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Inspect the options for the `test` subcommand:

```bash
cargo zkvm test --help
```

---

## Backwards Compatibility

If your existing workflows or scripts reference `cargo-zkvm-test`, the compatibility binary is preserved:

```bash
cargo zkvm-test --help
```

Both `cargo zkvm test` and `cargo zkvm-test` invoke the identical supervisor engine and share the same execution pipeline.

---

## Toolchain Auto-Resolution

`cargo-zkvm` automatically locates the Succinct compiler (`rustc`) by querying:

```bash
rustup which --toolchain succinct rustc
```

When resolved, `cargo-zkvm` executes host `cargo` with `RUSTC=<path>`, completely bypassing rustup's proxy cargo wrapper. This eliminates the noisy `info: cargo is unavailable for the active toolchain` fallback warning and avoids any requirement to patch host Cargo configuration files.
