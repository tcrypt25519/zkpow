---
title: "ADR-0003: Prover-hinted block work quotient verified by single 512-bit multiply-with-carry"
status: "Accepted"
date: "2026-10-24"
authors: "tcrypt (maintainer)"
tags: ["architecture", "decision", "rust", "consensus", "zkvm", "arithmetic"]
supersedes: ""
superseded_by: ""
---

# Status

Accepted

# Context

Bitcoin consensus defines cumulative chain work as the sum of the work proven by each block in the chain:

$$\text{chain\_work} = \sum \text{block\_work}$$

The work represented by an individual block is the expected number of double-SHA-256 hashes required to find a header hash at or below the block's target $T$. In Bitcoin Core (`GetBlockProof` in `chain.cpp`), this is calculated as:

$$\text{block\_work} = \left\lfloor \frac{2^{256}}{T + 1} \right\rfloor$$

In the zkVM, computing this value naively presents severe efficiency and soundness constraints:

1. **Constrained division cost**: A full 256-bit by 256-bit binary restoring or non-restoring long division requires 257 conditional shift-and-subtract iterations. Even when optimized by skipping leading zeros (running in $258 - n$ iterations, where $n = \text{bits}(T + 1)$), division in the zkVM costs thousands of cycles per evaluation.
2. **Epoch boundary frequency**: While block work is calculated only at epoch boundaries (or batch cold starts), any unconstrained witness supplied by the untrusted prover must be authenticated.
3. **Soundness against malicious provers**: A malicious prover could attempt to hint an inflated quotient $q' > q$ to artificially inflate the chain work and win the cumulative-work tiebreaker with less proof of work, or hint a deflated quotient $q' < q$ to craft invalid state transitions.

We must verify that a prover-supplied 256-bit witness $q$ equals $\lfloor 2^{256} / d \rfloor$ with minimal zkVM cycle overhead, using SP1's native cryptographic precompiles and syscalls.

# Decision

Supply the block work quotient $q$ as an unconstrained prover hint and verify it inside the zkVM using **one** invocation of the `syscall_uint256_mul_with_carry` precompile by evaluating:

$$\Phi = q \cdot d + d = (q + 1) \cdot d$$

where $d = T + 1$ (the divisor) and $q = \text{work}$.

Represented as a 512-bit unsigned integer $\Phi = (\text{hi} \cdot 2^{256}) + \text{lo}$, where $\text{hi}, \text{lo} \in [0, 2^{256}-1]$:
1. Call `syscall_uint256_mul_with_carry(&w_limbs, &d_limbs, &d_limbs)` to compute:
   $$\text{lo} + 2^{256} \cdot \text{hi} = (q \cdot d) + d$$
2. Reject the hint with `invalid_hint!` (SP1 exit code 3) unless all of the following hold:
   - $\text{hi}[1..4] = [0, 0, 0]$ (the product fits within 320 bits: $\text{hi} < 2^{64}$)
   - $\text{hi}[0] = 1$ (meaning $\text{hi} = 1$, exactly)
   - $\text{lo} \ne 0$
   - $\text{lo} \le d$

# Mathematical Proof of Correctness

Let $d, q \in \mathbb{Z}^+$ with $1 < d \le 2^{256}$. By Euclidean integer division, dividing $2^{256}$ by $d$ yields unique integers $Q$ (the quotient) and $R$ (the remainder) satisfying:

$$2^{256} = Q \cdot d + R \quad \text{where} \quad 0 \le R < d$$

Therefore, $q = \lfloor 2^{256} / d \rfloor$ if and only if $q = Q$.

### Lemma 1 (Exact Quotient Bounding Inequality)

$$q = \left\lfloor \frac{2^{256}}{d} \right\rfloor \iff 2^{256} < (q + 1) \cdot d \le 2^{256} + d$$

#### Forward Direction ($\implies$):
Assume $q = \lfloor 2^{256} / d \rfloor$. By definition of integer division:
$$2^{256} = q \cdot d + R \quad \text{with} \quad 0 \le R \le d - 1$$

