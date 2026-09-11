# zkVM Test Runner: Developer Experience and Toolchain Integration Proposal

**A proposal for standalone global distribution, transparent toolchain orchestration, and zero-configuration testing for zkVM applications.**

|                |                                                                                             |
| -------------- | ------------------------------------------------------------------------------------------- |
| **Status**     | Request for Comments (Draft)                                                                |
| **Owner**      | Tyler (tcrypt) / Core Tooling                                                               |
| **Scope**      | Distribution, Cargo subcommand UX, toolchain encapsulation, and zero-config workspace setup |
| **Related**    | [zkvm-test.md](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/docs/zkvm-test/plans/zkvm-test.md), [zkvm-test-guide.md](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/docs/zkvm-test/zkvm-test-guide.md) |
| **Audience**   | zkVM engineers, compiler/toolchain maintainers, and downstream SP1/RISC Zero developers     |

---

## 1. Summary

We propose elevating `zkvm-test-runner` to a globally distributable, standalone Cargo plugin named `cargo-zkvm-test` (installed once via `cargo install cargo-zkvm-test` or bundled into `sp1up`).

Under this design:
1. **Developers invoke tests via `cargo zkvm-test`** without toolchain specifiers (`+succinct`), without target flags (`--target riscv64im-succinct-zkvm-elf`), and without custom Cargo aliases.
2. **The plugin acts as a compilation and execution supervisor** (mirroring `cargo-nextest`), directly orchestrating the build subprocess, automatically injecting required SP1 linker/runtime flags (`-C link-arg=--image-base=2013265920`, `-C passes=lower-atomic`, `--cfg getrandom_backend="custom"`), and extracting ELF test artifacts from the compiler JSON stream.
3. **Repository configuration becomes zero-overhead**: projects no longer require `runner` entries or target-specific `rustflags` in `.cargo/config.toml`.
4. **Standard `cargo test` remains intact for rapid host verification** (`cfg(not(target_os = "zkvm"))`), while `cargo zkvm-test` provides true in-VM execution, cycle counting, instruction breakdown, and hierarchical trace profiling.

---

## 2. Context and Problem Analysis

### 2.1 Current Friction in zkVM Testing Workflows

The current zkVM developer experience suffers from four primary friction points:

1. **Toolchain Dispatch Fragmentation (`cargo +succinct ...`):**
   SP1 programs require the custom `succinct` Rust toolchain to compile for `riscv64im-succinct-zkvm-elf`. Because Cargo aliases (`.cargo/config.toml`) cannot specify rustup toolchain flags (rustup parses `+toolchain` before cargo is invoked), users cannot create a simple alias like `cargo test-zkvm`. Every invocation requires typing `cargo +succinct ...`.
2. **Missing `rustdoc` in Custom Toolchains:**
   The `succinct` toolchain does not distribute a custom `rustdoc`. Running standard `cargo test` under `+succinct` attempts to build and run documentation tests, which immediately fails unless `--lib`, `--tests`, or `--no-run` is passed.
3. **Pervasive `.cargo/config.toml` Boilerplate:**
   To link an SP1 guest binary, `rustc` requires specific low-level arguments (`image-base=0x78000000`, `passes=lower-atomic`, `panic=abort`). In current setups, every downstream repository must maintain these flags in `.cargo/config.toml`, alongside a `runner = ["cargo-zkvm-test"]` hook. If a new repository omits these flags, compilation fails with cryptic linker errors.
4. **Disjoint Tooling between Building (`cargo prove`) and Testing:**
   Developers build guest ELFs using `cargo prove build`, but have no unified tool for testing. Running tests via custom scripts, manual target runners, or off-target mock harnesses introduces split mental models between build environments and test execution.

---

## 3. Architecture and UX Design

