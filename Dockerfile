ARG SP1_BUILD_PLATFORM=linux/amd64

FROM --platform=${SP1_BUILD_PLATFORM} ghcr.io/succinctlabs/sp1:v6.1.0 AS guest-builder

WORKDIR /root/program

COPY .cargo ./.cargo
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/root/program/target/elf-compilation/docker \
    export CARGO_TARGET_DIR=/root/program/target/elf-compilation/docker \
    && export RUSTUP_TOOLCHAIN=succinct \
    && export RUSTC_BOOTSTRAP=1 \
    && export RUSTC="$(rustc --print sysroot)/bin/rustc" \
    && export CARGO_ENCODED_RUSTFLAGS="$(printf '%s\037%s\037%s\037%s\037%s\037%s\037%s\037%s\037%s\037%s\037%s\037%s' \
        '-C' 'passes=lower-atomic' \
        '-C' 'link-arg=--image-base=2013265920' \
        '-C' 'panic=abort' \
        '--cfg' 'getrandom_backend="custom"' \
        '-C' 'llvm-args=-misched-prera-direction=bottomup' \
        '-C' 'llvm-args=-misched-postra-direction=bottomup')" \
    && export CFLAGS_riscv32im_succinct_zkvm_elf=-D__ILP32__ \
    && cargo build --release --target riscv64im-succinct-zkvm-elf --ignore-rust-version -p zkpow-guest \
    && cp /root/program/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/zkpow-guest /tmp/zkpow-guest

FROM golang:1.24.10-bookworm AS golang

FROM rust:1.95.0-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        git \
        libclang-dev \
        libprotobuf-dev \
        pkg-config \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY --from=golang /usr/local/go /usr/local/go

WORKDIR /app

ENV PATH=/usr/local/go/bin:${PATH}
ENV SP1_SKIP_PROGRAM_BUILD=true

COPY .cargo ./.cargo
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN mkdir -p /app/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release

COPY --from=guest-builder /tmp/zkpow-guest /app/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/zkpow-guest

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p zkpow-host --bin zkpow-host \
    && cp /app/target/release/zkpow-host /tmp/zkpow-host \
    && strip /tmp/zkpow-host

FROM debian:bookworm-slim AS layout

RUN mkdir -p /rootfs/db /rootfs/app /rootfs/usr/local/bin \
    && ln -s /db /rootfs/data

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

WORKDIR /app

COPY --from=layout --chown=65532:65532 /rootfs/ /
COPY --from=builder --chown=65532:65532 /tmp/zkpow-host /usr/local/bin/zkpow-host

ENV HEADERS_DB_PATH=/data/headers.db

VOLUME ["/db"]

ENTRYPOINT ["/usr/local/bin/zkpow-host"]
