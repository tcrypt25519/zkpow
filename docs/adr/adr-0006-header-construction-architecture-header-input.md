---
title: "ADR-0006: Header-construction architecture: 44-byte HeaderInput witness instead of 80-byte block headers"
status: "Accepted"
date: "2026-10-27"
authors: "tcrypt (maintainer)"
tags: ["architecture", "decision", "rust", "consensus", "zkvm", "io", "cryptography"]
supersedes: ""
superseded_by: ""
---

# Status

Accepted

# Context

A standard Bitcoin block header is 80 bytes, structured as follows:

```text
offset  size  field           description
 0.. 4     4  version         Block version
 4..36    32  prev_blockhash  Double-SHA-256 hash of previous block header
36..68    32  merkle_root     Root hash of transactions merkle tree
68..72     4  timestamp       Block creation timestamp (Unix epoch seconds)
72..76     4  compact_target  Difficulty target in compact 32-bit `nBits` format
76..80     4  nonce           Mining nonce
```

In a naive zkVM Bitcoin light-client implementation, the prover supplies full 80-byte block headers in the witness stream. The guest circuit must then execute three distinct checks for every block:

1. **Explicit parent linkage check**:
   $$\text{assert\_eq}!(H_i.\text{prev\_blockhash}, \text{hash}(H_{i-1}))$$
2. **Explicit difficulty conformance check**:
   $$\text{assert\_eq}!(H_i.\text{compact\_target}, \text{state}.\text{active\_nbits})$$
3. **Proof-of-work threshold check**:
   $$\text{SHA-256d}(H_i) \le \text{expand\_target}(\text{state}.\text{active\_nbits})$$

In a batch of 2,016 headers (one full difficulty retarget epoch), passing full 80-byte headers imposes substantial costs:

- **Witness stream footprint**: $2{,}016 \times 80 = 161{,}280$ bytes of unconstrained witness buffer transfer per batch.
- **Circuit comparison overhead**: $2{,}016 \times 32 = 64{,}512$ bytes of explicit 256-bit equality comparisons for parent linkage, plus $2{,}016$ equality checks on `compact_target`.
- **Memory copies and allocations**: If full headers are materialized in guest memory, constructing and copying 80-byte arrays into SHA-256 message schedule buffers incurs hundreds of thousands of avoidable memory cycles across a batch.

Furthermore, allowing the prover to supply `prev_blockhash` and `compact_target` in the witness gives an untrusted prover unnecessary degrees of freedom that must be exhaustively checked against in-circuit state.

# Decision

