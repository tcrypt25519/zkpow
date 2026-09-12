# Host Testing and Dual Execution

While testing code inside the zkVM provides ground-truth cycle counts and verifies compatibility with the target architecture, compiling for RISC-V and running under an emulator is naturally slower than executing native machine code.

`zkvm-test` supports **dual-target execution**, enabling an optimal development workflow:
1. Iterate rapidly on your host machine with native speed (~0.05s).
2. Validate in-VM execution, cycle constraints, and trace profiling before committing (~0.4s).

---

## Running Host Tests via Standard Cargo

Because `#[zkvm_test::test]` expands to standard Rust `#[test]` attributes on non-zkVM targets, you can run all tests natively using standard `cargo test`:

```bash
cargo test -p zkpow-sha256
```

On the host:
- Code executes directly as native `aarch64` (Apple Silicon) or `x86_64` machine code.
- Cryptographic precompiles and VM syscalls (e.g. `sp1_zkvm::io::read`, `commit`) automatically use off-target mock implementations provided by `zkvm-test`.
- Compilation completes in milliseconds.

---

## Running Host Tests via `cargo zkvm test --host`

You can also run host tests directly through the `cargo-zkvm` runner using the `--host` flag:

```bash
cargo zkvm test -p zkpow-sha256 --host
```

When `--host` is passed:
- The supervisor compiles against your default host triple (without the Succinct RISC-V toolchain).
- Tests are executed natively on the host.
- The supervisor captures results, supports the same filter expressions (`-E`), and can export JUnit XML reports.

---

## Dual Execution: `--all-targets`

To execute tests both natively on the host and inside the zkVM in a single unified command, pass `--all-targets`:

```bash
cargo zkvm test -p zkpow-sha256 --all-targets
```

Execution proceeds in two clean phases:
1. **Phase 1 (Host)**: Compiles and executes native host unit tests, verifying business logic.
2. **Phase 2 (zkVM)**: Compiles against `riscv64im-succinct-zkvm-elf` and executes inside the zkVM emulator, verifying cycle limits and prover constraints.

If either phase fails, the run returns a non-zero exit code.
