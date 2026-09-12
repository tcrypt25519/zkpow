# Snapshots

Testing serialized state outputs, cryptographic public values, and Merkle tree roots often requires comparing complex binary or hex outputs against known-good "golden" files.

`zkvm-test` provides integrated **golden snapshot testing** that automatically verifies outputs and supports one-flag golden file updates.

---

## Writing a Snapshot Test

Declare `snapshot = "path/to/snapshot.hex"` (or `.snap`) in the `#[test]` attribute:

```rust
use zkvm_test::test;

#[test(snapshot = "genesis_header.hex")]
fn test_genesis_header_hash_snapshot() {
    let genesis_header = [0u8; 80];
    let computed_hash = zkpow_sha256::double_sha256(&genesis_header);
    
    // Commit public values for snapshot comparison:
    zkvm_test::commit(&computed_hash);
}
```

Inside the test:
- Call `zkvm_test::commit(slice)` (or `sp1_zkvm::io::commit_slice`) to write public values.
- When the test completes, `cargo-zkvm` compares the committed bytes against the contents of the snapshot file.

---

## Snapshot File Locations

Snapshots are resolved relative to the test package:
1. `<crate_root>/snapshots/<filename>`
2. `<crate_root>/tests/snapshots/<filename>`
3. `<workspace_root>/snapshots/<filename>`

---

## Updating Snapshots (`--accept-snapshots`)

When writing a new test, or when intentionally changing committed output formats, pass `--accept-snapshots`:

```bash
cargo zkvm test -p zkpow-sha256 --accept-snapshots
```

If the snapshot file does not exist, `cargo-zkvm` creates it automatically with the committed bytes formatted as hexadecimal.

If the snapshot file exists but differs from the test output:
- **Without `--accept-snapshots`**: The test fails, showing a diff between expected and actual public values.
- **With `--accept-snapshots`**: The test passes and overwrites the golden file on disk with the new values.

```console
        PASS zkpow_sha256::tests::test_genesis_header_hash_snapshot [snapshot updated]
             [     81,340 cyc │     82,726 gas │   22.97 MB │   0.01s ]
```