Multiply $(q + 1)$ by $d$:
$$(q + 1) \cdot d = q \cdot d + d = (2^{256} - R) + d = 2^{256} + (d - R)$$

Since $0 \le R \le d - 1$:
1. Because $R \le d - 1 < d$, we have $d - R > 0$. Thus:
   $$(q + 1) \cdot d = 2^{256} + (d - R) > 2^{256}$$
2. Because $R \ge 0$, we have $d - R \le d$. Thus:
   $$(q + 1) \cdot d = 2^{256} + (d - R) \le 2^{256} + d$$

Combining (1) and (2):
$$2^{256} < (q + 1) \cdot d \le 2^{256} + d \quad \blacksquare$$

#### Backward Direction ($\impliedby$):
Assume $q$ satisfies $2^{256} < (q + 1) \cdot d \le 2^{256} + d$.
Subtract $d$ across the entire inequality:
$$2^{256} - d < q \cdot d \le 2^{256}$$

Now consider any integer $k \ne q$:
- If $k \le q - 1$:
  $$(k + 1) \cdot d \le q \cdot d \le 2^{256}$$
  This violates $(k + 1) \cdot d > 2^{256}$.
- If $k \ge q + 1$:
  $$(k + 1) \cdot d \ge (q + 2) \cdot d = q \cdot d + 2d > (2^{256} - d) + 2d = 2^{256} + d$$
  This violates $(k + 1) \cdot d \le 2^{256} + d$.

Because $Q = \lfloor 2^{256} / d \rfloor$ is known to exist and satisfies the inequality (by the forward direction), and no $k \ne Q$ can satisfy it, $q$ must equal $Q$. $\blacksquare$

---

### Theorem (Soundness of 512-bit Precompile Checks)

Let $(\text{lo}, \text{hi})$ be the output of `uint256_mul_with_carry(&q, &d, &d)`, such that:
$$(q + 1) \cdot d = q \cdot d + d = \text{hi} \cdot 2^{256} + \text{lo}$$
where $\text{lo}, \text{hi}$ are 256-bit unsigned integers represented as 4-limb arrays of `u64`.

The conditions checked in `verify_block_work_witness`:
1. $\text{hi}[1..4] = [0, 0, 0]$ and $\text{hi}[0] = 1 \iff \text{hi} = 1$
2. $\text{lo} \ne 0$
3. $\text{lo} \le d$

are necessary and sufficient to enforce $2^{256} < (q + 1) \cdot d \le 2^{256} + d$.

#### Proof:
Substitute $\text{hi} = 1$ into the value of $(q + 1) \cdot d$:
$$(q + 1) \cdot d = 1 \cdot 2^{256} + \text{lo} = 2^{256} + \text{lo}$$

1. **Lower Bound ($2^{256} < (q + 1) \cdot d$)**:
   $$(q + 1) \cdot d = 2^{256} + \text{lo} > 2^{256} \iff \text{lo} > 0 \iff \text{lo} \ne 0$$
   This is enforced by `lo[0] | lo[1] | lo[2] | lo[3] != 0`.

2. **Upper Bound ($(q + 1) \cdot d \le 2^{256} + d$)**:
   $$(q + 1) \cdot d = 2^{256} + \text{lo} \le 2^{256} + d \iff \text{lo} \le d$$
   This is enforced by `u256::from_limbs(lo) <= divisor`.

3. **High Limb Sanity ($\text{hi} = 1$)**:
   If $\text{hi} = 0$, then $(q + 1) \cdot d = \text{lo} \le 2^{256} - 1 < 2^{256}$, understating the product.
   If $\text{hi} \ge 2$, then $(q + 1) \cdot d \ge 2 \cdot 2^{256} + \text{lo} > 2^{256} + d$ (since $d \le 2^{256}$), overstating the product.
   Hence $\text{hi}$ must equal 1 exactly.

Therefore, the checks hold if and only if $q = \lfloor 2^{256} / (T + 1) \rfloor$. $\blacksquare$

# Consequences

