# The Test Attribute

The `zkvm_test::test` procedural macro is the primary entry point for writing tests that run inside a zkVM. It behaves like the standard Rust `#[test]` attribute while supporting zkVM-specific parameters for cycle budgeting, fixtures, snapshot testing, and failure expectations.

---

## Basic Syntax

Import `zkvm_test::test` and annotate your test functions:

```rust
use zkvm_test::test;

#[test]
fn test_basic_addition() {
    let a = 20;
    let b = 22;
    assert_eq!(a + b, 42);
}
```

When building for the zkVM (`target_os = "zkvm"`), the macro generates the embedded metadata catalog and dispatch hooks required by `zkvm-test::test_runner`.

When building for the host (`target_os != "zkvm"`), the macro expands to a standard Rust test function. This means the exact same test file can be executed under native `cargo test` without any modifications.

---

## Attribute Arguments

The `#[test]` attribute accepts optional configuration arguments:

| Argument | Type | Purpose | Example |
| :--- | :--- | :--- | :--- |
| `budget` | Integer | Hard limit on maximum permitted VM cycles | `#[test(budget = 500_000)]` |
| `fixture` | String | Path to an external fixture file injected via host stdin | `#[test(fixture = "block_100.bin")]` |
| `snapshot` | String | Golden output file path for public values comparison | `#[test(snapshot = "header_hash.snap")]` |
| `should_panic` | Boolean / String | Asserts that the test panics (optional substring match) | `#[test(should_panic = "overflow")]` |
| `exit_code` | Integer | Asserts that the guest halts with a specific non-zero exit code | `#[test(exit_code = 3)]` |
| `ignore` | Boolean | Skips the test during default runs | `#[test(ignore)]` |

Arguments can be combined in comma-separated lists:

```rust
#[test(budget = 1_000_000, fixture = "headers/batch_01.hex")]
fn test_block_header_batch_validation() {
    let raw_bytes = zkvm_test::fixture();
    let validated_count = process_header_batch(&raw_bytes);
    assert_eq!(validated_count, 16);
}
```

---

## Structuring Test Code

In idiomatic Rust crates, unit tests are placed in a `tests` module inside `src/lib.rs` (or distributed across source files) guarded by `#[cfg(test)]`:

```rust
// In src/crypto/hash.rs

pub fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    // hashing logic...
    [0u8; 32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use zkvm_test::test;

    #[test]
    fn test_hash_pair_symmetry() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        assert_ne!(hash_pair(&a, &b), hash_pair(&b, &a));
    }
}
```

Integration tests located in `tests/*.rs` are also fully supported. The supervisor automatically compiles integration test binaries and runs them through the same zkVM harness.
