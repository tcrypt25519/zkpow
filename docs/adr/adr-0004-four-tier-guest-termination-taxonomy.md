---
title: "ADR-0004: Four-tier guest termination taxonomy: validity proof, consensus rejection proof, runtime panic, and invalid hint halt"
status: "Accepted"
date: "2026-10-24"
authors: "tcrypt (maintainer)"
tags: ["architecture", "decision", "rust", "zkvm", "consensus", "error-handling"]
supersedes: ""
superseded_by: ""
---

# Status

Accepted

# Context

The `zkpow` guest binary executes inside the SP1 RISC-V zkVM. When executing and proving a batch of block headers, termination can occur under fundamentally different circumstances:

1. **All headers are valid**: Every header passes difficulty, PoW, linkage, and median-time-past checks.
2. **A header violates consensus**: A block header in the batch fails proof-of-work, has an outdated timestamp, or fails cold-start minimums.
3. **The prover supplied malformed hints**: An unconstrained hint (e.g. median timestamp or block work quotient) was mathematically invalid, corrupted, missing, or left over.
4. **An unrecoverable runtime failure occurred**: The program encountered an environment fault, memory allocation failure, buffer I/O failure, or a Rust bug (`unreachable!`, unexpected slice indexing).

In a traditional native program, all non-success outcomes collapse into either an error exit code or a process panic. In a zero-knowledge virtual machine, collapsing these outcomes is dangerous and loses critical cryptographic capabilities:

- **Prover unconstrained execution vs. circuit constraints**: Prover hints are not constrained; they must be verified inside the circuit. If an invalid hint triggers a regular panic, a dishonest or faulty prover can disguise bad witness generation as a guest program bug.
- **Proof generation semantics**: In SP1, a program that panics unwinds and aborts execution. **No zero-knowledge proof is generated** for a panicking program. In contrast, a program that calls `syscall_halt(0)` executes to completion, allowing the SP1 prover to generate a valid, verifiable STARK proof over the committed public values.
- **Validity proofs vs. invalidity (fraud) proofs**: A block whose double-SHA-256 hash does not meet the difficulty target will *never* be valid on that chain; its hash and target are immutable. If the guest simply panicked on invalid headers, the prover could never produce cryptographic evidence proving that a specific block was rejected by Bitcoin consensus.

We need an explicit, four-tier taxonomy that strictly separates these termination modes across both the zkVM execution layer and the committed public values contract.

# Decision

Establish a four-tier termination taxonomy across the SP1 syscall boundary and the committed `PublicValues`:

| Tier | Termination Category | SP1 Syscall Exit Code | ReturnCode (Public Values Byte 184) | Proof Generated? | Meaning |
|---|---|---|---|---|---|
| **Tier 1** | **Consensus Success (Validity)** | `0` via `syscall_halt(0)` | `ReturnCode::SUCCESS` (`0`) | **Yes** | All batch headers valid. Tip state advanced. Proof can be chained recursively. |
| **Tier 2** | **Consensus Rejection (Invalidity)** | `0` via `syscall_halt(0)` | `ReturnCode(Some(ErrorCode))` (`> 0`) | **Yes** | Header at `height + 1` is mathematically invalid. State committed up to last valid header. Cannot be chained recursively. |
| **Tier 3** | **Prover Fault (Invalid Hint)** | `3` via `halt_invalid_hint` | *None (no public values)* | **No** | Prover provided wrong/missing/extra witness data. Execution halts immediately without generating a proof. |
| **Tier 4** | **Runtime Fault (Program Bug / Panic)** | `1` via standard Rust `panic!` | *None (no public values)* | **No** | Program crashed (I/O, allocation, assertion failure). Execution aborts without generating a proof. |

### Tier 1: Consensus Success (Chain Validity Proof)
When all headers in a batch pass validation:
- The updated chain state is written to the output buffer.
- `PublicValues` serializes `State || verifier_key || ReturnCode::SUCCESS (0)`.
- The guest commits the 185 bytes via `commit_slice` and halts cleanly with `syscall_halt(0)`.
- The SP1 prover constructs a recursive STARK proof that extends the chain.

