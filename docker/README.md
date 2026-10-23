# zkpow-prover image

Self-contained proving node: `zkpow-host` is compiled at image-build time. On
container start the entrypoint pulls `headers.db.tar.zst` from S3-compatible
storage, decompresses it to `/data/headers.db`, then supervisord runs the
prover (`run-prover.sh`, crash-resume loop) and a proof-sync loop
(`sync-loop.sh` -> `scripts/vast/sync-proofs.sh`).

## Build

Buildx is required for cross-platform builds.

The workspace has a path dependency on the sibling repo `../zkvm-test`, which
lives outside the Docker build context. Pass it as a buildx named context with
`--build-context zkvm-test=<path-to-zkvm-test>` on every build.

```bash
# one-time builder setup (macOS / any host)
docker buildx create --name zkpow-builder --use 2>/dev/null || docker buildx use zkpow-builder
docker buildx inspect --bootstrap
```

### Production (linux/amd64, CUDA) — for Vast.ai / cloud GPU

From an arm64 Mac this cross-builds under QEMU; expect a long build.
Prefer building on an x86_64 CI runner if available.

```bash
docker buildx build --platform linux/amd64 \
  -f docker/Dockerfile.vast-cuda \
  --build-context zkvm-test=../zkvm-test \
  -t <registry>/zkpow-prover:latest \
  --push .
```

### Multi-arch (amd64 + arm64)

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -f docker/Dockerfile.vast-cuda \
  --build-context zkvm-test=../zkvm-test \
  -t <registry>/zkpow-prover:latest \
  --push .
```

Note: `cargo build --features cuda` on arm64 requires the arm64 CUDA devel
image; if the CUDA build fails on arm64, ship arm64 without CUDA:

```bash
docker buildx build --platform linux/arm64 \
  -f docker/Dockerfile.vast-cuda \
  --build-context zkvm-test=../zkvm-test \
  -t <registry>/zkpow-prover:arm64 \
  --build-arg ZKPOW_FEATURES="" --push .
```

### Local test build (native arch, no CUDA)

```bash
docker buildx build --platform linux/arm64 \
  -f docker/Dockerfile.vast-cuda \
  --build-context zkvm-test=../zkvm-test \
  -t zkpow-prover:test \
  --build-arg ZKPOW_FEATURES="" --load .
```

## Run

The image carries no credentials or workload config. Two channels supply env
vars at launch:

- **Sensitive vars** (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
  `AWS_ENDPOINT_URL`, `AWS_DEFAULT_REGION`, `S3_DEST`, `ZKPOW_DB_S3_URI`,
  `SSH_DEST`) — stored as Vast.ai account env vars by
  `scripts/vast/launch-instance.sh` (`vastai create/update env-var`); Vast.ai
  injects them into the instance automatically. Never bake them into the
  image; `scripts/vast/env.vast` is dockerignored.
- **Non-sensitive vars** (`ZKPOW_*`, `RUST_LOG`, …) — passed in the instance's
  docker settings (`vastai create instance --env '-e K=V ...'`).

For a plain `docker run` (local test, other clouds), pass everything with `-e`:

```bash
docker run --gpus all --shm-size=16g \
  -e S3_DEST=s3://zkpow-proofs/mainnet \
  -e AWS_ACCESS_KEY_ID=... \
  -e AWS_SECRET_ACCESS_KEY=... \
  -e AWS_ENDPOINT_URL=https://s3.us-west-004.backblazeb2.com \
  -e AWS_DEFAULT_REGION=us-west-004 \
  -v zkpow-data:/data \
  <registry>/zkpow-prover:latest
```

### Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `S3_DEST` | unset | Bucket/prefix for proof sync; also default DB source (`${S3_DEST}/headers.db.tar.zst`) |
| `ZKPOW_DB_S3_URI` | `${S3_DEST}/headers.db.tar.zst` | Explicit S3 URI of the DB archive; set this to pull the DB without enabling proof upload |
| `ZKPOW_DB_PATH` | `/data/headers.db` | Local DB path |
| `ZKPOW_PROOF_STORE_DIR` | `/data/proof_store` | Proof output dir |
| `ZKPOW_RUN_ID` | `chain-vast-<utc ts>` | Run identifier |
| `ZKPOW_BATCH_SIZE` / `ZKPOW_BATCH_COUNT` | 2016 / 500 | Proving session shape |
| `ZKPOW_PROVER_BACKEND` | cuda | `cuda`, `cpu`, or `mock` |
| `ZKPOW_SYNC_INTERVAL` | 30 | Seconds between proof syncs |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_ENDPOINT_URL` / `AWS_DEFAULT_REGION` | unset | S3 credentials (Backblaze B2 shown; any S3-compatible store works) |

If `/data/headers.db` already exists (persistent volume), the download is
skipped. Proof sync exits immediately when neither `S3_DEST` nor `SSH_DEST`
is set.
