# Panics and Exit Codes

Testing error handling and validation branches is as essential as verifying success paths. `zkvm-test` provides robust mechanisms to assert that guest code panics with expected diagnostics or halts with specific exit codes.

---

## Testing Panics with `should_panic`

To assert that a test fails via `panic!`, annotate it with `#[test(should_panic)]`:

```rust
use zkvm_test::test;

#[test(should_panic)]
fn test_divide_by_zero_panics() {
    let zero = 0;
    let _ = 42 / zero;
}
```

If the test completes without panicking, `cargo zkvm test` reports a failure:

```console
        FAIL my_crate::tests::test_divide_by_zero_panics
             [ note: test did not panic as expected ]
```

### Matching Panic Messages

You can assert that the panic message contains a specific substring using `#[test(should_panic = "expected text")]`:

```rust
pub fn verify_signature(sig_len: usize) {
    if sig_len != 64 {
        panic!("invalid signature length: expected 64, got {}", sig_len);
    }
}

#[test(should_panic = "invalid signature length")]
fn test_signature_too_short_fails() {
    verify_signature(32);
}
```

If the test panics with a different message, the failure is reported alongside the observed panic payload.

---

## High-Resolution Panic Diagnostics

In bare-metal zkVM targets without a standard OS allocator, recovering formatted panic strings can be challenging. `zkvm-test` includes an embedded panic hook that decodes:
- Static string slices passed to `panic!("message")`.
- Formatted arguments passed via `panic!("error code: {}", code)` without requiring dynamic heap allocation.
- Source file location and line numbers.

When a test unexpectedly panics, the runner displays the exact panic message:

```console
        FAIL my_crate::tests::test_unexpected_condition
             [     74,164 cyc │     77,509 gas │   20.94 MB │   0.03s ]
             thread 'main' panicked at src/lib.rs:88:9:
             assertion `left == right` failed: target hash mismatch
               left: 000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f
              right: 0000000000000000000000000000000000000000000000000000000000000000
```

---

## Custom Exit Codes

zkVM guest programs frequently communicate fatal errors or early termination by calling runtime halt syscalls with non-zero exit codes (such as SP1's `sp1_zkvm::io::write` or custom halt handlers).

Use `#[test(exit_code = N)]` to verify that a function exits with a specific code:

```rust
use zkvm_test::test;

#[test(exit_code = 3)]
fn test_fatal_protocol_violation() {
    // In zkpow-core: invalid hint parsing halts with status code 3
    zkpow_core::halt::fatal_protocol_abort(3);
}
```

When run:
```console
        PASS my_crate::tests::test_fatal_protocol_violation
             [     74,164 cyc │     77,509 gas │   20.94 MB │   0.03s ] [exit: 3]
```

---

## Hardware and Runtime Fault Diagnostics

If guest execution triggers an unhandled processor exception rather than a graceful panic, `cargo-zkvm` captures and reports detailed machine-level diagnostics:

- **Memory Faults**: Unmapped memory reads/writes, out-of-bounds stack accesses, and illegal page addresses with the offending virtual address.
- **Alignment Violations**: Misaligned 32-bit/64-bit load and store operations.
- **Syscall Errors**: Unimplemented or unrecognized runtime ecall numbers.
- **Watchdog Terminations**: Execution cycles exceeding safety caps.
