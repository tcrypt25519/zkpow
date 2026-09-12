# Output and Logging

Guest programs running inside a zkVM frequently produce debug logs via `println!`, `eprintln!`, or logging macros (`log::debug!`, `tracing::info!`). Because tests execute in parallel across multiple worker threads, capturing and streaming output requires clean isolation.

---

## Default Output Buffering

By default, `cargo-zkvm` captures all standard output and standard error emitted by each guest test execution:

```rust
use zkvm_test::test;

#[test]
fn test_output_capture() {
    println!("Step 1: Initializing header accumulator...");
    println!("Step 2: Processing batch with 10 elements");
    assert!(true);
}
```

- **Passing Tests**: Output is buffered in memory and suppressed from the terminal to keep the test runner output clean, concise, and easy to read.
- **Failing Tests**: If a test fails or panics, its complete buffered output is printed immediately below the failure badge, helping you diagnose the issue.

---

## Disabling Output Capture (`--nocapture`)

To inspect guest output live as it executes, pass `--nocapture` (or `--no-capture`):

```bash
cargo zkvm test -p zkpow-sha256 --nocapture
```

When `--nocapture` is passed, the runner streams stdout directly to your terminal.

> **Tip**: When using `--nocapture`, consider setting `--test-threads 1` so that output from concurrent tests does not interleave in your terminal:
> ```bash
> cargo zkvm test -p zkpow-sha256 --nocapture --test-threads 1
> ```

---

## Log Filtering and Verbosity

You can control log verbosity using standard environment variables:

```bash
RUST_LOG=info cargo zkvm test -p zkpow-sha256
```

Inside the guest, SP1's logging hooks capture log events and channel them through the runner's trace diagnostic stream.
