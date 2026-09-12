# GitHub Actions

This chapter provides a production-ready GitHub Actions workflow for running zkVM tests, checking cycle regression gates against the base branch, and publishing test reports.

---

## Production CI Workflow Example

Create `.github/workflows/zkvm-tests.yml` in your repository:

```yaml
name: zkVM In-VM Tests & Regression Gates

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  zkvm-test:
    name: Run zkVM Tests
    runs-on: ubuntu-latest
    steps:
      - name: Checkout Code
        uses: actions/checkout@v4

      - name: Install Rust Toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install SP1 Toolchain
        run: |
          curl -L https://sp1.succinct.xyz | bash
          ~/.sp1/bin/sp1up
          echo "$HOME/.sp1/bin" >> $GITHUB_PATH

      - name: Install cargo-zkvm
        run: |
          cargo install zkvm-test-runner --locked

      - name: Cache zkVM Build Directory
        uses: actions/cache@v4
        with:
          path: target/zkvm
          key: ${{ runner.os }}-zkvm-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-zkvm-

      - name: Fetch Baseline Metrics (for PRs)
        if: github.event_name == 'pull_request'
        run: |
          git fetch origin ${{ github.base_ref }}:base-branch
          git checkout base-branch
          cargo zkvm test --save-baseline base
          git checkout ${{ github.sha }}

      - name: Run zkVM Tests (PR with Regression Gate)
        if: github.event_name == 'pull_request'
        run: |
          cargo zkvm test \
            --compare-baseline base \
            --fail-on-regression 5 \
            --junit-path target/zkvm-junit.xml

      - name: Run zkVM Tests (Push to Main)
        if: github.event_name == 'push'
        run: |
          cargo zkvm test \
            --save-baseline default \
            --junit-path target/zkvm-junit.xml

      - name: Publish Test Results
        uses: EnricoMi/publish-unit-test-result-action@v2
        if: always()
        with:
          files: target/zkvm-junit.xml
```

---

## Performance Optimizations in CI

1. **Cache `target/zkvm`**: Reusing intermediate compiled artifacts in `target/zkvm` between runs dramatically shortens CI build times.
2. **Dedicated Fast Runners**: While the mock prover executes quickly on standard 2-core GitHub runners, running multi-million cycle suites benefits from 8-core or 16-core compute runners (`--test-threads 16`).
3. **Dual Execution in Parallel**: Separate fast host tests (`cargo test`) and deep in-VM tests (`cargo zkvm test`) into separate workflow matrix jobs so developers receive fast assertion feedback within 30 seconds.
