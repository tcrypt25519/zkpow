# Selecting Which Tests Run

`cargo-zkvm` provides flexible test filtering options modeled directly on `cargo test` and `cargo-nextest`. You can filter by test name, crate package, execution target, or boolean filter expressions.

---

## Positional Name Filters

The simplest way to filter tests is by passing a substring pattern:

```bash
cargo zkvm test genesis
```

This runs only tests whose fully qualified path contains `"genesis"` (e.g. `zkpow_sha256::tests::test_genesis_header_hash_snapshot`).

### Exact Matching (`--exact`)

To run a test only when its name matches the pattern exactly, add `--exact`:

```bash
cargo zkvm test zkpow_sha256::tests::test_genesis_header_hash_snapshot --exact
```

---

## Package Selection

In multi-crate Cargo workspaces, you can scope execution to specific packages:

- **Single Package (`-p <SPEC>`)**:
  ```bash
  cargo zkvm test -p zkpow-sha256
  ```
- **Multiple Packages**:
  ```bash
  cargo zkvm test -p zkpow-sha256 -p zkpow-core
  ```
- **All Workspace Packages (`--workspace` or `--all`)**:
  ```bash
  cargo zkvm test --workspace
  ```
  *(Note: Running `cargo zkvm test` from the workspace root automatically defaults to all zkVM packages in the workspace.)*
- **Excluding Packages (`--exclude <SPEC>`)**:
  ```bash
  cargo zkvm test --workspace --exclude zkpow-core
  ```

---

## Nextest Filter Expressions (`-E`, `--expr`)

For complex selection criteria, `cargo-zkvm` supports Nextest-style boolean filter expressions using `-E` (or `--expr`):

```bash
cargo zkvm test -E 'package(zkpow-sha256) & test(header)'
```

Filter expressions support:
- `package(<name>)`: Matches packages by name.
- `test(<name>)`: Matches test function names.
- `binary(<name>)`: Matches test binary artifact names.
- `kind(<kind>)`: Matches target kind (`lib`, `test`, `bench`, `example`).
- Boolean operators: `&` (AND), `|` (OR), `!` (NOT), and parentheses `(...)`.

### Examples

Run all tests in `zkpow-core` except MTP tests:
```bash
cargo zkvm test -E 'package(zkpow-core) & !test(mtp)'
```

Run tests matching `genesis` across both `sha256` and `params` crates:
```bash
cargo zkvm test -E '(package(sha256) | package(params)) & test(genesis)'
```

---

## Listing Tests Without Executing (`--list`)

To inspect which tests match your filters without compiling or running them in the VM:

```bash
cargo zkvm test --list
```

Output:
```console
zkpow_sha256::tests::budgeted_hash_exceeding_limit: test (ignored)
zkpow_sha256::tests::guest_output_capture_demonstration: test
zkpow_sha256::tests::test_genesis_header_hash_snapshot: test
zkpow_sha256::tests::public_values_optimized_hash_matches_standard_sha256: test
```

---

## Running Ignored Tests

Tests annotated with `#[test(ignore)]` are skipped during normal test runs.

- **Run only ignored tests**:
  ```bash
  cargo zkvm test --ignored
  ```
- **Run both regular and ignored tests**:
  ```bash
  cargo zkvm test --include-ignored
  ```