```
+-----------------------------------------------------------------------------+
|                               Developer Action                              |
|                             $ cargo zkvm-test                               |
+-----------------------------------------------------------------------------+
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                      cargo-zkvm-test CLI Supervisor                         |
|   1. Inspects workspace package graph via guppy                             |
|   2. Detects target architecture & required toolchain (succinct / risc0)    |
|   3. Verifies toolchain installation via rustup                             |
+-----------------------------------------------------------------------------+
                                       |
                   Spawns compiler with injected env & flags
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                             Compilation Phase                               |
|   $ cargo +succinct test --no-run --message-format=json                     |
|     --target riscv64im-succinct-zkvm-elf                                    |
|     RUSTFLAGS="-C passes=lower-atomic -C link-arg=--image-base=2013265920"  |
+-----------------------------------------------------------------------------+
                                       |
                      Streams JSON Compiler Artifacts
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                             Execution Phase                                 |
|   For each discovered ELF:                                                  |
|   1. Query test catalog via two-phase protocol (CMD_LIST)                   |
|   2. Filter tests via nextest filter expressions                            |
|   3. Run isolated VM instances in parallel across worker pool               |
|   4. Capture guest stdout/stderr, cycles, and hierarchical profiles         |
|   5. Export JUnit XML / update golden snapshots                             |
+-----------------------------------------------------------------------------+
```

### 3.1 Global Distribution Model

`cargo-zkvm-test` will be packaged as a standalone host CLI utility:

1. **Crates.io Release:** Published as `cargo-zkvm-test`. Users install it globally via:
   ```bash
   cargo install cargo-zkvm-test --locked
   ```
2. **Pre-compiled Binary Distribution:** Pre-built releases for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, and `aarch64-apple-darwin`, installable via `cargo-binstall`:
   ```bash
   cargo binstall cargo-zkvm-test
   ```
3. **Upstream Bundling via `sp1up`:**
   Because `cargo-zkvm-test` is designed for the SP1 and RISC Zero ecosystems, we propose an RFC to Succinct to bundle `cargo-zkvm-test` into `sp1up`. Running `sp1up` will install `cargo-prove`, the `succinct` toolchain, and `cargo-zkvm-test` in a single command.

> **Why & What - Standalone Binary vs Workspace Build Dependency**
>
> **What:** Package `cargo-zkvm-test` as an independently installed host CLI tool rather than a repository build dependency or artifact dependency.
>
> **Why:** Cargo artifact dependencies (`-Z bindeps`) require the nightly compiler on every project build and tie the test runner lifecycle to the workspace lockfile. A global host installation mirrors `cargo-nextest` and `cargo-audit`, allowing immediate usage across any existing repository without modifying `Cargo.toml`.
>
> **Alternatives considered:**
> - *Cargo artifact dependency (`bindeps` in Cargo.toml):* Requires `-Z bindeps`, permanently forcing developers and CI into nightly toolchains even for simple builds.
> - *Submodule or vendored CLI:* Burdens every downstream project with maintaining build scripts and duplicate compile times.
>
> **Fallback:** Developers can install directly from git via `cargo install --git https://github.com/tcrypt25519/bitcoin-header-chain cargo-zkvm-test`.

---

### 3.2 Invocation and Toolchain Encapsulation

Developers run tests using standard Cargo syntax:

```bash
# Run all zkVM tests across the workspace
cargo zkvm-test

# Run tests for a specific package
cargo zkvm-test -p zkpow-core

# Run specific test with substring filter
cargo zkvm-test mtp

# Run with Nextest boolean filter expressions
cargo zkvm-test -E 'package(zkpow-core) & test(header)'

# Profile cycles and instruction categories
cargo zkvm-test -p zkpow-core --profile --instructions
```

The user never writes `+succinct` or `--target riscv64im-succinct-zkvm-elf`. 

Under the hood, `cargo-zkvm-test` executes the following sequence:
1. **Toolchain Discovery:** Queries `rustup toolchain list`.
2. **Actionable Diagnosis:** If `succinct` is missing, the process exits with an explicit installation hint:
   ```
   error: required toolchain 'succinct' is not installed.
   info:  to install the SP1 toolchain, run:
            curl -L https://sp1.succinct.xyz | bash
            sp1up
   ```