Do not provide full 80-byte `Header` structs to the zkVM guest. Instead, define a stripped 44-byte [`HeaderInput`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/header.rs#L8-L13) containing solely the fields a miner was free to choose, omitting `prev_blockhash` and `compact_target`:

```rust
#[wiretype::wiretype]
pub struct HeaderInput {
    pub version:     u32,            //  0.. 4
    pub merkle_root: [u8; 32],       //  4..36
    pub timestamp:   BlockTimestamp, // 36..40
    pub nonce:       u32,            // 40..44
}
```

The guest reconstructs the double-SHA-256 preimage on the fly during hash execution using authenticated state:

1. **`prev_blockhash`** is supplied by the guest's running tip state:
   - For block 1 of a batch, it comes directly from [`State.block_hash`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/state.rs#L19).
   - For subsequent blocks within a batch, it is passed in registers as SHA state words (`[u64; 8]`) directly from the previous block's SHA-256d output.
2. **`compact_target`** is supplied by the active epoch context ([`EpochManager.active_nbits`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/state.rs#L263)).
3. **No explicit linkage or target equality checks**: The circuit performs **zero** explicit `prev_blockhash` or `compact_target` comparisons. The parent hash and active compact target are woven directly into the message schedule buffer when evaluating `zkpow_sha256::of_header_words`.

# Soundness & Security Analysis

Let $H = (V, P, M, T, C, N)$ be an 80-byte Bitcoin header where:
- $V = \text{version}$ (4 bytes)
- $P = \text{prev\_blockhash}$ (32 bytes)
- $M = \text{merkle\_root}$ (32 bytes)
- $T = \text{timestamp}$ (4 bytes)
- $C = \text{compact\_target}$ (4 bytes)
- $N = \text{nonce}$ (4 bytes)

A valid Bitcoin block header at height $i$ in active epoch $E$ extending authenticated tip hash $P^* = \text{hash}(H_{i-1})$ must satisfy:
1. $P = P^*$
2. $C = C_E$ (the canonical epoch target)
3. $\text{SHA-256d}(V \,\|\, P \,\|\, M \,\|\, T \,\|\, C \,\|\, N) \le \text{expand\_target}(C_E)$

### Implicit Linkage and Difficulty Soundness Proof

In `zkpow`, the prover provides witness $W = (V, M, T, N)$.
The guest computes the hash using $P^*$ and $C_E$:
$$H_{\text{computed}} = \text{SHA-256d}(V \,\|\, P^* \,\|\, M \,\|\, T \,\|\, C_E \,\|\, N)$$

and verifies:
$$H_{\text{computed}} \le \text{expand\_target}(C_E)$$

Now suppose a malicious prover attempts to submit a header that mined against a different parent hash $P' \ne P^*$ or mined at a different difficulty $C' \ne C_E$.

By collision resistance of SHA-256, if $(P', C') \ne (P^*, C_E)$:
$$\text{SHA-256d}(V \,\|\, P^* \,\|\, M \,\|\, T \,\|\, C_E \,\|\, N) \ne \text{SHA-256d}(V \,\|\, P' \,\|\, M \,\|\, T \,\|\, C' \,\|\, N)$$

The hash verified by the guest in the proof-of-work inequality is the hash with $P^*$ and $C_E$ embedded. If the miner found proof-of-work on $(V, P', M, T, C', N)$, that proof-of-work does not transfer to $(V, P^*, M, T, C_E, N)$. Because double-SHA-256 is a pseudo-random function, evaluating it with altered parent or target fields yields an essentially random 256-bit value, which has probability $\le \text{expand\_target}(C_E) / 2^{256}$ of satisfying the proof-of-work threshold (e.g. $< 2^{-70}$ on modern Bitcoin).

Therefore:
- **Parent linkage is guaranteed**: The candidate header cannot pass proof-of-work unless it was actually mined on top of the authenticated parent $P^*$.
- **Difficulty conformance is guaranteed**: The candidate header cannot pass proof-of-work unless it meets the target $C_E$ computed by the guest's epoch retarget rules.
- **The hash check is the linkage check**: Explicit equality assertions are mathematically redundant and can be omitted entirely.

# Consequences

### Positive

- **POS-001: 45% witness bandwidth reduction**:
  Decreases witness payload from 80 bytes to 44 bytes per header (saving 36 bytes/header). For a batch of 2,016 headers, witness I/O drops from 161,280 bytes to 88,704 bytes (a savings of 72,576 bytes per proof).
- **POS-002: Elimination of redundant comparisons**:
  Eliminates 2,016 256-bit equality comparisons ($H.\text{prev} == \text{state}.\text{tip}$) and 2,016 32-bit equality comparisons ($H.\text{nbits} == \text{expected}$) per batch.
- **POS-003: Direct schedule buffer insertion**:
  `HeaderInput` fields are written directly into the SHA-256 64-word schedule buffer alongside state words (`prev_blockhash` as `[u64; 8]`), eliminating the need to construct intermediate 80-byte memory buffers inside the zkVM.
- **POS-004: Tighter witness surface**:
  An untrusted prover has zero opportunity to supply malformed, off-chain, or conflicting parent hashes or difficulty targets.

### Negative

- **NEG-001: Specialized header representation**:
  The guest does not operate on standard 80-byte Bitcoin headers. Tooling, test fixtures, and RPC loaders must understand `HeaderInput` and strip the 36 bytes when preparing prover inputs.
- **NEG-002: Re-materialization required for off-chain export**:
  If an external caller or verifier requires the full 80-byte header, it must be reconstructed via [`HeaderInput::to_header(prev_blockhash, compact_target)`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/header.rs#L19-L28).

# Alternatives Considered

### Option 1: Standard 80-byte headers with explicit circuit assertions
- **Description**: Pass standard 80-byte headers into the guest. Assert `header.prev_blockhash == state.block_hash` and `header.compact_target == expected_target`, then hash the 80 bytes.
- **Rejection Reason**: Wastes 72 KB of witness I/O per batch and wastes cycles checking equality for values that can be statically woven into the hash preimage.

### Option 2: Pass full 80-byte headers but ignore the fields
- **Description**: Supply 80 bytes for compatibility, but have the guest slice out only `version`, `merkle_root`, `timestamp`, and `nonce`, ignoring the embedded parent hash and bits.
- **Rejection Reason**: Incurs all the witness I/O penalties of 80-byte headers without any architectural benefit.

# Implementation Notes

- **Input Definition**: [`crates/core/src/header.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/header.rs#L8-L13) defines `HeaderInput` with `#[wiretype::wiretype]`.
- **Database Witness Loader**: In `crates/store/src/db.rs` (`load_header_batch_witness`), the SQLite query queries:
  ```sql
  SELECT h.height, h.version, h.merkle_root, h.timestamp, h.nonce, h.median_time_past, h.ordered_timestamp_run
  FROM headers h ...
  ```
  extracting 44 bytes of header data directly without serializing `prev` or `n_bits`.
- **Circuit Preimage Construction**: In `crates/sha256/src/lib.rs` (`header_first_block_words` and `header_second_block`), `prev_blockhash` words and `compact_target` are loaded directly into the `[u64; 64]` schedule buffer alongside `HeaderInput` fields.
- **Host Mocking**: In `crates/core/src/lib.rs` (`hash_header_input`), the host-side fallback rematerializes full `Header` objects when running on native platforms or under test mocks.

# References

- `crates/core/src/header.rs` (`HeaderInput`, `HeaderInput::to_header`)
- `crates/types/src/header.rs` (`Header`)
- `crates/sha256/src/lib.rs` (`of_header_words`, `header_first_block_words`, `header_second_block`)
- `crates/core/src/state.rs` (`seal_header`)
- `crates/store/src/db.rs` (`load_header_batch_witness`)
- [ADR-0004: Four-tier guest termination taxonomy](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/docs/adr/adr-0004-four-tier-guest-termination-taxonomy.md)
