# Fixtures

zkVM guest programs rarely run in isolation—they process external transaction payloads, block headers, state proofs, and cryptography test vectors. Embedding large test vectors directly into Rust source files bloats compilation units and pollutes code readability.

`zkvm-test` provides a unified **host-injected fixture system** allowing tests to declare and load external files seamlessly across both in-VM and host test executions.

---

## Declaring a Fixture

Attach `fixture = "path/to/file"` to the `#[test]` attribute:

```rust
use zkvm_test::test;

#[test(fixture = "block_headers.bin")]
fn test_block_headers_fixture_verification() {
    // Read the raw binary bytes from the host-provided fixture:
    let bytes: &'static [u8] = zkvm_test::fixture();
    assert_eq!(bytes.len(), 800); // 10 headers * 80 bytes
}
```

For UTF-8 text or hex files, use `zkvm_test::fixture_str()`:

```rust
#[test(fixture = "mainnet_genesis.hex")]
fn test_genesis_hash_from_hex() {
    let hex_str = zkvm_test::fixture_str();
    let decoded = hex::decode(hex_str.trim()).expect("valid hex");
    assert_eq!(decoded.len(), 80);
}
```

---

## Unified Search Hierarchy

Both `cargo zkvm test` (in-VM) and `cargo test` (host) locate fixtures using the exact same five-level search hierarchy:

1. **CLI Flag (`--fixtures <DIR>`)**:  
   Overrides the fixture root directory from the command line:
   ```bash
   cargo zkvm test --fixtures /custom/testdata
   ```
2. **Environment Variable (`ZKVM_FIXTURES`)**:  
   Allows CI configurations or scripts to specify a custom fixture root:
   ```bash
   export ZKVM_FIXTURES=/shared/data/fixtures
   ```
3. **Crate-Level Directory (`<crate_root>/fixtures/`)**:  
   Searches a `fixtures/` directory located next to the crate's `Cargo.toml`.
4. **Crate Integration Tests Directory (`<crate_root>/tests/fixtures/`)**:  
   Searches `tests/fixtures/` within the crate.
5. **Workspace Root Directory (`<workspace_root>/fixtures/`)**:  
   Searches `fixtures/` at the root of the Cargo workspace, allowing multiple member crates to share common test vectors (e.g. historical blockchain blocks).

---

## How Fixture Injection Works

- **Under `cargo zkvm test` (in-VM)**:  
  The supervisor reads the fixture file from the host filesystem before spawning the test. It transmits the fixture data over standard input using the protocol's `CMD_FIXTURE` stream. Inside the guest, `zkvm_test::fixture()` retrieves this stream directly from memory with zero host filesystem dependencies.
- **Under `cargo test` (host)**:  
  `zkvm_test::fixture()` runs natively on the host, traverses the same search hierarchy, reads the file from disk using standard `std::fs`, and caches the slice in memory.

Because both execution modes use the exact same search logic, tests never fail due to path divergence when switching between host and guest.
