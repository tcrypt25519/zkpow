# zkpow-prover image

Builds `zkpow-host` into the image. On start, `entrypoint.sh` pulls
`headers.db.tar.zst` from S3, extracts it to `/data`, and execs supervisord,
which runs `zkpow-host` (restarts on crash) and a 30s `aws s3 sync` loop of
`/data/proof_store` when `S3_DEST` is set.

## Build

Requires buildx. `make docker` builds the image and records a git-ignored
`.docker-image.stamp`; it rebuilds only when the Dockerfile, entrypoint,
supervisord config, manifests, or crate sources change.

```bash
# production (amd64 + CUDA)
make docker

# local test on arm64 Mac (no CUDA)
make docker DOCKER_PLATFORM=linux/arm64 DOCKER_FEATURES= DOCKER_IMAGE=zkpow-prover:test
```

## Deploy (Vast.ai)

`scripts/vast/launch-instance.sh <offer_id>` pushes the credentials in
`scripts/vast/env.vast` (`AWS_*`, `S3_DEST`, …) to Vast.ai account env vars,
which Vast injects into the instance, and sets the non-sensitive `ZKPOW_*`
vars via `--env`. `scripts/vast/env.vast` is dockerignored and gitignored —
never bake it into the image.

## Env vars

| Var | Default | Purpose |
|---|---|---|
| `ZKPOW_DB_S3_URI` | `$S3_DEST/headers.db.tar.zst` | S3 URI of the DB archive |
| `S3_DEST` | unset | Proof upload destination; enables proof-sync |
| `ZKPOW_DB_PATH` | `/data/headers.db` | Local DB path |
| `ZKPOW_PROOF_STORE_DIR` | `/data/proof_store` | Proof output dir |
| `ZKPOW_RUN_ID` | `chain-<utc ts>` | Run identifier |
| `ZKPOW_BATCH_SIZE` / `ZKPOW_BATCH_COUNT` | binary defaults | Session shape |
| `ZKPOW_PROVER_BACKEND` | cuda (set at launch) | `cuda`/`cpu`/`mock` |
| `AWS_*` | unset | S3 creds/endpoint/region |

Mount a volume at `/data` to persist the DB and proofs across restarts; the
download is skipped if `headers.db` already exists.
