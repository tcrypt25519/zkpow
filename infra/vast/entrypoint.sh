#!/bin/bash
set -uo pipefail
# entrypoint.sh — container entrypoint for Vast.ai GPU instances.
#
# Responsibilities:
#   1. Write the Vast.ai-injected SSH public key so you can SSH in immediately.
#   2. Start sshd and keep it alive as the container anchor (container lives as long as sshd does).
#   3. Wait for headers.db to be rsync'd by vast_deploy.sh.
#   4. Run continuous-prover in a background restart loop.
#      On each restart, auto-recover PREV_PROOF from the latest proof file on disk.
#
# NOT using set -e: the container must survive prover crashes so you can SSH in to investigate.
# If the prover exits (exit 1 on batch failure), it restarts after a 60s cooldown.
# SSH in and run `kill %1` or `kill $(pgrep continuous-prover)` to stop the loop.
#
# Env vars (set by Vast.ai template or via --env at create time):
#   PUBLIC_KEY        SSH public key injected by Vast.ai
#   DB_PATH           Override headers.db location (default: /app/headers.db)
#   CUDA, CUDA_DEVICE_ID, NUM_HEADERS, RUST_LOG, GENERATE_GROTH16

# ---- 1. SSH key setup -------------------------------------------------------
mkdir -p /root/.ssh
chmod 700 /root/.ssh

if [[ -n "${PUBLIC_KEY:-}" ]]; then
    echo "${PUBLIC_KEY}" >> /root/.ssh/authorized_keys
    chmod 600 /root/.ssh/authorized_keys
    echo "[entrypoint] SSH public key written to /root/.ssh/authorized_keys"
else
    echo "[entrypoint] WARNING: PUBLIC_KEY not set — SSH will require a password or pre-existing key."
fi

# ---- 2. Start sshd (container anchor) ---------------------------------------
/usr/sbin/sshd -D &
SSHD_PID=$!
echo "[entrypoint] sshd started (PID ${SSHD_PID})"

# ---- 3. Wait for headers.db -------------------------------------------------
DB_PATH="${DB_PATH:-/app/headers.db}"
echo "[entrypoint] Waiting for headers database at ${DB_PATH} ..."
waited=0
while [[ ! -f "${DB_PATH}" ]]; do
    sleep 10
    waited=$((waited + 10))
    echo "[entrypoint]   still waiting (${waited}s) — rsync it with vast_deploy.sh"
    if [[ $waited -ge 1800 ]]; then
        echo "[entrypoint] ERROR: ${DB_PATH} not found after 30 min. Prover will not start."
        echo "[entrypoint] Container remains alive; SSH in and rsync the DB manually, then:"
        echo "[entrypoint]   /usr/local/bin/continuous-prover"
        # Don't exit — keep sshd alive so you can fix it over SSH
        wait $SSHD_PID
        exit 1
    fi
done
echo "[entrypoint] Headers DB ready ($(du -sh "${DB_PATH}" 2>/dev/null | cut -f1 || echo '?'))"

# ---- 4. Prover restart loop (runs in background) ----------------------------
(
    cd /workspace
    RUN=0
    while true; do
        RUN=$((RUN + 1))

        # On restart: find the latest successfully written compressed proof and resume from it.
        # This means a crash mid-batch loses only that batch, not all prior work.
        LATEST=$(find /workspace/profiling -name "*.bin" ! -name "*groth16*" 2>/dev/null | sort | tail -1)
        if [[ -n "$LATEST" ]]; then
            export PREV_PROOF="$LATEST"
            echo "[entrypoint] Run #${RUN}: resuming from ${PREV_PROOF}"
        else
            unset PREV_PROOF
            echo "[entrypoint] Run #${RUN}: starting from genesis (no prior proof found)"
        fi

        echo "[entrypoint]   CUDA=${CUDA:-0}  CUDA_DEVICE_ID=${CUDA_DEVICE_ID:-0}  NUM_HEADERS=${NUM_HEADERS:-100}"
        echo "[entrypoint]   Outputs → /workspace/profiling/   Logs → /workspace/logs/run.jsonl"

        /usr/local/bin/continuous-prover
        CODE=$?

        echo "[entrypoint] Run #${RUN} exited with code=${CODE} at $(date)"
        echo "[entrypoint] Check /workspace/logs/run.jsonl for details."
        echo "[entrypoint] SSH in and run 'kill \$(pgrep -f continuous-prover)' to stop the loop."
        echo "[entrypoint] Restarting in 60s ..."
        sleep 60
    done
) &

echo "[entrypoint] Prover loop running in background. Container anchored on sshd (PID ${SSHD_PID})."

# ---- Container stays alive until sshd exits ---------------------------------
wait $SSHD_PID
