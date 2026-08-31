//! Cargo target runner for `riscv64im-succinct-zkvm-elf` binaries.
//!
//! Cargo invokes this with the ELF path when running tests or binaries built
//! for the zkVM target (wired up in `.cargo/config.toml`). The ELF executes in
//! the SP1 executor with empty stdin, and the guest's halt code becomes this
//! process's exit code, so `cargo test` reports pass and fail correctly.
//!
//! Build it once before running zkVM tests:
//!
//! ```text
//! cargo build --release -p zkvm-test-runner
//! cargo +succinct zkvm-test -p zkpow-sha256
//! ```

use std::{sync::Arc, time::Instant};

use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(elf_path) = args.next() else {
        eprintln!("usage: zkvm-test-runner <elf-path>");
        std::process::exit(2);
    };
    let extra: Vec<String> = args.collect();
    if !extra.is_empty() {
        eprintln!(
            "zkvm-test-runner: cannot forward harness arguments {extra:?} into the zkVM; \
             the guest has no argv, so test filters and libtest flags are unsupported"
        );
        std::process::exit(2);
    }

    let elf_bytes = match std::fs::read(&elf_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("zkvm-test-runner: failed to read {elf_path}: {err}");
            std::process::exit(2);
        },
    };

    let client = ProverClient::builder().mock().build().await;
    match client.execute(Elf::from(elf_bytes), SP1Stdin::new()).await {
        Ok((_public_values, report)) => {
            let exit_code = u8::try_from(report.exit_code).unwrap_or(255);
            if exit_code != 0 {
                eprintln!("zkvm-test-runner: guest exited with code {}", report.exit_code);
            }
            std::process::exit(exit_code.into());
        }
        Err(err) => {
            eprintln!("zkvm-test-runner: execution failed: {err}");
            std::process::exit(1);
        }
    }
}
