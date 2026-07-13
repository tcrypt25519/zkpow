#!/usr/bin/env sh

set -euo pipefail

cargo build --release
RUST_LOG=info sp1_profiler=1 ./target/release/zkpow-host