3. **Subprocess Construction:** Spawns `cargo +succinct test --no-run --message-format=json --target riscv64im-succinct-zkvm-elf` with all user-specified feature flags, package filters, and build profiles.

> **Why & What - Subcommand Plugin vs Cargo Target Runner**
>
> **What:** Adopt the Cargo subcommand plugin model (`cargo zkvm-test`) and abandon reliance on Cargo target runners (`[target.<triple>] runner = ...`).
>
> **Why:** Cargo target runners are invoked serially by Cargo for each built binary, with Cargo managing the terminal, test selection, and lifecycle. A target runner cannot coordinate multi-package filtering, cannot prevent `rustdoc` invocations, cannot aggregate cycle counts across packages, and cannot encapsulate the `+succinct` toolchain flag. A subcommand supervisor owns the entire build-and-run lifecycle.
>
> **Alternatives considered:**
> - *Cargo Target Runner in `.cargo/config.toml`:* Keeps `cargo test` syntax, but fails on rustdoc, requires manual `+succinct`, and cannot support nextest filter expressions.
>
> **Fallback:** `zkvm-test-runner` retains its target runner entrypoint (`src/main.rs`) for environments that strictly require invocation via Cargo's runner interface.

---

### 3.3 Zero-Configuration Workspace Integration

Currently, an SP1 project requires specific entries in `.cargo/config.toml`:

```toml
# Legacy .cargo/config.toml requirement (TO BE ELIMINATED)
[target.riscv64im-succinct-zkvm-elf]
runner = ["cargo-zkvm-test"]
rustflags = [
  "-C", "passes=lower-atomic",
  "-C", "link-arg=--image-base=2013265920",
  "-C", "panic=abort",
  "--cfg", 'getrandom_backend="custom"',
]
```

Under this proposal, `cargo-zkvm-test` injects these flags directly when launching the build subprocess:
1. It reads existing `RUSTFLAGS` from the environment or `.cargo/config.toml`.
2. It appends the required compiler flags (`lower-atomic`, `image-base`, `panic=abort`, `getrandom_backend`).
3. It sets `CARGO_TARGET_DIR` to `<workspace>/target/zkvm` (unless overridden via `--target-dir`), preventing host and guest build artifacts from colliding.

**Downstream impact:** Downstream projects require **zero** configuration in `.cargo/config.toml`. A project only needs to add `zkvm-test` to its `[dev-dependencies]` in `Cargo.toml`.

---

### 3.4 Dual-Mode Testing Strategy: Host Simulation vs zkVM Fidelity

A central question in zkVM ergonomics is: *Should `cargo test` be replaced or preserved?*

We propose maintaining an intentional separation:

| Command | Target | Execution Engine | Speed | Use Case |
| :--- | :--- | :--- | :--- | :--- |
| `cargo test` | Host (`aarch64`, `x86_64`) | Native CPU (Libtest) | ~0.05s | Inner developer loop: rapid logic verification, mocking, and unit testing |
| `cargo zkvm-test` | `riscv64im-succinct-zkvm-elf` | SP1 zkVM Executor | ~0.50s | In-VM fidelity: cycle metering, memory bounds, proof mode, trace profiling |
| `cargo zkvm-test --all-targets` | Both Host & zkVM | Sequenced Host + VM | Combined | Full CI/CD validation before committing |

Because `zkvm-test` provides transparent off-target fallback (`cfg(not(target_os = "zkvm"))`), existing tests annotated with `#[zkvm_test]` run natively on the host during `cargo test`. 

Developers get the best of both worlds:
- Instant feedback during code edits via standard `cargo test`.
- Deep cryptographic and architectural verification via `cargo zkvm-test`.

