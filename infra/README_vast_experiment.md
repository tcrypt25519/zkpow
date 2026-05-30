# zkpow — Vast.ai GPU Experiment

Run `continuous-prover` on a Vast.ai GPU instance and rsync proof outputs back locally.

## Quick-start

```bash
# 1. Fill in credentials and config
cp .env.example .env.vast
$EDITOR .env.vast
source .env.vast

# 2. Build image + launch instance + upload headers.db (~30-60 min first run)
./scripts/vast_deploy.sh

# 3. In another terminal — watch progress
./scripts/vast_monitor.sh

# 4. In a third terminal — pull proof files as they appear
./scripts/pull_outputs_loop.sh

# 5. When done — DESTROY the instance to stop charges
vastai destroy instance <INSTANCE_ID> -y
```

---

## Required env vars

| Variable | Description |
|----------|-------------|
| `VAST_API_KEY` | Vast.ai API key — https://console.vast.ai/manage-keys/ |
| `DOCKER_IMAGE` | Full image name to push, e.g. `docker.io/user/zkpow-prover:latest` |
| `SSH_KEY_PATH` | SSH private key path (default `~/.ssh/id_ed25519`) |
| `HEADERS_DB` | Local path to `headers.db` (default `./headers.db`) |

All other vars have defaults (see `.env.example`).

---

## Commands

### Build and deploy
```bash
source .env.vast
./scripts/vast_deploy.sh
```
- Builds `Dockerfile.vast` and pushes to `$DOCKER_IMAGE`
- Searches Vast.ai for the cheapest matching GPU offer
- Creates an instance with CUDA, NUM_HEADERS, etc. wired in as env vars
- Rsyncs `headers.db` to `/app/headers.db` on the instance
- The prover starts automatically once the DB is there

To rebuild the image but reuse a running instance:
```bash
SKIP_BUILD=0 ./scripts/vast_deploy.sh   # full rebuild
```

To skip the build entirely (image already pushed, just want to (re)launch):
```bash
SKIP_BUILD=1 ./scripts/vast_deploy.sh
```

### Monitor
```bash
source .env.vast
./scripts/vast_monitor.sh
```
Shows: supervisor status, GPU utilisation, disk usage, proof file list, last 30 log lines.

SSH directly:
```bash
source .vast_instance   # exports SSH_HOST, SSH_PORT, SSH_KEY_PATH
ssh -p $SSH_PORT -i $SSH_KEY_PATH root@$SSH_HOST
```

Tail the structured JSON log live:
```bash
ssh -p $SSH_PORT -i $SSH_KEY_PATH root@$SSH_HOST 'tail -f /workspace/logs/run.jsonl'
```

### Pull outputs
```bash
source .env.vast
./scripts/pull_outputs_loop.sh
```
Rsyncs `/workspace/profiling/` (proof `.bin` files) and optionally `/workspace/logs/`
to `./remote_outputs/` every `$PULL_INTERVAL` seconds (default 120).

One-shot pull (no loop):
```bash
source .vast_instance
rsync -avz -e "ssh -p $SSH_PORT -i $SSH_KEY_PATH" \
    root@$SSH_HOST:/workspace/profiling/ ./remote_outputs/profiling/
```

### What happens when the prover crashes

