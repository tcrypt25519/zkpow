# Your First zkVM Test

This guide takes you through adding `zkvm-test` to an existing Rust crate and running your first in-VM test.

---

## 1. Add Dependencies

Add `zkvm-test` to your crate's `Cargo.toml` under `[dev-dependencies]`:

```toml
[package]
name = "my-zkvm-crate"
version = "0.1.0"
edition = "2021"

[dependencies]
sp1-zkvm = "6.6.0"

[dev-dependencies]
zkvm-test = "0.1.0"
```

If you are developing inside a workspace that defines `zkvm-test`, you can reference it via `workspace = true`:

```toml
[dev-dependencies]
zkvm-test = { workspace = true }
```

---

## 2. Configure the Test Runner in Crate Root

In bare-metal zkVM targets, the standard Rust test harness requires a custom test runner entry point. Enable `#![feature(custom_test_frameworks)]` in your crate root (`src/lib.rs` or `src/main.rs`):

```rust
// In src/lib.rs
#![cfg_attr(target_os = "zkvm", feature(custom_test_frameworks))]
#![cfg_attr(target_os = "zkvm", test_runner(zkvm_test::test_runner))]
#![cfg_attr(target_os = "zkvm", reexport_test_harness_main = "test_main")]

// If writing a binary crate (src/main.rs) rather than a library:
#[cfg(target_os = "zkvm")]
fn main() {
    #[cfg(test)]
    test_main();
}
```

> **Why is this necessary?**  
> On native host targets (macOS, Linux), the compiler provides a default runtime test harness. When compiling for `target_os = "zkvm"`, `zkvm-test::test_runner` replaces the host runtime with a lightweight embedded catalog dispatcher that communicates test cases and outcomes back to the host supervisor.

---

## 3. Write a Test Function

Write a test function using `zkvm_test::test`. Tests can be organized in a submodule or written inline:

```rust
pub fn fibonacci(n: u32) -> u32 {
    let mut a = 0;
    let mut b = 1;
    for _ in 0..n {
        let temp = a;
        a = b;
        b = temp + b;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use zkvm_test::test;

    #[test]
    fn test_fibonacci_correctness() {
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(1), 1);
        assert_eq!(fibonacci(7), 13);
        assert_eq!(fibonacci(10), 55);
    }
}
```

---

## 4. Run the Test

Execute the test in the zkVM using `cargo zkvm test`:

```console
$ cargo zkvm test -p my-zkvm-crate
cargo-zkvm: testing 1 zkVM package: my-zkvm-crate
   Compiling my-zkvm-crate v0.1.0 (/workspace/my-zkvm-crate)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.42s

     Running zkVM tests for `my_zkvm_crate` (target/zkvm/riscv64im-succinct-zkvm-elf/debug/deps/my_zkvm_crate-abc123)
────────────
    Starting 1 test across 1 binary (my_zkvm_crate)
      Target config: (mode: mock, threads: 16)
        PASS my_zkvm_crate::tests::test_fibonacci_correctness
             [     69,120 cyc │     71,850 gas │   19.42 MB │   0.01s ]

────────────
     Summary [  0.01s] 1 test run: 1 passed, 0 failed, 0 ignored
             Total: 69,120 cycles │ 71,850 gas │ peak rss: 85.12 MB
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

You have successfully executed your first unit test inside the zkVM!

The test output indicates:
- **`69,120 cyc`**: Total instruction cycles consumed during VM execution.
- **`71,850 gas`**: Estimated prover gas required for proving.
- **`19.42 MB`**: The peak memory footprint of the STARK execution trace.
- **`0.01s`**: Host elapsed execution time.
