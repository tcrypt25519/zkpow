//! Target runner binary for `riscv64im-succinct-zkvm-elf` test binaries.
//!
//! Features:
//! - Per-test isolation: each test executes in an isolated VM instance; a panic never halts other tests.
//! - Test filtering: test name substring matching, `--exact`, `--list`, `--ignored`.
//! - `#[should_panic]` and `#[should_panic, exit_code = N]` support.
//! - Output capture: captures guest `stdout` and `stderr` streams, weaving them into failure reports and `--nocapture`.
//! - Configurable prover: `--prover <mock|cpu|cuda|network>` and `ZKVM_PROVER` env var.
//! - Cycle counts: reports exact VM instruction/cycle counts per test and total.
//! - Parallel execution: runs test instances concurrently across threads.
//! - Tier 1 fallback: seamlessly runs legacy/stock libtest binaries if present.

use zkvm_test_runner::runner::{parse_args_from, run};

#[tokio::main]
async fn main() {
    let args = parse_args_from(std::env::args().skip(1));
    let exit_code = run(args).await;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