### Positive
- **POS-001: Massive cycle reduction**: Reduces block work calculation from thousands of cycles (involving 200+ division iterations) to ~70 cycles for a single precompile execution and four register checks.
- **POS-002: Single-syscall verification**: Previous iterations required two separate `syscall_uint256_mul_with_carry` calls ($q \cdot d \le 2^{256}$ and $(q + 1) \cdot d > 2^{256}$). Computing $(q + 1) \cdot d = q \cdot d + d$ via the addend parameter $c$ folds the check into exactly one syscall.
- **POS-003: Deterministic host/guest parity**: The host fallback executes the identical `verify_block_work_witness` check over the quotient produced by `compute_block_work_division`, ensuring unit tests on host platforms continuously test the verification logic.
- **POS-004: Type safety with `NonMaxU256`**: `Target` encapsulates `NonMaxU256`, guaranteeing that $T < 2^{256} - 1$, so $d = T + 1$ is guaranteed never to overflow 256 bits.

### Negative
- **NEG-001: Precompile coupling**: Relies on the SP1 `syscall_uint256_mul_with_carry` syscall interface. Porting to another zkVM requires either a corresponding 512-bit multiply-accumulate precompile or emulated multi-limb arithmetic.
- **NEG-002: Subtle mathematical invariants**: The algebraic reduction ($q \cdot d + d$) and limb assertions ($\text{hi} = 1, \text{lo} \ne 0, \text{lo} \le d$) are non-trivial to audit at a glance without the formal proof.

# Alternatives Considered

### Option 1: In-circuit 256-bit binary long division
- **Description**: Directly execute restoring or non-restoring division within the zkVM circuit without prover hints.
- **Rejection Reason**: Extremely costly in zkVM cycles. Even when skipping leading zeros ($258 - n$ steps), binary division in RISC-V consumes thousands of ALU instructions and branch penalties per difficulty epoch.

### Option 2: Two-syscall verification ($q \cdot d \le 2^{256}$ and $(q + 1) \cdot d > 2^{256}$)
- **Description**: Verify the lower and upper bounds using two separate `syscall_uint256_mul_with_carry` calls, setting the addend $c = 0$.
- **Rejection Reason**: Requires doubling the number of syscalls, generating additional memory records and precompile invocations in the execution trace. Adding $d$ as the addend operand ($c = d$) computes $(q + 1) \cdot d$ in a single operation.

### Option 3: Supply remainder $r$ as a second prover hint
- **Description**: Have the prover supply both $(q, r)$ and verify $2^{256} = q \cdot d + r$ with $r < d$.
- **Rejection Reason**: Requires hinting and reading an extra 32-byte witness ($r$) into the guest, increasing witness stream parsing overhead without reducing constraint cost.

# Implementation Notes

- **Code Location**: [`crates/core/src/lib.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs#L349-L363) in `verify_block_work_witness` and [`crates/core/src/lib.rs`](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs#L408-L437) in `block_work_from_target`.
- **Precompile Signature**:
  ```rust
  syscall_uint256_mul_with_carry(
      a: *const u32, // w_limbs (q)
      b: *const u32, // d_limbs (d)
      c: *const u32, // d_limbs (+ d)
      lo: *mut u32,  // low 256 bits
      hi: *mut u32,  // high 256 bits
  )
  ```
- **Error Semantics**: Any violation of the inequality results in `crate::invalid_hint!("block work witness is not the exact quotient")` which terminates the guest with SP1 exit code 3 (invalid hint halt), rather than a consensus error or standard panic.

# References

- `crates/core/src/lib.rs` (`verify_block_work_witness`, `uint256_mul_with_carry`, `block_work_from_target`)
- `crates/core/src/tests/pow/failure.rs` (`block_work_witness_rejects_understated_work`, `block_work_witness_rejects_overstated_work`)
- `crates/core/src/tests/pow/success.rs` (`block_work_from_target_computes_exact_quotients`, `test_uint256_mul_with_carry_precompile`)
- Bitcoin Core: `src/chain.cpp:GetBlockProof`
- SP1 Syscalls: `sp1_lib::syscall_uint256_mul_with_carry`
