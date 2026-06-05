#!/bin/bash
set -euo pipefail
# Supervisor launch script for continuous-prover.
# Sourced environment from /etc/environment and /workspace/.env (via vast-base-image utils).

utils=/opt/supervisor-scripts/utils
. "${utils}/logging.sh"
. "${utils}/cleanup_generic.sh"
. "${utils}/environment.sh"

ZKPOW_DB_PATH="${ZKPOW_DB_PATH:-/app/headers.db}"

# Wait for headers.db to be rsync'd by the deploy script before starting.
# Without this the prover exits immediately and supervisor's startretries exhaust.
echo "Waiting for headers database at ${ZKPOW_DB_PATH} ..."
waited=0
while [[ ! -f "${ZKPOW_DB_PATH}" ]]; do
    sleep 10
    waited=$((waited + 10))
    echo "  still waiting for ${ZKPOW_DB_PATH} (${waited}s elapsed) ..."
    if [[ $waited -ge 1800 ]]; then
        echo "ERROR: headers.db not found after 30 min. Aborting."
        exit 1
    fi
done

echo "Headers DB found ($(du -sh "${ZKPOW_DB_PATH}" | cut -f1)). Starting continuous-prover."
echo "  ZKPOW_USE_CUDA=${ZKPOW_USE_CUDA:-0}  ZKPOW_BATCH_SIZE=${ZKPOW_BATCH_SIZE:-100}  ZKPOW_CUDA_DEVICE_ID=${ZKPOW_CUDA_DEVICE_ID:-0}"
echo "  Outputs → /workspace/profiling/  Logs → /workspace/logs/run.jsonl"

cd /workspace
exec /usr/local/bin/continuous-prover