### Tier 2: Consensus Rejection (Header Invalidity / Fraud Proof)
When a header fails consensus rules (insufficient PoW, timestamp $\le$ MTP, insufficient cold headers):
- The failure is an immutable, inherent fact about the header data itself. No honest or dishonest prover can ever make this header pass at this height.
- The guest does **not** panic. It commits the last known valid state (accumulated prior to the failing header via `EpochManager::flush`) along with the specific non-zero `ErrorCode` (`POW_INSUFFICIENT = 2`, `TIMESTAMP_TOO_OLD = 3`, `INSUFFICIENT_COLD_HEADERS = 5`).
- The guest calls `commit_slice` and halts with `syscall_halt(0)`.
- The SP1 prover generates a completely valid cryptographic proof that attests: *"Starting from genesis (or the prior authenticated state), the chain was valid up to height $H$, and the candidate header at height $H+1$ was rejected due to consensus error $E$."*
- **Recursive Terminal Guard**: In `guest/src/main.rs`, the recursive proof verification hardcodes `ReturnCode::SUCCESS (0)` in the reconstructed previous public values digest. Consequently, a Tier 2 rejection proof can **never** be recursively extended by subsequent batches. It is a terminal leaf that stands as standalone cryptographic evidence.

### Tier 3: Invalid Prover Hint (`syscall_halt(3)`)
When a prover hint fails internal circuit verification (e.g. claimed median does not have exact rank in the window, or block work witness $(q+1)d$ fails the 512-bit precompile check):
- This is not a property of the Bitcoin blockchain; it indicates that the prover's witness generator supplied incorrect data, ran out of hints, or left superfluous hints in the stream.
- The guest invokes the SP1 convention syscall `sp1_lib::halt_invalid_hint` (exit code 3).
- **No proof is produced**. The SP1 prover aborts before proving begins.
- **Custom Hint Assertion Suite**: To maintain clean developer ergonomics while strictly enforcing exit code 3 across targets, `zkpow-core` provides `invalid_hint!`, `assert_hint!`, `assert_hint_eq!`, and `assert_hint_ne!`.
  - On `target_os = "zkvm"`, they call `sp1_lib::halt_invalid_hint` / `sp1_lib::__invalid_hint_fmt`.
  - On non-zkVM host targets, they print the diagnostic message and trigger a standard Rust panic with `halt: invalid hint (3)`, allowing host-side unit tests (`#[zkvm_test(should_panic, exit_code = 3)]`) to test identical failure paths.

### Tier 4: Runtime Panic (`exit(1)`)
When an unexpected environment or logic failure occurs (e.g. `read_vec_raw` returns a null pointer due to missing stdin buffers, slice casting slop, out-of-memory, or a broken internal invariant):
- Standard Rust `panic!` takes over, terminating with exit code 1.
- No public values are committed and no proof is generated.

# Rationale

### Why Generate Proofs for Rejections (Tier 2)?
Allowing the guest to cleanly commit consensus rejections provides lightweight, verifiable invalidity proofs (fraud proofs) for free. Because SP1 verifies execution trace constraints regardless of whether the business logic resolved to `Ok` or `Err`, committing an error code produces a proof that an untrusted node fed the client an invalid header. While validity proofs remain the primary mechanism for chain tracking, invalidity proofs allow light clients to prove to third parties or arbitration smart contracts that a disputed block header was invalid under canonical consensus rules, without requiring the verifier to replay block validation.

### Why Exit 3 is Isolated from Exit 1
SP1 specifically reserves exit code 3 for invalid hints. In an untrusted prover setting, a prover cannot be trusted to execute hints honestly. If hint verification failures collapsed into exit 1 (general panics), an operator debugging a prover failure would be unable to distinguish between a software bug in the guest code and bad data supplied by an external worker or RPC source. Furthermore, isolating exit 3 ensures that invalid hints can never be mistaken for program crashes during automated fuzzing and regression testing.

