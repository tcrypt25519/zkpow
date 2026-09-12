# Cycle Budgets

In zero-knowledge applications, instruction cycle count directly dictates proving time, hardware requirements, and on-chain verification fees. `zkvm-test` provides first-class support for declaring per-test cycle ceilings.

---

## Declaring a Cycle Budget

Add `budget = <MAX_CYCLES>` to the `#[test]` attribute:

```rust
use zkvm_test::test;

#[test(budget = 500_000)]
fn test_block_header_hash_within_budget() {
    let header_bytes = [0u8; 80];
    let hash = zkpow_sha256::double_sha256(&header_bytes);
    assert_ne!(hash, [0u8; 32]);
}
```

When the test runs, the runner verifies that the observed cycles do not exceed the declared limit:

```console
        PASS zkpow_sha256::tests::test_block_header_hash_within_budget
             [    249,380 cyc │    216,647 gas │   70.41 MB │   0.02s ] [budget: 500,000, 49.9%]
```

The output pill indicates:
- **`[budget: 500,000, 49.9%]`**: The declared budget and the percentage of the budget consumed by the execution.

---

## Budget Exceeded Failures

If optimization regressions or algorithmic changes cause the cycle count to exceed the declared budget, the test immediately fails:

```console
        FAIL zkpow_sha256::tests::test_block_header_hash_within_budget
             [    582,100 cyc │    490,200 gas │  165.20 MB │   0.04s ] [budget: 500,000, 116.4%]
             error: test exceeded cycle budget of 500,000 cycles (used 582,100 cycles, +16.4%)
```

This prevents insidious performance regressions from silently creeping into your guest code.

---

## Prover Gas Estimation

Along with raw instruction cycles, `cargo-zkvm` calculates **prover gas**:

```
[ 249,380 cyc │ 216,647 gas │ 70.41 MB │ 0.02s ]
```

- **Cycles**: The exact number of RISC-V instructions executed by the VM core.
- **Gas**: The weighted cost of operations according to the prover constraint system (accounting for precompile table costs like Keccak and SHA-256 vs basic arithmetic).
- **Trace Size**: The memory size required to represent the STARK proving execution trace.

---

## Best Practices for Cycle Budgets

1. **Set Realistic Headroom**: Set budgets approximately 15% to 25% above current baseline measurements to absorb minor compiler version fluctuations without triggering false alarms.
2. **Budget Critical Hotpaths**: Apply strict budgets to functions executed thousands of times per transaction or block (e.g. signature checks, Merkle verifications, hash compressors).
3. **Use Baselines for Macro Tracking**: For workspace-wide automated regression tracking across hundreds of tests, pair cycle budgets with [Baselines & Regression Gates](../performance/baselines.md).
