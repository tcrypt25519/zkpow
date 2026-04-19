FROM golang:1.24.10-bookworm AS golang

FROM rust:1.95.0-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        curl \
        git \
        libclang-dev \
        libprotobuf-dev \
        pkg-config \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY --from=golang /usr/local/go /usr/local/go

WORKDIR /app

ENV SP1_BUILD_WITH_DOCKER=false
ENV PATH=/root/.sp1/bin:/usr/local/go/bin:${PATH}

RUN curl -L https://sp1up.succinct.xyz | bash \
    && /root/.sp1/bin/sp1up -v 6.1.0

COPY .cargo ./.cargo
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p zkpow-host --bin zkpow-host \
    && cp /app/target/release/zkpow-host /tmp/zkpow-host \
    && strip /tmp/zkpow-host

FROM debian:bookworm-slim AS layout

RUN mkdir -p /rootfs/db /rootfs/app /rootfs/usr/local/bin \
    && ln -s /db /rootfs/data

# The guest program is embedded into the host binary via include_elf!, so the
# runtime image only needs the host executable and the glibc/libstdc++ runtime.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

WORKDIR /app

COPY --from=layout --chown=65532:65532 /rootfs/ /
COPY --from=builder --chown=65532:65532 /tmp/zkpow-host /usr/local/bin/zkpow-host

ENV HEADERS_DB_PATH=/data/headers.db

VOLUME ["/db"]

ENTRYPOINT ["/usr/local/bin/zkpow-host"]
