# docker/Dockerfile.vast-cuda

**Path:** `docker/Dockerfile.vast-cuda`
**Base Image(s):** `nvidia/cuda:12.4.1-devel-ubuntu22.04`, `platform`, `nvidia/cuda:12.4.1-runtime-ubuntu22.04`

## Build Stages

| Stage | Base Image |
|-------|-----------|
| `platform` | `nvidia/cuda:12.4.1-devel-ubuntu22.04` |
| `builder` | `platform` |
| *(final)* | `nvidia/cuda:12.4.1-runtime-ubuntu22.04` |

## Build Arguments

| Argument | Default |
|----------|---------|
| `RUST_VERSION` | `1.97.1` |
| `ZKPOW_FEATURES` | `cuda` |

## Environment Variables

| Variable | Default |
|----------|---------|
| `DEBIAN_FRONTEND` | `noninteractive` |
| `RUSTUP_HOME` | `/usr/local/rustup` |
| `CARGO_HOME` | `/usr/local/cargo` |
| `SP1_HOME` | `/root/.sp1` |
| `CUDA_HOME` | `/usr/local/cuda` |
| `PATH` | `/usr/local/cargo/bin:/root/.sp1/bin:/usr/local/cuda/bin:$PATH` |
| `CARGO_TARGET_DIR` | `/workspace/target/docker-linux-cuda` |
| `CURL_HOME` | `/etc/curl` |
| `CARGO_NET_GIT_FETCH_WITH_CLI` | `true` |
| `DEBIAN_FRONTEND` | `noninteractive` |
| `LANG` | `C.UTF-8` |
| `LC_ALL` | `C.UTF-8` |
| `CUDA_HOME` | `/usr/local/cuda` |
| `PATH` | `/usr/local/cuda/bin:$PATH` |
| `LD_LIBRARY_PATH` | `/usr/local/cuda/lib64:${LD_LIBRARY_PATH:-}` |

## Volumes

- `/data`

**Working Directory:** `/workspace`

## Entry Point

**ENTRYPOINT:** `["/entrypoint.sh"]`

## File Copies

| Instruction | Source | Destination | From Stage |
|-------------|--------|-------------|------------|
| `COPY` | `<<EOF` | `${CURL_HOME}/.curlrc` | — |
| `COPY` | `Cargo.toml`, `Cargo.lock` | `./` | — |
| `COPY` | `crates` | `./crates` | — |
| `COPY` | `/usr/local/bin/zkpow-host` | `/usr/local/bin/zkpow-host` | `builder` |
| `COPY` | `docker/entrypoint.sh` | `/entrypoint.sh` | — |
| `COPY` | `docker/supervisord.conf` | `/etc/supervisor/supervisord.conf` | — |

## Unsupported Copied Sources

Shell extraction is not yet supported; these copied deployment scripts are reported as unsupported-source coverage notices.

| COPY Source | Resolved Source | Language |
|-------------|-----------------|----------|
| `docker/entrypoint.sh` | `docker/entrypoint.sh` | Shell |

## Labels

| Key | Value |
|-----|-------|
| `maintainer` | `tcrypt` |
| `description` | `Self-contained ZKPOW CUDA prover node (Vast.ai)` |

## Notes

Builds only the `zkpow-host` binary, with `ZKPOW_FEATURES=cuda` by default, on CUDA 12.4.1, and copies it into a runtime image. `docker/entrypoint.sh` defaults `ZKPOW_DATA_DIR` to `/data` and derives `ZKPOW_DB_PATH`, `ZKPOW_PROOF_STORE_DIR`, and a time-based `ZKPOW_RUN_ID`. If `headers.db` is missing it pulls `headers.db.tar.zst` from `ZKPOW_DB_S3_URI` or `S3_DEST`, and exits 1 if there is still no database. supervisord then runs `zkpow-host` and, when `S3_DEST` is set, a loop that syncs the proof store to S3 every 30 seconds.
