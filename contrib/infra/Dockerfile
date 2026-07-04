# syntax=docker/dockerfile:1
# Bitcoin Header Chain Prover — Production Image
# Optimized for Intel servers with AVX + GPU (CUDA) support
#
# Usage:
#   docker build -t bitcoin-header-chain:latest -f infra/Dockerfile .
#   docker run --gpus all --rm \
#     -v header-input:/input \
#     -v header-output:/output \
#     -e ZKPOW_USE_CUDA=1 \
#     -e ZKPOW_DB_PATH=/input/headers.db \
#     bitcoin-header-chain:latest

# =============================================================================
# Stage 1: CUDA Toolkit (for GPU proving)
# =============================================================================
FROM nvidia/cuda:12.4.1-devel-ubuntu22.04 AS cuda-toolkit

# =============================================================================
# Stage 2: Builder (Rust + SP1 toolchain)
# =============================================================================
FROM rust:latest AS builder

WORKDIR /app

# Install SP1 toolchain dependencies: protobuf, Go, and build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    clang \
    cmake \
    git \
    golang-go \
    protobuf-compiler \
    libprotobuf-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Install succinct toolchain for SP1
RUN curl -L https://sp1.succinct.xyz | bash && \
    /root/.sp1/bin/sp1up

# Install CUDA toolkit for GPU support
COPY --from=cuda-toolkit /usr/local/cuda /usr/local/cuda
ENV PATH=/usr/local/cuda/bin:/root/.sp1/bin:$PATH
ENV LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH:-''}"
#
# Enable AVX2 + FMA for Intel CPU optimization
ENV RUSTFLAGS="-C target-cpu=native -C target-feature=+avx2,+fma"

# Copy workspace manifests for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./
COPY crates/core/Cargo.toml ./crates/core/
COPY crates/guest/Cargo.toml ./crates/guest/
COPY crates/host/Cargo.toml ./crates/host/
COPY crates/memory-usage/ ./crates/memory-usage/

# Build dependencies first (layer caching)
RUN mkdir -p /app/crates/core/src /app/crates/guest/src /app/crates/host/src/bin && \
    printf '#![allow(unused)]\npub fn _stub() {}\n' > /app/crates/core/src/lib.rs && \
    echo 'fn main() {}' > /app/crates/guest/src/main.rs && \
    echo 'fn main() {}' > /app/crates/host/src/main.rs && \
    printf '#![allow(unused)]\n' > /app/crates/host/src/lib.rs && \
    echo 'fn main() {}' > /app/crates/host/src/bin/test_errors.rs && \
    echo 'fn main() {}' > /app/crates/host/src/bin/inspect_proof.rs && \
    echo 'fn main() {}' > /app/crates/host/src/bin/sp1_stress.rs

# Copy source code
COPY crates ./crates

# Build the host binary with CUDA support
# Note: sp1-build compiles the guest program to RISC-V ELF during this step
RUN cargo build --release --features CUDA -p zkpow-host

# Strip binary to reduce size
RUN strip /app/target/release/zkpow-host

# =============================================================================
# Stage 3: Minimal Runtime (Ubuntu to match builder's glibc)
# =============================================================================
FROM ubuntu:24.04 AS runtime

WORKDIR /app

# Install minimal runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy host binary and guest ELF from builder
COPY --from=builder /app/target/release/zkpow-host /app/
COPY --from=builder /app/target/elf-compilation/riscv64im-succinct-zkvm-elf/release/zkpow-guest /app/

# Create non-root user
RUN useradd -m -u 2000 -s /bin/bash appuser \
    && chown -R appuser:appuser /app

USER appuser

# Define volumes for input (SQLite DB) and output (proofs)
VOLUME ["/input", "/output"]

# Environment variables
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV ZKPOW_DB_PATH=/input/headers.db
ENV AWS_SHARED_CREDENTIALS_FILE=/input/.aws/credentials
ENV AWS_CONFIG_FILE=/input/.aws/config

# Health check (verifies binary is executable)
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD test -x /app/zkpow-host || exit 1

# Entrypoint script for flexible execution
ENTRYPOINT ["/app/zkpow-host"]
