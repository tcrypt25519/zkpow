# Cargo Configuration

`cargo-zkvm` is designed to be zero-configuration out of the box. However, projects can fine-tune package discovery, default target triples, and compiler flags using standard Cargo metadata and configuration files.

---

## Workspace Package Metadata

By default, running `cargo zkvm test` from the workspace root automatically tests every member crate that depends on `zkvm-test`.

You can explicitly enable or disable zkVM test discovery for any crate using `[package.metadata.zkvm-test]` in its `Cargo.toml`:

```toml
[package]
name = "my-special-crate"
version = "0.1.0"

[package.metadata.zkvm-test]
enable = true
```

Options:
- `enable = true` / `false`: Forces inclusion or exclusion during workspace discovery.

---

## Injected Compiler Flags (RUSTFLAGS)

When compiling for the SP1 target (`riscv64im-succinct-zkvm-elf`), `cargo-zkvm` automatically supplies all required low-level code generation flags if no environment flags are provided:

```
-C passes=lower-atomic
-C link-arg=--image-base=2013265920
-C panic=abort
--cfg getrandom_backend="custom"
-C llvm-args=-misched-prera-direction=bottomup
-C llvm-args=-misched-postra-direction=bottomup
```

### Non-Interference Policy
If your environment has already defined `CARGO_ENCODED_RUSTFLAGS`, `CARGO_RUSTFLAGS`, or `RUSTFLAGS`:
1. `cargo-zkvm` **does not overwrite** or interfere with your flags.
2. It inspects your active flags against the required SP1 list above.
3. If any required flag is missing or has an incorrect value (e.g. an invalid stack image base), the runner prints an informative warning on stderr while allowing execution to proceed.

---

## Build Directory Customization

By default, all zkVM compilation artifacts are isolated in `target/zkvm` relative to the workspace root.

To change this location:
- Pass the CLI option: `--target-dir <DIR>`
- Or set the environment variable:
  ```bash
  export CARGO_TARGET_DIR=/tmp/zkvm-build-cache
  ```
  *(When `CARGO_TARGET_DIR` is set, `cargo-zkvm` appends `zkvm/` to ensure host and guest build caches remain separated.)*
