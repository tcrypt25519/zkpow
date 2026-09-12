# Provers and Proof Modes

`cargo-zkvm` supports multiple prover execution backends and proof verification levels. This allows developers to choose the optimal balance between instant test feedback and cryptographic proof generation.

---

## Prover Backends (`--prover`)

Configure the execution backend using `--prover <BACKEND>`:

| Backend | Description | Speed | Hardware Required |
| :--- | :--- | :--- | :--- |
| **`mock`** (Default) | Fast RISC-V interpreter emulator | Ultra-fast (~0.01s - 0.1s) | Any CPU (macOS, Linux, Windows) |
| **`cpu`** | Full STARK witness generation and proving on CPU | Moderate (~5s - 60s) | Multi-core CPU (>= 32GB RAM recommended) |
| **`cuda`** | Hardware-accelerated GPU proving via CUDA | Fast (~1s - 10s) | NVIDIA GPU with CUDA support (Linux) |

### Mock Prover (Default)
The mock prover executes the compiled RISC-V ELF directly, collecting accurate cycle counts and verifying all assertions without generating STARK polynomial commitments or cryptographic proofs:

```bash
cargo zkvm test -p zkpow-sha256 --prover mock
```

### CPU Prover
To verify that constraints hold during witness generation and test end-to-end proof generation locally:

```bash
cargo zkvm test -p zkpow-sha256 --prover cpu
```

### CUDA Prover
On Linux machines equipped with an NVIDIA GPU, enable the `cuda` feature during installation and run:

```bash
cargo zkvm test -p zkpow-sha256 --prover cuda
```

---

## Proof Modes (`--mode` or `--proof-mode`)

When running with a real prover backend (`cpu` or `cuda`), control the proof verification pipeline using `--mode <MODE>`:

- **`none`** (Default): Executes the program and tracks cycle metrics without generating cryptographic proofs.
- **`core`**: Generates a standard STARK core proof for the guest execution.
- **`compressed`**: Compresses the core STARK proof into a smaller recursive proof.
- **`plonk`**: Wraps the recursive proof into a PlonK SNARK for Ethereum EVM verification.
- **`groth16`**: Wraps the proof into a constant-size Groth16 SNARK proof.

Example:
```bash
cargo zkvm test -p zkpow-sha256 --prover cpu --mode core
```

---

## Concurrency Control (`--test-threads`)

By default, `cargo-zkvm` executes tests concurrently using all logical CPU cores detected on the host. You can explicitly limit worker thread concurrency via `--test-threads <N>`:

```bash
# Run tests sequentially:
cargo zkvm test --test-threads 1

# Limit to 4 parallel workers:
cargo zkvm test --test-threads 4
```
