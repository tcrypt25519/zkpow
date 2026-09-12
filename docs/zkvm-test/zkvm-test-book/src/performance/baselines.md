# Baselines and Regression Gates

While per-test cycle budgets work well for critical hotpaths, manually tuning hundreds of individual cycle limits across a large codebase is impractical. `cargo-zkvm` provides automated **performance baselines** and **CI regression gates** to catch unintended slowdowns across your entire test suite.

---

## Saving a Baseline (`--save-baseline`)

Save the current cycle measurements of your test suite as a named baseline using `--save-baseline`:

```bash
# Save to the default baseline:
cargo zkvm test --save-baseline

# Save to a custom named baseline:
cargo zkvm test --save-baseline main-branch
```

Baseline files are serialized as JSON and stored in `.zkvm-test-metrics/`:
```
.zkvm-test-metrics/
├── default.json
└── main-branch.json
```

---

## Comparing Against a Baseline (`--compare-baseline`)

To compare your current branch or local working tree against a previously captured baseline:

```bash
cargo zkvm test --compare-baseline main-branch
```

The runner displays the cycle delta for every test:

```console
        PASS zkpow_sha256::tests::test_block_headers_fixture_verification
             [  5,918,068 cyc │     -32,410 (-0.5%) │   4,375,262 gas │   1.63 GB │   0.17s ]
```

Green deltas denote performance optimizations; red deltas highlight cycle regressions.

---

## Enforcing CI Regression Gates (`--fail-on-regression`)

To automatically fail a Pull Request if any test regresses in cycle count by more than a specified percentage, add `--fail-on-regression <PERCENT>`:

```bash
cargo zkvm test --compare-baseline main-branch --fail-on-regression 5
```

If any test experiences a regression greater than 5%, `cargo-zkvm` exits with code 1 and prints a regression summary:

```console
error: 1 test regressed beyond the 5% threshold:
  - zkpow_sha256::tests::test_genesis_header_hash_snapshot: 92,000 cyc (+13.1% vs baseline 81,340 cyc)
```

This provides a continuous, automated performance safety net in CI.
