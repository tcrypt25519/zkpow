# Attribute Reference

This chapter details all configuration parameters supported by the `#[zkvm_test::test]` procedural macro attribute.

---

## The `#[test]` Attribute

The macro is imported via `use zkvm_test::test;`.

```rust
#[test]
fn test_default() {}

#[test(budget = 500_000, should_panic = "invalid")]
fn test_configured() {}
```

---

## Parameter Catalog

### 1. `budget = <INTEGER>`
- **Type**: `u64` (integer literal, supports underscore separators: `1_000_000`).
- **Description**: Sets a maximum upper limit on instruction cycles for the test. If the observed cycle count exceeds this value, the test fails.
- **Example**:
  ```rust
  #[test(budget = 750_000)]
  fn test_merkle_verification() { ... }
  ```

### 2. `fixture = "<PATH>"`
- **Type**: String literal.
- **Description**: Injects an external data file into the test execution over standard input. Read in the test via `zkvm_test::fixture()` or `zkvm_test::fixture_str()`.
- **Search Hierarchy**: Traverses `--fixtures`, `ZKVM_FIXTURES`, `<crate>/fixtures`, `<crate>/tests/fixtures`, and `<workspace>/fixtures`.
- **Example**:
  ```rust
  #[test(fixture = "vectors/tx_sample.bin")]
  fn test_tx_parsing() { ... }
  ```

### 3. `snapshot = "<PATH>"`
- **Type**: String literal (commonly `.hex` or `.snap`).
- **Description**: Compares public values committed via `zkvm_test::commit(slice)` against the contents of a golden file on disk.
- **Update Flag**: Update golden files in-place using `cargo zkvm test --accept-snapshots`.
- **Example**:
  ```rust
  #[test(snapshot = "genesis.hex")]
  fn test_genesis_output() {
      zkvm_test::commit(&hash);
  }
  ```

### 4. `should_panic`
- **Type**: Bare flag or String literal (`should_panic = "substring"`).
- **Description**: Asserts that the test function aborts with a panic. If a string literal is provided, asserts that the decoded panic payload contains the specified substring.
- **Examples**:
  ```rust
  #[test(should_panic)]
  fn test_any_panic() { panic!(); }

  #[test(should_panic = "out of bounds")]
  fn test_specific_panic() { panic!("index out of bounds: 5"); }
  ```

### 5. `exit_code = <INTEGER>`
- **Type**: `u32` (integer literal).
- **Description**: Asserts that the guest halts via runtime exit syscall with a specific non-zero exit code.
- **Example**:
  ```rust
  #[test(exit_code = 3)]
  fn test_fatal_hint_rejection() {
      zkpow_core::halt::fatal_protocol_abort(3);
  }
  ```

### 6. `ignore`
- **Type**: Bare flag.
- **Description**: Marks the test as skipped during standard test runs. Can be executed with `--ignored` or `--include-ignored`.
- **Example**:
  ```rust
  #[test(ignore)]
  fn test_heavy_benchmark() { ... }
  ```
