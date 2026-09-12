# Building and Installing Rustdoc for the Succinct Toolchain

## Purpose

The official SP1 `succinct` toolchain packages only `rustc` and target standard libraries for `riscv64im-succinct-zkvm-elf`. It excludes `rustdoc` and `cargo`. When developers run standard Cargo commands like `cargo test` or `cargo +succinct test` without target-filtering flags, Cargo attempts to run documentation tests and fails with:

```text
error: 'rustdoc' is not installed for the custom toolchain 'succinct'.
note: test exited abnormally; to see the full output pass --no-capture to the harness.
```

Because `rustdoc` shares an unstable internal AST/metadata format (`.rmeta`) with the exact commit of `rustc`, an upstream rustup `rustdoc` binary cannot be substituted. This guide documents the verified procedure for building a compatible `rustdoc` directly from Succinct's compiler fork and installing it into the toolchain sysroot on Apple Silicon (`aarch64-apple-darwin`).

---

## Prerequisites

Ensure host development tools are available:
- `git`
- `python3`
- `cmake` (via Homebrew: `brew install cmake`)
- `ninja` (via Homebrew: `brew install ninja`)
- Apple Xcode Command Line Tools or Xcode (`clang`, `ld`)
- Active host Rust toolchain (`rustup`)

---

## 1. Clone Succinct's Compiler Fork

Clone the repository and check out the exact release tag used by the installed SP1 toolchain:

```bash
mkdir -p ~/code/succinct-rust
cd ~/code/succinct-rust
git init
git remote add origin https://github.com/succinctlabs/rust.git
# Fetch only the required tag shallowly
git fetch --depth=1 origin tag succinct-1.94.0-64bit
git checkout FETCH_HEAD
```

Verify the commit matches `c7149403db5f6f72f410d6dffcee90378235f23b`.

Initialize only the submodules needed to build the compiler and rustdoc:

```bash
git submodule update --init --recursive library/backtrace src/llvm-project
```

---

## 2. Configure Bootstrap Settings

Create `~/code/succinct-rust/bootstrap.toml`:

```toml
change-id = "ignore"

[build]
extended = true
tools = ["rustdoc"]
configure-args = []
cargo-native-static = true

[rust]
lld = true
llvm-tools = true

[llvm]
download-ci-llvm = false
# Critical: restrict targets to host and zkVM architectures.
# Omitting obsolete targets (such as Hexagon) avoids C++20 standard library
# template incompatibilities with modern Apple Clang SDKs.
targets = "AArch64;RISCV;X86"
```

---

## 3. Verify Compiler and Tool Buildability

Run `x.py check` to download bootstrap dependencies and type-check compiler crates and rustdoc:

```bash
cd ~/code/succinct-rust
./x.py check src/tools/rustdoc
```

This completes in ~1 minute 15 seconds with zero errors.

---

## 4. Build Rustdoc and Host Standard Library

Run the build steps:

```bash
cd ~/code/succinct-rust

# Build stage1 rustdoc tool binary and compiler drivers (~4.5 minutes on Apple Silicon)
./x.py build src/tools/rustdoc

# Build host standard library crates (core, alloc, std) (~11 seconds)
./x.py build library
```

The build produces:
- Compiler driver: `build/aarch64-apple-darwin/stage1/lib/librustc_driver-<hash>.dylib`
- Rustdoc executable: `build/aarch64-apple-darwin/stage1-tools/aarch64-apple-darwin/release/rustdoc_tool_binary`

Copy the built executable into the stage1 compiler sysroot:

```bash
cp build/aarch64-apple-darwin/stage1-tools/aarch64-apple-darwin/release/rustdoc_tool_binary \
   build/aarch64-apple-darwin/stage1/bin/rustdoc
chmod +x build/aarch64-apple-darwin/stage1/bin/rustdoc
```

Verify the binary:

```bash
./build/aarch64-apple-darwin/stage1/bin/rustdoc --version
# Output: rustdoc 1.94.0-dev
```

---

## 5. Install and Link Toolchain

### Option A: Register as `succinct-dev` Toolchain (Parallel Testing)

Link the newly built stage1 sysroot into `rustup`:

```bash
rustup toolchain link succinct-dev ~/code/succinct-rust/build/aarch64-apple-darwin/stage1
```

Verify rustup sees `rustdoc`:

```bash
rustup run succinct-dev rustdoc --version
# Output: rustdoc 1.94.0-dev
```

### Option B: Patch the Existing Installed `succinct` Toolchain

Locate the active `succinct` sysroot directory:

```bash
SYSROOT=$(rustc +succinct --print sysroot)
# Typical location: ~/.sp1/toolchains/<HASH>
```

Copy `rustdoc` directly into that directory's `bin/`:

```bash
cp ~/code/succinct-rust/build/aarch64-apple-darwin/stage1/bin/rustdoc "$SYSROOT/bin/rustdoc"
chmod +x "$SYSROOT/bin/rustdoc"
```

*Note: If `librustc_driver-<hash>.dylib` hashes match the existing `rustc`, the single binary copy suffices. If hashes differ between build releases, use Option A.*

---

## 6. Verification

Run the full test suite (including doctests) against `zkpow-core`:

```bash
cargo +succinct-dev test -v --package zkpow-core
```

Expected output:
```text
running 66 tests
...
test result: ok. 66 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests zkpow_core

running 1 test
test crates/core/src/halt/mod.rs - halt::invalid_hint (line 21) ... ignored

test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Exit status: `0`. Both unit/in-VM tests and doc-tests run to completion without abnormal termination.
