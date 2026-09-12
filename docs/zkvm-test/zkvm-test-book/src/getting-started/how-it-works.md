# How the Runner Works

Understanding the architecture of `cargo-zkvm` helps clarify how it achieves zero-configuration execution, builds hermetic test binaries, and measures instruction cycles without interfering with standard Cargo workflows.

---

## The Two-Phase Architecture

`cargo-zkvm` decouples test compilation from test execution into two clean phases, following the proven model established by `cargo-nextest`:

```
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 1: Compilation (Host Cargo + Succinct rustc)                     │
│ cargo test --no-run --message-format=json --target <zkvm-target>       │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼  (JSON compiler artifacts & ELFs)
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 2: Execution (cargo-zkvm supervisor)                             │
│ 1. Catalog Phase: Send CMD_LIST -> decode test functions               │
│ 2. Parallel Dispatch: Launch SP1 emulator instances across worker pool │
│ 3. Trace Processing: Stream execution trace -> extract cycle metrics  │
└────────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Compilation

When you run `cargo zkvm test`:
1. **Toolchain Resolution**: The supervisor identifies the location of the Succinct compiler via `rustup which --toolchain succinct rustc`.
2. **Flag Injection**: If the environment has not already defined `CARGO_ENCODED_RUSTFLAGS` or `CARGO_RUSTFLAGS`, `cargo-zkvm` injects the required SP1 compilation flags:
   - `-C passes=lower-atomic`: Lowers atomic instructions to single-core primitives.
   - `-C link-arg=--image-base=2013265920`: Sets the SP1 guest stack/code image base (`0x78000000`).
   - `-C panic=abort`: Ensures unwinding is disabled.
   - `--cfg getrandom_backend="custom"`: Routes entropy calls to SP1's custom runtime backend.
3. **Artifact Discovery**: Cargo is spawned with `--no-run --message-format=json`. The supervisor intercepts the JSON message stream, identifying every compiled test executable ELF.

---

## Build Cache Isolation: `target/zkvm`

Standard Cargo compiles host targets into `target/debug` or `target/release`. If zkVM builds were emitted into the same root target directory, switching between `cargo test` (host) and `cargo zkvm test` (RISC-V) would repeatedly invalidate intermediate build caches (`.rlib`, `.rmeta`), forcing full recompilations.

To preserve sub-second incremental build times, `cargo-zkvm` automatically directs all zkVM compilation artifacts to an isolated target subdirectory:

```
target/
├── debug/                        # Host build cache (cargo test)
├── release/
└── zkvm/                         # Isolated zkVM build cache (cargo zkvm test)
    └── riscv64im-succinct-zkvm-elf/
        └── debug/
            └── deps/
                └── my_crate-1a2b3c4d   # Test ELF
```

You can customize this location via `--target-dir` or the `CARGO_TARGET_DIR` environment variable.

---

## Phase 2: Cataloging and Execution

Once test ELFs are produced, `cargo-zkvm` executes each binary inside an isolated emulator:

1. **Test Discovery (`CMD_LIST`)**:
   - The supervisor executes the ELF with `CMD_LIST` on standard input.
   - The guest `zkvm-test::test_runner` detects the command, serializes the list of registered `#[test]` names and attributes into public values, and halts.
   - The supervisor decodes this catalog without running any test bodies.
2. **Filtering & Planning**:
   - The catalog is filtered against CLI pattern filters, `--exact`, and Nextest boolean expressions (`-E`).
   - Tests marked `#[ignore]` are skipped unless `--include-ignored` or `--ignored` is supplied.
3. **Concurrent Execution (`CMD_RUN`)**:
   - The supervisor spawns worker threads (configured via `--test-threads`, defaulting to the number of logical CPU cores).
   - Each worker launches an isolated SP1 emulator instance passing `CMD_RUN` and the test identifier on standard input.
   - The guest executes the selected test function.
4. **Metric Extraction & Reporting**:
   - Upon completion, the supervisor inspects the execution trace.
   - Total cycles, gas consumption, peak RSS, and guest `println!` output are extracted and formatted into the terminal report.
   - If an assertion fails or panic occurs, the emulator's exit state and backtrace are captured and attributed specifically to that test case.
