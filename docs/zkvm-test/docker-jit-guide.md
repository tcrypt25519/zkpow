# SP1 Linux x86_64 JIT Docker Environment

This guide describes how to build and execute SP1 zkVM programs and test suites on Linux x86_64 utilizing SP1's native JIT engine from a macOS host.

---

## 1. Why Docker for SP1 JIT?

SP1's native executor (`sp1_use_native_executor` in `sp1-core-executor`) dynamically emits x86_64 machine code via `dynasmrt` and interacts with Linux kernel memory management primitives (`memfd_create`, direct page table protections).

In upstream SP1 (`sp1/crates/core/executor/src/build.rs`):
```rust
#[cfg(all(
    target_arch = "x86_64",
    target_endian = "little",
    target_os = "linux",
    not(feature = "profiling"),
    not(feature = "mprotect")
))]
println!("cargo:rustc-cfg=sp1_use_native_executor");
```

On macOS (both Apple Silicon ARM64 and macOS x86_64), SP1 cannot compile the native JIT engine and falls back to `sp1_use_portable_executor` (the software interpreter), which runs 5-10x slower.

By running an x86_64 Linux container (`--platform linux/amd64`) via Docker on macOS, the native JIT engine becomes fully active.

---

## 2. macOS Apple Silicon Prerequisites

When running on Apple Silicon (M1/M2/M3/M4):
1. Open **Docker Desktop Settings** (or your container runtime configuration, e.g. OrbStack).
2. Under **General -> Virtualization**, ensure **"Use Rosetta for x86/amd64 emulation on Apple Silicon"** is **checked**.
3. Rosetta 2 translates x86_64 machine instructions and JIT-compiled pages with near-native performance compared to software emulation.

---

## 3. Quick Start

### Build the Docker Image
```bash
./scripts/sp1-docker-jit.sh build
```
This builds `sp1-jit:latest` with Ubuntu 22.04 LTS, Rust stable & nightly, `sp1up`, `cargo-prove`, and the `succinct` RISC-V toolchains.

### Verify JIT Compatibility
```bash
./scripts/sp1-docker-jit.sh check
```
Output confirms target architecture (`x86_64 linux`), `/dev/shm` size, `memfd_create` support, and `cargo prove` availability.

### Open an Interactive Shell
```bash
./scripts/sp1-docker-jit.sh shell
```

### Run Builds and Tests
```bash
# Build SP1 guest program
./scripts/sp1-docker-jit.sh run cargo prove build -p zkpow-guest

# Run cargo tests inside container
./scripts/sp1-docker-jit.sh test

# Run zkvm-test runner with native JIT
./scripts/sp1-docker-jit.sh run cargo zkvm-test
```

---

## 4. Docker Compose Usage

You can also manage the container using Docker Compose:

```bash
# Build the service
docker compose -f docker/docker-compose.jit.yml build

# Start an interactive container
docker compose -f docker/docker-compose.jit.yml run --rm sp1-jit
```

---

## 5. Architectural & Execution Gotchas

### Shared Memory (`/dev/shm`)
SP1's gas estimation and trace chunk generation creates shared memory rings sized around 4.4 GiB per concurrent estimation thread.
- Standard Docker containers default to 64 MiB `/dev/shm`, which triggers `SIGBUS` or broken pipes.
- Both `scripts/sp1-docker-jit.sh` and `docker/docker-compose.jit.yml` automatically configure `--shm-size=8g`.

### Profiling Feature Flag Caveat
`sp1_use_native_executor` is automatically disabled if `sp1-core-executor` or `sp1-sdk` is compiled with:
- `feature = "profiling"`
- `feature = "mprotect"`

If either feature is active, SP1 falls back to `sp1_use_portable_executor`. To leverage the JIT, ensure cycle metrics are parsed via stdout markers rather than requiring the VM-level profiling feature flag.

### Target Directory Isolation
When mounting a macOS workspace into a Linux container, building into the same `target/` directory will invalidate host binaries or collide during concurrent builds.
The container sets `CARGO_TARGET_DIR=/workspace/target/docker-linux-x86_64` to maintain full isolation.
