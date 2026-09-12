# Troubleshooting

This chapter covers common issues, compiler diagnostics, and operational questions when using `zkvm-test`.

---

## 1. `getrandom` Compilation Failure

### Symptom
Compiling a guest program or crate fails with errors such as:
```
error[E0433]: failed to resolve: could not find `custom` in `backend`
  --> .../getrandom-0.3.x/src/lib.rs
```

### Cause
`getrandom` version 0.3 on bare-metal RISC-V targets requires explicit backend configuration via `--cfg getrandom_backend="custom"`.

### Solution
`cargo-zkvm` automatically injects this configuration flag during compilation. If you are invoking `cargo` manually or overriding `RUSTFLAGS`, ensure the flag is present:
```bash
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo zkvm test
```

---

## 2. Missing `rustdoc` in Succinct Toolchain

### Symptom
Running `cargo +succinct test` fails at the doctest stage with:
```
error: 'rustdoc' is not installed for the custom toolchain 'succinct'
note: test exited abnormally
```

### Cause
The upstream SP1 toolchain releases only ship `rustc` and the RISC-V target standard libraries, omitting `rustdoc`. When Cargo defaults to executing documentation tests on libraries, it cannot find `rustdoc`.

### Solution
Run your tests using `cargo zkvm test` rather than `cargo +succinct test`. `cargo-zkvm` directly executes `rustc` for target test compilation without triggering host doctest runner invocations.

Alternatively, pass `--tests` or `--lib` if invoking cargo directly:
```bash
cargo +succinct test -p my-crate --tests
```

---

## 3. Rustup Proxy Fallback Notices

### Symptom
Terminal displays informational warnings on every invocation:
```
info: cargo is unavailable for the active toolchain 'succinct'
info: falling back to latest installed cargo
```

### Cause
Running `cargo +succinct` tells rustup's proxy to look for `cargo` in the custom `succinct` toolchain directory. Because Succinct does not package `cargo`, rustup emits a fallback notice.

### Solution
Use `cargo zkvm test`. The runner queries `rustup which --toolchain succinct rustc` and passes `RUSTC=<path>` directly to host Cargo, bypassing rustup's proxy wrapper and eliminating the notice.

---

## 4. Stack Overflow or Out-of-Bounds Memory Fault

### Symptom
Test fails with a memory fault diagnostic:
```
FAIL my_crate::tests::test_large_allocation
     memory fault: access to unmapped address 0x77fffff0
```

### Cause
SP1 programs configure the guest stack starting at `0x78000000` (`2013265920`) growing downward. If a test allocates large arrays or deep recursive frames on the stack without sufficient headroom, it may cross the unmapped page boundary.

### Solution
1. Avoid allocating massive arrays directly on the stack (e.g. `let buf = [0u8; 1024 * 1024]`). Use heap structures like `alloc::vec::Vec`.
2. Verify that your build includes the correct link argument:
   `-C link-arg=--image-base=2013265920` (auto-injected by `cargo-zkvm`).

---

## 5. Fixture File Not Found

### Symptom
```
panic: zkvm-test: fixture 'data.bin' could not be resolved
```

### Cause
The requested fixture could not be located in any of the 5 standard search paths.

### Solution
Ensure the fixture file is placed in one of the expected directories:
- Next to `Cargo.toml` in `fixtures/data.bin`
- In `tests/fixtures/data.bin`
- In the workspace root in `fixtures/data.bin`
- Or specify an explicit path via the `--fixtures <DIR>` CLI flag or `ZKVM_FIXTURES` environment variable.
