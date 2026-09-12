# Instruction Breakdown

Understanding the composition of instruction opcodes helps identify algorithmic bottlenecks, such as excessive memory pressure, redundant branching, or opportunities to accelerate computation with hardware precompiles.

---

## Instruction Categories (`--instructions`)

Pass `--instructions` (or `--opcodes`) to display a high-level breakdown of instruction types:

```bash
cargo zkvm test -p zkpow-sha256 --instructions
```

Output:

```console
        ├─ Instruction Categories:
        │  ├── Memory (load/store)          245,040 inst  ( 53.2%)
        │  ├── ALU (arithmetic/logic)       166,556 inst  ( 36.2%)
        │  ├── Control (branch/jump)         48,155 inst  ( 10.5%)
        │  ├── Math (mul/div/rem)               852 inst  (  0.2%)
        │  └── System (ecall/trap)               41 inst  (  0.0%)
```

### Category Definitions
- **Memory (load/store)**: `lw`, `sw`, `lb`, `lbu`, `lh`, `lhu`, `sb`, `sh`. High memory percentages (>50%) often indicate that register spilling, excessive stack frames, or unaligned memory access patterns are dominating execution.
- **ALU (arithmetic/logic)**: `add`, `sub`, `and`, `or`, `xor`, `sll`, `srl`, `sra`, and their immediate variants. Represents standard register-to-register computation.
- **Control (branch/jump)**: `beq`, `bne`, `blt`, `bge`, `jal`, `jalr`. High branch densities indicate tight loop overhead or unrolled match conditions.
- **Math (mul/div/rem)**: RISC-V M-extension instructions (`mul`, `mulh`, `div`, `rem`).
- **System (ecall/trap)**: Runtime environment calls and precompile dispatches.

---

## Precompile and Syscall Tracking

zkVMs use dedicated hardware precompiles to compute expensive cryptographic operations (such as SHA-256, Keccak-256, and elliptic curve pairings) outside the standard RISC-V CPU core.

When `--instructions` is active, `cargo-zkvm` logs the exact invocation counts of hardware syscalls:

```console
        └─ Syscalls & Precompiles:
           ├── SHA_EXTEND                         8 invocations
           └── SHA_COMPRESS                       8 invocations
```

This makes it immediately obvious whether your code is successfully dispatching to hardware-accelerated precompile tables or falling back to slower software implementations.