### Unified Ergonomics for Hint Assertions
Rust developers instinctively reach for `assert!` or `assert_eq!`. Using native assertions for prover hints would result in exit code 1 instead of exit code 3. The `assert_hint!` macros in `zkpow-core` provide identical semantics and formatting options while routing to the appropriate platform halt mechanism.

# Consequences

### Positive
- **POS-001: Cryptographic invalidity proofs**: Systems integrating `zkpow` can receive and verify proofs of consensus violations without verifying headers themselves.
- **POS-002: Terminal recursion safety**: Failed proofs cannot poison the canonical chain because the guest's recursive verification step hardcodes `ReturnCode::SUCCESS (0)`.
- **POS-003: Clean operational triage**: Exit code 3 immediately pinpoints prover witness generator bugs without confusing them with guest logic errors (exit code 1).
- **POS-004: Dual-target testability**: The `assert_hint!` macro suite enables unit tests to verify hint validation on the host using `should_panic(expected = "halt: invalid hint (3)")` while testing the exact same code in `cargo zkvm test` against `exit_code = 3`.

### Negative
- **NEG-001: Client handling complexity**: Proof verifiers must inspect `ReturnCode` in byte 184 of `PublicValues`. Simply verifying that the SP1 STARK proof is valid is insufficient to guarantee that the chain advanced; a valid STARK proof can attest to a rejection.
- **NEG-002: Stdin cast discrepancy**: In `crates/guest/src/main.rs`, an empty or truncated input stream results in a `PodCastError`, which currently triggers a Rust panic (exit code 1) rather than an invalid hint (exit code 3).

# Alternatives Considered

### Option 1: Panic on consensus rejection (Two-tier model: Success vs Panic)
- **Description**: Treat consensus failures (`PowInsufficient`, `TimestampTooOld`) as panics.
- **Rejection Reason**: Prevents the generation of invalidity proofs and loses all contextual diagnostic information (such as the exact failing height and rejection reason) once execution unwinds.

### Option 2: Emit exit code 3 for consensus rejections
- **Description**: Halt with SP1 exit code 3 when a header fails PoW or MTP.
- **Rejection Reason**: Violates the purpose of SP1 hint halts. Exit code 3 produces no proof; it aborts proving. Consensus rejection must be provable to external verifiers.

### Option 3: Use native Rust `assert!` for witness verification
- **Description**: Use standard `assert!` expressions to check prover-supplied medians and block work quotients.
- **Rejection Reason**: Causes the zkVM to halt with exit code 1 on bad prover hints, violating SP1's hint-validation protocol and making automated triage impossible.

# Implementation Notes

- **Guest Entrypoint**: [`crates/guest/src/main.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/guest/src/main.rs#L27-L31) (`commit_public_values`) and [`crates/guest/src/main.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/guest/src/main.rs#L106-L110) (`return_code` mapping).
- **Error Codes**: [`crates/core/src/public_values.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/public_values.rs#L60-L138) (`ErrorCode` and `ReturnCode`).
- **Hint Halting Macros**: [`crates/core/src/halt/mod.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/halt/mod.rs#L1-L121) (`invalid_hint!`, `assert_hint!`, `assert_hint_eq!`, `assert_hint_ne!`).
- **Platform Implementation**: [`crates/core/src/halt/zkvm.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/halt/zkvm.rs) (calls `sp1_lib::halt_invalid_hint`) and [`crates/core/src/halt/platform.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/halt/platform.rs) (panics with matching string).

# References

- `crates/core/src/halt/mod.rs`
- `crates/core/src/public_values.rs`
- `crates/guest/src/main.rs`
- `crates/core/src/tests/halt.rs`
- SP1 Syscall Documentation: `sp1_lib::halt_invalid_hint`
