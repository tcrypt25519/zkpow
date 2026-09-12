# JUnit XML Reports

Integrating test results into Continuous Integration systems (such as GitHub Actions, GitLab CI, Buildkite, and Jenkins) requires machine-readable reports. `cargo-zkvm` exports test results directly into standard **JUnit XML format**.

---

## Exporting JUnit Reports (`--junit-path`)

Pass `--junit-path <PATH>` to output a JUnit XML report upon test completion:

```bash
cargo zkvm test --junit-path target/zkvm-junit.xml
```

When running multi-package workspace tests, `cargo-zkvm` automatically aggregates test suites from all packages into a single unified XML file.

---

## XML Report Schema

The generated report adheres to the standard JUnit schema, embedding duration, cycle metrics, trace memory, and failure diagnostics:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="zkvm-test" tests="12" failures="0" errors="0" skipped="1" time="0.450">
  <testsuite name="zkpow_sha256" tests="12" failures="0" errors="0" skipped="1" time="0.450">
    <testcase name="test_genesis_header_hash_snapshot" classname="zkpow_sha256" time="0.010" cycles="81340" record_bytes="24085440" status="passed"/>
    <testcase name="budgeted_hash_exceeding_limit" classname="zkpow_sha256" time="0.000" cycles="0" record_bytes="0" status="skipped">
      <skipped message="test ignored"/>
    </testcase>
  </testsuite>
</testsuites>
```

If a test fails, the `<failure>` element includes the exact guest panic message and emulator backtrace:

```xml
    <testcase name="test_invalid_header_nonce" classname="zkpow_core" time="0.050" cycles="5792142" status="failed">
      <failure message="assertion failed: proof_of_work_valid" type="panic">
thread 'main' panicked at crates/core/src/pow.rs:42:9:
assertion failed: proof_of_work_valid
      </failure>
    </testcase>
```

---

## Uploading Test Results in CI

CI workflows can parse and display these test results using standard action plugins:

```yaml
- name: Publish Test Results
  uses: EnricoMi/publish-unit-test-result-action@v2
  if: always()
  with:
    files: target/zkvm-junit.xml
```
