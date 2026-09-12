# Unified zkVM Developer Experience and Toolchain Integration Proposal

**A proposal for a unified Cargo plugin (`cargo-zkvm`) providing zero-configuration building, testing, trace profiling, and execution across zkVM applications.**

|                |                                                                                             |
| -------------- | ------------------------------------------------------------------------------------------- |
| **Status**     | Request for Comments (Draft)                                                                |
| **Owner**      | Tyler (tcrypt) / Core Tooling                                                               |
| **Scope**      | Distribution, CLI taxonomy (`cargo zkvm`), toolchain encapsulation, compiler flag injection, and workspace package discovery |
| **Related**    | [zkvm-test.md](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/docs/zkvm-test/plans/zkvm-test.md), [zkvm-test-guide.md](file:///Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/docs/zkvm-test/zkvm-test-guide.md) |
| **Audience**   | zkVM engineers, compiler and toolchain maintainers, and downstream SP1 and RISC Zero developers |

---

## 1. Summary

We propose replacing fragmented zkVM CLI tooling (`cargo prove`, `cargo-zkvm-test`, and manual `cargo +succinct` invocations) with a single, globally distributable Cargo plugin: `cargo-zkvm`.

Under this design:
1. **Unified CLI taxonomy:** Developers interact with a single command suite featuring dedicated subcommands:
   - `cargo zkvm test`: Nextest-grade test runner with cycle metering, hierarchical trace profiling, and parallel VM execution.
   - `cargo zkvm build`: Compiles guest programs to reproducible zkVM ELFs, replacing `cargo prove build` with accurate, non-misleading terminology.
   - `cargo zkvm run`: Executes guest binaries in the zkVM without generating cryptographic proofs, reporting cycle metrics and guest outputs.
   - `cargo zkvm prove`: Generates zero-knowledge STARK/SNARK proofs using local or distributed prover backends.
2. **Global distribution:** Installed once via `cargo install cargo-zkvm --locked`, pre-compiled via `cargo binstall cargo-zkvm`, or bundled directly into `sp1up`.
3. **Transparent toolchain and flag encapsulation:** The plugin directly supervises compilation subprocesses. It automatically discovers the active toolchain (`succinct` or `risc0`), silences rustup proxy warnings, resolves target architecture triples (`riscv64im` vs `riscv32im`), and injects all required compiler flags (`--cfg getrandom_backend="custom"`, `-C link-arg=--image-base=2013265920`, `-C passes=lower-atomic`, `-C panic=abort`).
4. **Zero-configuration workspace integration:** Downstream repositories require **no** `.cargo/config.toml` runner scripts, custom rustflags, or stack pointer constants. A repository only needs to declare dev-dependencies.
5. **Robust package and target discovery:** Eliminates compilation failures on non-guest workspace crates by validating guest compatibility before invoking the zkVM cross-compiler.

---

## 2. Grounding and Problem Analysis

### 2.1 Investigation of Current Flakiness and Failures

Recent developer testing revealed three distinct points of friction when using the test runner across workspace packages:

#### 2.1.1 Target Architecture Mismatch and the `getrandom` Failure
When running `cargo zkvm-test` across the workspace without a package filter (`-p`), compilation fails immediately:
```
cargo-zkvm-test: testing 4 zkVM packages: zkpow-core, zkpow-params, zkpow-sha256, zkpow-types
   Compiling getrandom v0.3.4
error: could not compile `getrandom` (lib) due to 4 previous errors
```

Investigation of the root cause revealed a configuration discrepancy:
1. The repository's `.cargo/config.toml` configured rustflags under `[target.riscv32im-succinct-zkvm-elf]`:
   ```toml
   [target.riscv32im-succinct-zkvm-elf]
   rustflags = [
     "-C", "passes=lower-atomic",
     "-C", "link-arg=--image-base=2013265920",
     "-C", "panic=abort",
     "--cfg", 'getrandom_backend="custom"',
   ]
   ```
2. The test runner plugin defaulted to the 64-bit target `riscv64im-succinct-zkvm-elf`.
3. Because the target triples differed (`riscv64im` vs `riscv32im`), Cargo ignored the section in `.cargo/config.toml`.
4. Modern crates utilizing `getrandom` v0.3+ on custom or bare-metal targets require `--cfg getrandom_backend="custom"` to locate their entropy provider. Without this flag injected, `getrandom` fails to compile.
5. Single-package invocations like `cargo zkvm-test -p zkpow-sha256` succeeded solely because `zkpow-sha256` had no dependency path leading to `getrandom` v0.3.

#### 2.1.2 Rustup Toolchain Proxy Warnings
Every invocation under `+succinct` produces the following notice:
```
info: `cargo` is unavailable for the active toolchain
info: falling back to "/Users/tcrypt/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/cargo"
```
The upstream `succinct` toolchain distributes a patched `rustc` and LLVM toolset, but does not package a standalone `cargo` binary. Rustup's wrapper emits this fallback notice to stderr on every command, creating visual clutter and confusing users into suspecting a broken toolchain installation.

#### 2.1.3 Misleading Terminology in `cargo prove`
In the SP1 ecosystem, compiling guest binaries relies on `cargo prove build`. This naming is semantically inaccurate: the command does not generate proofs; it compiles Rust code into a RISC-V ELF binary. This conflation creates conceptual confusion for engineers new to zkVM development, who expect `prove` to initiate cryptographic proving.

---

## 3. Architecture and Unified CLI Taxonomy

We propose creating a unified Cargo binary named `cargo-zkvm`, invoked as `cargo zkvm <subcommand>`.

```
+-----------------------------------------------------------------------------+
|                               Developer Action                              |
|                             $ cargo zkvm <cmd>                              |
+-----------------------------------------------------------------------------+
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                            cargo-zkvm CLI Suite                             |
|                                                                             |
|   +-------------------+  +-------------------+  +-----------------------+   |
|   | cargo zkvm test   |  | cargo zkvm build  |  | cargo zkvm run        |   |
|   | Unit/integ tests  |  | Guest ELF build   |  | In-VM execution       |   |
|   | Trace profiling   |  | Replaces `prove`  |  | Cycle metering        |   |
|   | Cycle breakdown   |  | Metadata export   |  | Stdout / Stderr logs  |   |
|   +-------------------+  +-------------------+  +-----------------------+   |
+-----------------------------------------------------------------------------+
                                       |
                    Supervises Compiler & Runtime Harness
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                        Toolchain Encapsulation Layer                        |
|   1. Detects & validates rustup toolchains (succinct / risc0)               |
|   2. Sets RUSTC direct path (eliminates rustup fallback noise)              |
|   3. Injects RUSTFLAGS: lower-atomic, image-base, custom getrandom_backend  |
|   4. Directs build artifacts to isolated target/zkvm/ directory             |
+-----------------------------------------------------------------------------+
```

### 3.1 Subcommand Specifications

#### 3.1.1 `cargo zkvm test`
Replaces `cargo zkvm-test`. Executes unit tests, integration tests, and doc tests compiled for the zkVM target:
- Supports Nextest filter expressions (`-E 'package(zkpow-core) & test(mtp)'`).
- Collects cycle counts, instruction categories (Math, Memory, Control Flow, System), and hierarchical trace profiles (`--profile`).
- Runs test cases in parallel across a configurable worker pool (`--test-threads`).
- Supports dual-mode testing: running native host tests for rapid iteration (`--host`) or full validation across both environments (`--all-targets`).

#### 3.1.2 `cargo zkvm build`
Replaces `cargo prove build`. Compiles guest binaries into optimized zkVM ELF executables:
```bash
# Build guest binary in release mode
cargo zkvm build --release

# Build specific guest package
cargo zkvm build -p zkpow-guest --bin guest-prover
```
- Injects standard production compiler flags (stack top `0x78000000`, atomic lowering, abort on panic).
- Emits structured compilation metadata and ELF hashes to `<workspace>/target/zkvm/release/`.
- Validates binary size and warns if program sections exceed zkVM memory address limits.

#### 3.1.3 `cargo zkvm run`
Executes a compiled guest binary inside the zkVM without generating cryptographic proofs:
```bash
# Execute guest ELF with input arguments
cargo zkvm run -p zkpow-guest -- --input-path fixtures/block_100k.bin

# Run with cycle profiling enabled
cargo zkvm run -p zkpow-guest --profile
```
- Provides instant cycle count, gas estimation, memory footprint, and execution trace inspection.
- Streams guest runtime logs (`sp1_zkvm::io::print`) directly to terminal stdout.

#### 3.1.4 `cargo zkvm prove`
Generates zero-knowledge proofs for production verification:
```bash
# Generate STARK proof locally
cargo zkvm prove -p zkpow-guest --prover cpu

# Generate Groth16 / PLONK proof via Succinct Prover Network
cargo zkvm prove -p zkpow-guest --prover network --network-key $SP1_PROVER_KEY
```

> **Why & What - Unified `cargo-zkvm` vs Separate Standalone Tools**
>
> **What:** Combine testing, building, running, and proving into a single `cargo-zkvm` binary with subcommands, rather than maintaining independent tools (`cargo-prove`, `cargo-zkvm-test`, custom runners).
>
> **Why:** Developers currently navigate multiple disjoint tools with differing argument conventions, duplicate dependencies, and conflicting configuration expectations. A single binary reduces toolchain installation to a single step, provides a cohesive mental model, and centralizes flag injection and toolchain discovery.
>
> **Alternatives considered:**
> - *Keep `cargo zkvm-test` separate from `cargo-prove`:* Downstream users must install and update two heavy CLI binaries that duplicate SP1 runtime dependencies.
> - *Rebrand under `cargo-prove`:* "Prove" implies cryptographic proof generation; using it for building or testing is confusing for developers unfamiliar with the ecosystem.
>
> **Fallback:** `cargo-zkvm` can provide backward-compatible symlinks or aliases for `cargo zkvm-test` and `cargo-prove`.

---

## 4. Toolchain Encapsulation and Flag Injection

### 4.1 Eliminating the `cargo +succinct` Requirement

Running commands with `cargo +succinct` exposes developers to internal rustup mechanics and generates confusing fallback warnings. 

`cargo-zkvm` encapsulates the toolchain entirely:
1. **Toolchain Discovery:** The CLI queries `rustup which --toolchain succinct rustc` to obtain the direct filesystem path to the patched compiler.
2. **Subprocess Invocation:** Rather than executing `cargo +succinct`, `cargo-zkvm` executes the host's standard `cargo` binary with explicit environment variables:
   - `RUSTC=<path-to-succinct-rustc>`
   - `RUSTUP_TOOLCHAIN=succinct` (if needed by build scripts)
3. **Outcome:** Host `cargo` drives compilation using the Succinct compiler without invoking the rustup toolchain proxy. The `info: cargo is unavailable` warning is completely eliminated.
4. **Actionable Diagnostics:** If the toolchain is not found, `cargo-zkvm` exits with explicit remediation:
   ```
   error: required toolchain 'succinct' is not installed.
   info:  to install the SP1 toolchain, run:
            curl -L https://sp1.succinct.xyz | bash
            sp1up
   ```

### 4.2 Automated Flag Injection via `CARGO_ENCODED_RUSTFLAGS`

To eliminate the need for `.cargo/config.toml` rustflags, `cargo-zkvm` constructs and injects required flags at process launch.

The supervisor defines standard flags matching `sp1-build`:
```rust
let image_base = 0x78000000; // 2013265920 = SP1 STACK_TOP
let zkvm_flags = [
    "-C", "passes=lower-atomic",
    "-C", &format!("link-arg=--image-base={image_base}"),
    "-C", "panic=abort",
    "--cfg", "getrandom_backend=\"custom\"",
    "-C", "llvm-args=-misched-prera-direction=bottomup",
    "-C", "llvm-args=-misched-postra-direction=bottomup",
];
```

The injection algorithm:
1. Reads existing flags from `RUSTFLAGS` or Cargo config files.
2. Merges project-specific flags with `zkvm_flags`, avoiding duplicate arguments.
3. Encodes the result using unit separators (`\x1f`) into `CARGO_ENCODED_RUSTFLAGS`.
4. Spawns the compilation step with the injected environment.

This guarantees that `--cfg getrandom_backend="custom"` and stack base definitions are always present during zkVM compilation, regardless of target architecture or project configuration.

### 4.3 Target Architecture Resolution

SP1 supports both 32-bit and 64-bit RISC-V targets (`riscv32im-succinct-zkvm-elf` and `riscv64im-succinct-zkvm-elf`).

`cargo-zkvm` handles target resolution using a clear precedence hierarchy:
1. **Explicit CLI flag:** `--target <triple>` takes highest precedence.
2. **Crate metadata:** Inspects `Cargo.toml` for package-level target overrides:
   ```toml
   [package.metadata.zkvm]
   target = "riscv32im-succinct-zkvm-elf"
   ```
3. **Workspace configuration:** Inspects workspace-level `[workspace.metadata.zkvm]`.
4. **Default fallback:** Defaults to `riscv64im-succinct-zkvm-elf` for modern SP1 projects, with automatic fallback detection if a crate specifies 32-bit assembly constraints.

---

## 5. Workspace Package Discovery and Filtering

### 5.1 Preventing Non-Guest Compilation Failures

When a user runs `cargo zkvm test` or `cargo zkvm build` from a workspace root without specifying `-p`, compiling every crate for a zkVM target will fail if host-only crates (such as CLI tools, native storage adapters, or web servers) contain platform-incompatible dependencies.

`cargo-zkvm` implements intelligent package classification:

```
+-----------------------------------------------------------------------------+
|                          Workspace Package Discovery                        |
+-----------------------------------------------------------------------------+
                                       |
                                       v
         +-----------------------------------------------------------+
         | For each package in workspace:                            |
         | 1. Does package depend on `zkvm-test` or `sp1-zkvm`?      |
         | 2. Does `[package.metadata.zkvm]` specify `enabled = true`?|
         | 3. Does `[package.metadata.zkvm]` specify `target`?       |
         +-----------------------------------------------------------+
                        /                             \
                       /                               \
               YES (Guest Compatible)            NO (Host Only)
                     /                                   \
                    v                                     v
     +------------------------------+     +-------------------------------+
     | Added to zkVM compile list   |     | Excluded from zkVM target     |
     | Target: riscv*-succinct-zkvm |     | Included only if --all-targets|
     +------------------------------+     | or --host is active           |
                                          +-------------------------------+
```

### 5.2 Discovery Rules

1. **Direct Dependency Detection:** Any package depending on `zkvm-test`, `zkvm-test-macros`, `sp1-zkvm`, or `risc0-zkvm` is classified as a zkVM package.
2. **Metadata Opt-In:** Packages can explicitly declare zkVM status:
   ```toml
   [package.metadata.zkvm]
   enable = true
   ```
3. **Metadata Opt-Out:** Packages can explicitly exclude themselves:
   ```toml
   [package.metadata.zkvm]
   enable = false
   ```
4. **Workspace-Level Selection:** The root `Cargo.toml` can declare the default test package list:
   ```toml
   [workspace.metadata.zkvm]
   packages = ["zkpow-core", "zkpow-sha256", "zkpow-types"]
   exclude = ["zkpow-host", "zkpow-cli"]
   ```

Running `cargo zkvm test` without flags will compile and execute tests *only* for verified zkVM packages, preventing unnecessary build failures in host crates.

---

## 6. Global Distribution and Installation

### 6.1 Installation Workflow

Developers install `cargo-zkvm` as a standard global host utility:

```bash
# Standard installation via crates.io
cargo install cargo-zkvm --locked

# Fast pre-compiled binary installation via cargo-binstall
cargo binstall cargo-zkvm

# Upstream SP1 bundling (proposed RFC to Succinct)
sp1up
```

### 6.2 Pre-compiled Release Binaries

To minimize compilation time for users, pre-compiled binaries are published on GitHub Releases for major development platforms:
- `x86_64-apple-darwin` (Intel macOS)
- `aarch64-apple-darwin` (Apple Silicon macOS)
- `x86_64-unknown-linux-gnu` (x86 Linux)
- `aarch64-unknown-linux-gnu` (ARM64 Linux)

Using `cargo-binstall cargo-zkvm` allows developers to set up the tool in seconds without compiling heavy SP1 VM dependencies locally.

---

## 7. Downstream Repository Integration

### 7.1 Zero-Configuration Setup

Under this proposal, a new repository adopting `cargo-zkvm` requires no configuration in `.cargo/config.toml`. 

The developer experience consists of:
1. Adding `zkvm-test` to the crate's `Cargo.toml`:
   ```toml
   [dev-dependencies]
   zkvm-test = "0.1"
   ```
2. Writing standard tests annotated with `#[zkvm_test]`:
   ```rust
   #[zkvm_test]
   fn test_header_validation() {
       assert!(verify_header(&fixture()));
   }
   ```
3. Executing tests:
   ```bash
   cargo zkvm test
   ```

### 7.2 Migration from Legacy `.cargo/config.toml`

Existing repositories with custom runners and compiler flags in `.cargo/config.toml` can cleanly remove them:

```diff
- [target.riscv32im-succinct-zkvm-elf]
- runner = ["cargo-zkvm-test"]
- rustflags = [
-   "-C", "passes=lower-atomic",
-   "-C", "link-arg=--image-base=2013265920",
-   "-C", "panic=abort",
-   "--cfg", 'getrandom_backend="custom"',
- ]
```

`cargo-zkvm` detects existing `.cargo/config.toml` entries and ensures injected flags do not conflict or duplicate repository-defined options.

---

## 8. Open Questions

| Question | Owner | Impact |
| :--- | :--- | :--- |
| **Q1:** What is the ideal binary name? `cargo-zkvm` is straightforward and aligns with `cargo-binstall` and `cargo-nextest`. Would an alternative (e.g. `cargo-polyzk`, `cargo-zksuite`, or extending `cargo-prove`) be preferred? | Tyler (tcrypt) | Ecosystem naming and user recognition |
| **Q2:** How should toolchain version mismatches be reported if a user updates their local Rust toolchain while keeping an older SP1 SDK? | Tooling Maintainers | Providing actionable warnings when compiler and SDK versions diverge |
| **Q3:** Should `cargo zkvm` support multi-zkVM backends (SP1 and RISC Zero) in a single binary, or provide modular cargo features? | Core Tooling | Binary download size and dependency footprints |

---

## 9. Benefits and Costs

### 9.1 Benefits
- **Zero Configuration:** Eliminates all `.cargo/config.toml` runner and rustflag boilerplate across repositories.
- **Unified Tooling:** Consolidates building (`build`), running (`run`), profiling (`test`), and proving (`prove`) under a single command suite.
- **Clean Terminal Output:** Silences rustup proxy warnings and isolates compiler artifacts.
- **Robust Package Discovery:** Automatically skips host-only crates during workspace runs, preventing `getrandom` and target-mismatch compilation errors.
- **Accurate Semantics:** Distinguishes between compiling guest binaries (`cargo zkvm build`) and generating cryptographic proofs (`cargo zkvm prove`).

### 9.2 Costs and Trade-offs
- **Binary Footprint:** A combined binary embedding the SP1 executor and profiling machinery is ~100MB unstripped. *Mitigation: Distribute stripped release binaries via GitHub Releases and `cargo-binstall`.*
- **Maintenance Alignment:** Compiler flags injected by the tool must track updates to `sp1-build` as SP1 evolves.

---

## 10. Residual Risks and What NOT to Do

1. **DO NOT rely on shell scripts or Cargo aliases for toolchain dispatch:** Cargo aliases cannot invoke rustup toolchains cleanly and break cross-platform support.
2. **DO NOT enforce zkVM compilation on host-only workspace packages:** Blindly passing a zkVM target triple to every package in a workspace causes compilation failures on crates utilizing native OS system calls. Always classify packages prior to compilation.
3. **DO NOT force standard `cargo test` into the zkVM:** Preserving native host testing (`cfg(not(target_os = "zkvm"))`) maintains sub-second test iteration for routine development.

---

*This document is a Request for Comments. Technical findings regarding `getrandom` compilation requirements, rustflags injection, and toolchain discovery are verified against `sp1-build` and `crates/zkvm-test-runner`.*
