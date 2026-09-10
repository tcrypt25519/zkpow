`zkvm-test` doesn't exist in this repo. It's a spec for a separate tool, and the guide references `crates/zkvm-test-runner` in some other tree. So this is a comparison of what SP1 ships against what the guide proposes.

## The architectural difference is the whole story

Everything else is downstream of this: SP1 puts the test on the host, `zkvm-test` puts it in the guest.

Today the guest is one `main` behind `entrypoint!`, and a "test" is a host function that builds an `SP1Stdin`, calls `execute`, and asserts on public values. To get N cases you either ship N guest binaries (`patch-testing/k256/program/Cargo.toml` declares four `[[bin]]` targets) or multiplex on an input byte (`invalid-hint` reads a `trigger: u8` and branches). Both are workarounds for not having test discovery.

`zkvm-test` uses `#![feature(custom_test_frameworks)]` with `test_runner(zkvm_test::test_runner)`, so tests live in the guest crate, compile into one test ELF, and the runner enumerates and executes each one. That's the `cargo test` model and it deletes the bin-per-case explosion entirely. The succinct toolchain is nightly, so the feature is available. The `cfg_attr(target_os = "zkvm", ...)` guard is the right call for keeping `--host` builds on stable.

It also gets the target triple right. The guide says `riscv64im-succinct-zkvm-elf`, matching `crates/build/src/lib.rs:17`. SP1's own compiling docs still say `riscv32im`.

## Feature by feature

| Guide feature                                | SP1 today                                                                | Gap                                   |
| -------------------------------------------- | ------------------------------------------------------------------------ | ------------------------------------- |
| Test discovery, `--list`, `--exact`, filters | Nothing. You write a host driver.                                        | Total                                 |
| Per-test isolation                           | One child process per `execute` already (x86_64 Linux)                   | Mostly there, needs per-test executes |
| Parallel test threads                        | Patch CI forces `--test-threads=1`                                       | Total                                 |
| Cycles per test                              | `sp1_test` prints `Cycle Count: N`, writes JSON if `SP1_PATCH_BENCH` set | Internal only                         |
| Gas per test                                 | `report.gas()` exists, nothing displays it per test                      | Presentation                          |
| `should_panic(expected = "...")`             | Assert `exit_code == 1` by hand, grep a `.stderr(tx)` watch channel      | Total                                 |
| `exit_code = 42`                             | Manual assert (`invalid-hint` does this for code 3)                      | Ergonomics                            |
| `max_cycles = 250_000`                       | `.cycle_limit()` exists but is coarse. See below.                        | Real                                  |
| Sub-routine cycle tree                       | Flat `HashMap<String, u64>`, needs the `profiling` feature               | Real                                  |
| Output capture, bounded snapshots            | Shipped in #2924, 2 KiB watch channels                                   | Parity                                |
| `fixture = "..."`                            | Nothing. `include_bytes!` or hand-written stdin.                         | Total                                 |
| `snapshot = "..."`                           | Nothing                                                                  | Total                                 |
| JUnit XML                                    | Nothing                                                                  | Total                                 |
| Nextest `-E` expressions                     | Nothing                                                                  | Total                                 |
| Baselines, `--fail-on-regression N%`         | Patch cycle bot: informational, exits 0, internal                        | Total for users                       |
| `--prover mock/cpu/cuda`                     | `SP1_PROVER` env, `prove`/`gpu` cargo features                           | Closest to parity                     |
| `--mode core/compressed/plonk/groth16`       | All exist on `ProveRequest`                                              | Presentation                          |
| Instruction categories                       | `ExecutionReport` Display already prints opcode and syscall counts       | Presentation                          |
| `--host` dual compilation                    | Docs recommend it, no tooling                                            | Total                                 |
| `target/zkvm` cache isolation                | `sp1-build` uses `target/elf-compilation` for guest ELFs                 | Partial                               |

The guide says it categorizes "all 53 SP1 RISC-V opcodes." I counted the `Opcode` enum: exactly 53. Whoever wrote this read the source.

## Some potential issues

**`max_cycles` can't be built on `cycle_limit`.** The guide's fault table lists "Cycle Limit Exceeded" as an `ExecutionError` variant it decodes. `ExceededCycleLimit` is only constructed in two places, `crates/prover/src/worker/controller/core.rs:329` and `crates/prover/src/worker/prover/execute.rs:250`, and both check at chunk boundaries. The execute path checks inside the `gas_handles.next()` branch, so it only fires when a gas chunk report lands, and gas chunks are 134,217,728 memory entries. A 250k cycle test produces one chunk and the check happens once, at the end. That's why `test_cycle_limit_fail` in `crates/sdk/src/lib.rs:118` is `#[ignore]`d with "The cycle limit logic needs to be reimplemented."

The feature is still fine. Just don't implement it that way. Read `report.total_instruction_count()` after the run and fail the test. Post-hoc budget checking is trivial and exact. The in-flight limit is a runaway guard, not a budget, and the guide conflates them.

**Per-test gas at 16 threads is a problem.** The sample output shows "4,210 cycles, 95 gas" with "threads: 16." Gas estimation sizes a shared memory ring at `gas_trace_chunk_threshold` (about 2.2 GiB per chunk) times `gas_trace_chunk_slots` (2), so roughly 4.4 GiB per concurrent estimation (`crates/core/executor/src/opts.rs:44`). tmpfs is sparse so small tests won't touch all of it, but the segments are still created per runner, and `/dev/shm` exhaustion shows up as SIGBUS in the child. Shrinking the threshold is the obvious fix, except the repo has a test at `crates/core/runner/src/tests.rs:133` that exists specifically to show gas changes when you change that threshold. So per-test gas is either heavy or not comparable to the number the network quotes you. Pick one and document it.

**The sample output contradicts itself.** Line 689: `running 3 tests (prover: mock, mode: Core, threads: 16)`. Mock prover with Core proof mode means no actual proof was generated. Either the header is wrong or `--mode core` silently does nothing under `--prover mock`.

**Unstated: how does `run_track` get its numbers?** If it goes through `report.cycle_tracker`, the runner needs `sp1-sdk/profiling`, and that flag makes `detect_executor` emit `sp1_use_portable_executor` instead of native (`crates/core/executor/src/build.rs:4`). Every test in the suite drops to the interpreter, not just the profiled ones. If instead the runner parses the markers out of captured guest stdout itself, it sidesteps the feature entirely and keeps the JIT. That's the right answer and the guide should say so explicitly.

**Executor reuse isn't mentioned.** "Hundreds of tests in seconds" means hundreds of `execute` calls, each transpiling the program, spawning a child, and creating SHM segments. `MinimalExecutorRunner::reset()` exists for exactly this, and the prover keeps a `minimal_executor_cache` (`crates/prover/src/worker/controller/core.rs:265`). A runner that doesn't reuse will spend most of its wall clock on setup.

## The part that's just correct

The baseline gating is the right answer to a problem SP1 has and handles badly. `patch-testing-cycles.yml` prints a table and exits 0. No threshold, no gate, `HashMap` row ordering, and both runs generate their own unseeded random inputs so the diff column is partly RNG.

And the guide's own FAQ, question three, tells you cycle drift comes from "random number generators without fixed seeds." SP1's cycle regression bot has exactly that bug. The guide diagnosed the incumbent's defect in its troubleshooting section without noticing.