The container does **not** die. `sshd` is the container anchor (PID 1's `wait` target).
The prover restart loop automatically:
1. Finds the latest `*.bin` proof on disk and sets `PREV_PROOF` to it (so you don't lose prior batches)
2. Restarts `continuous-prover` after a 60s cooldown

```bash
source .vast_instance

# SSH in and check logs
ssh -p $SSH_PORT -i $SSH_KEY_PATH root@$SSH_HOST \
  'tail -50 /workspace/logs/run.jsonl | python3 -c "
import sys, json
for l in sys.stdin:
    l = l.strip()
    if l:
        obj = json.loads(l)
        print(obj.get(\"level\",\"\"), obj.get(\"fields\",{}).get(\"message\",\"\"))
"'

# Stop the restart loop if you want to investigate manually
ssh -p $SSH_PORT -i $SSH_KEY_PATH root@$SSH_HOST 'kill $(pgrep -f continuous-prover) 2>/dev/null; kill %1 2>/dev/null; echo done'
```

### Destroy instance (stops all billing)
```bash
vastai destroy instance <INSTANCE_ID> -y
```
> ⚠️ This is permanent and irreversible. Pull all outputs you want first.

---

## How it works

```
Local machine
  vast_deploy.sh
    │  docker build Dockerfile.vast  (Stage 1: cuda dev libs, Stage 2: Rust build, Stage 3: cuda runtime)
    │  docker push $DOCKER_IMAGE
    │  vastai create instance ...
    │  rsync headers.db → /app/headers.db on instance
    └──────────────────────────────────────────────────────

Vast.ai instance (nvidia/cuda runtime + openssh-server)
  /entrypoint.sh
    ├── writes PUBLIC_KEY → /root/.ssh/authorized_keys  (Vast injects key as env var)
    ├── starts sshd
    ├── waits for /app/headers.db (rsync'd by deploy script)
    └── exec continuous-prover
          → /workspace/profiling/sp1/continuous/<ts>/batch_N/proofs/proof_height_X_to_Y.bin
          → /workspace/logs/run.jsonl
```

`continuous-prover` loops forever, proving batches of `NUM_HEADERS` headers each.
Each batch output is a new `proof_height_X_to_Y.bin` (compressed SP1 proof).

---

## Output paths (inside the container)

| Path | Contents |
|------|----------|
| `/workspace/profiling/sp1/continuous/<ts>/batch_N/proofs/` | Compressed proof `.bin` files |
| `/workspace/logs/run.jsonl` | Structured JSON log (one entry per line) |

After `pull_outputs_loop.sh`, same structure appears under `./remote_outputs/`.

---

## GPU requirements

The SP1 CUDA prover (`cuda_env.rs`) enforces:
- CUDA ≥ 12.5
- Compute capability ≥ 8.6 (RTX 3090, RTX 4090, A6000, etc.)
- ≥ 24 GB VRAM recommended

The default `GPU_QUERY` targets this. A100 (compute cap 8.0) does **not** meet the threshold.
To try an A100 anyway, override: `GPU_QUERY='num_gpus=1 gpu_ram>=40 reliability>0.90 rentable=true'`

---

## Known rough edges / assumptions

1. **First build is slow** — 30-60 min. SP1's dependency tree is large; Go is compiled for
   Gnark FFI; `sp1up` downloads the RISC-V toolchain. Subsequent builds use BuildKit cache mounts
   and are much faster.

2. **`rust-toolchain` is ignored by `.dockerignore`** — the image uses `rust:latest`. If the
   project needs exactly `1.95.0`, pin the builder stage: `FROM rust:1.85` or whichever version
   is stable at build time.

3. **`sp1up` fetches from the internet** — the build stage needs outbound access to
   `sp1.succinct.xyz`. Build fails if the CDN is down or the URL changes.

4. **`headers.db` is rsync'd separately** — it's 258 MB, excluded from the Docker build context,
   and expected at `/app/headers.db` at runtime (path is baked at compile time via
   `CARGO_MANIFEST_DIR`). The start script waits up to 30 min for it to appear.

5. **No auto-restart** — if the prover exits (batch failure), the container exits too and
   Vast.ai may or may not restart it depending on instance settings. SSH in and check
   `/workspace/logs/run.jsonl`, then `docker restart` or use `vastai reboot instance <id>`.

6. **Spot/interruptible instances** — by default `vast_deploy.sh` uses on-demand pricing.
   Add `--bid_price 0.20` to `vastai create instance` for interruptible (cheaper but can be
   preempted). If preempted, proof files on `/workspace` are lost unless pulled first.

7. **SSH key must be registered with Vast.ai** before creating an instance. `vast_deploy.sh`
   calls `vastai create ssh-key` automatically, but it may not take effect instantly.

8. **No PREV_PROOF on restart** — if the instance is destroyed and recreated, the next
   `continuous-prover` run starts from block 1 unless you pass `PREV_PROOF` pointing to a
   previously pulled proof file. Set it via supervisor env or re-deploy with an updated
   `--env` flag.