> **Why & What - Preserving Host Tests vs Forcing All Tests Into the zkVM**
>
> **What:** Keep `cargo test` as an ultra-fast host test pass and reserve `cargo zkvm-test` for zkVM execution.
>
> **Why:** zkVM execution incurs an unavoidable simulation and runtime cost (emulating RISC-V instructions, syscall dispatch, trace generation). Forcing every local test run into the VM slows down the inner iteration loop by an order of magnitude and prevents using host debugging tools (lldb, gdb, valgrind).
>
> **Alternatives considered:**
> - *Alias `cargo test` to always cross-compile to zkVM:* Rejected because developers lose fast unit test iteration and host tooling.
>
> **Fallback:** Users can run `cargo zkvm-test --host` or `cargo zkvm-test --all-targets` to control execution scope from a single tool.

---

### 3.5 Eliminating the `cargo prove` Divergence

In the SP1 ecosystem, developers use `cargo prove build` to generate production ELFs. 

`cargo-zkvm-test` unifies the test workflow by adopting the identical compilation profile and flag definitions as `sp1-build`:
- Compilation targets the same ELF output format.
- Stack base addresses (`0x78000000`), atomic lowering, and memory configurations match production prover environments.
- Tests exercise the exact binary code that will be proven in production, eliminating discrepancies between test ELFs and production ELFs.

For command-line consistency across tools, `cargo zkvm-test` supports standard SP1 environment variables:
- `SP1_PROVER`: sets default backend (`mock`, `cpu`, `cuda`).
- `ZKVM_PROVER`: generic backend override.
- `SP1_DUMP_PATH`: dumps execution traces for external diagnostic tools.

---

## 4. Open Questions

| Question | Owner | Impact |
| :--- | :--- | :--- |
| **Q1:** Should the binary be named `cargo-zkvm-test` (invoked as `cargo zkvm-test`) or `cargo-zkvm` with subcommands (`cargo zkvm test`, `cargo zkvm profile`)? | Core Tooling / tcrypt | CLI taxonomy and future expansion to build/proving commands |
| **Q2:** How should toolchain auto-update be handled if the upstream Succinct toolchain releases a breaking update? | Tooling Maintainers | Version pinning between `sp1-sdk` dependency and toolchain release |
| **Q3:** Should `cargo-zkvm-test` feature flag support RISC Zero natively out of the box, or split into vendor-specific crates? | Ecosystem Integrations | Dependency footprint and build time of `cargo-zkvm-test` |

---

## 5. Benefits and Costs

### 5.1 Benefits
- **Zero-Friction Adoption:** New developers run `cargo install cargo-zkvm-test` and immediately run tests on any SP1 repository.
- **No `+succinct` Noise:** Developers stop wrestling with rustup toolchain flags and broken Cargo aliases.
- **Clean Configuration:** Eliminates custom linker flags, target runners, and dummy scripts from repository `.cargo/config.toml` files.
- **Deterministic CI:** Automated JUnit XML generation, regression detection (`--fail-on-regression`), and baseline comparisons work identically in CI and local machines.

### 5.2 Costs and Trade-offs
- **Binary Size & Compilation Time:** `cargo-zkvm-test` links `sp1-core-executor` and `sp1-sdk`, resulting in a non-trivial binary size (~100MB unstripped) and longer initial install times when built via `cargo install`. *Mitigation: Distribute pre-compiled release binaries via GitHub Releases and `cargo-binstall`.*
- **Toolchain Flag Tracking:** As SP1 evolves (e.g. changing `STACK_TOP` or linker requirements), `cargo-zkvm-test` must maintain alignment with `sp1-build`.

---

## 6. Residual Risks and What NOT to Do

1. **DO NOT attempt to hack Cargo's built-in `[alias]` table to support `+succinct`:** Cargo explicitly does not permit toolchain overrides in aliases. Workarounds using shell wrappers or makefiles break cross-platform portability.
2. **DO NOT hijack standard `cargo test` as the only way to run zkVM tests:** Forcing developers into the VM runner on every test invocation degrades iteration speed. Keep host tests fast and separate.
3. **DO NOT rely on runtime environment variables for mandatory compiler flags:** Inject flags explicitly during the subprocess invocation so tests remain reproducible regardless of the user's shell environment.

---

*This document is a Request for Comments. Technical facts are verified against `crates/zkvm-test-runner` and the `sp1-build` compiler specifications in commit `HEAD`.*
