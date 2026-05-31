#!/usr/bin/env bash
# pull_outputs_loop.sh — repeatedly rsync proof outputs and logs from the instance.
#
# Reads connection details from .vast_instance (written by vast_deploy.sh)
# or from env vars:
#   SSH_HOST         SSH hostname
#   SSH_PORT         SSH port
#   SSH_KEY_PATH     Path to SSH private key (default: ~/.ssh/id_ed25519)
#
# Optional:
#   PULL_INTERVAL    Seconds between pulls (default: 120)
#   LOCAL_OUTPUT_DIR Local directory for pulled files (default: ./remote_outputs)
#   PULL_LOGS        Set to 1 to also pull logs/ (default: 0)

set -euo pipefail

# ---- Load connection details ------------------------------------------------
if [[ -f .vast_instance ]]; then
  # shellcheck disable=SC1091
  source .vast_instance
fi

: "${SSH_HOST:?SSH_HOST not set. Run vast_deploy.sh first or set manually.}"
: "${SSH_PORT:?SSH_PORT not set.}"
SSH_KEY_PATH="${SSH_KEY_PATH:-$HOME/.ssh/id_ed25519}"
PULL_INTERVAL="${PULL_INTERVAL:-120}"
LOCAL_OUTPUT_DIR="${LOCAL_OUTPUT_DIR:-./remote_outputs}"
PULL_LOGS="${PULL_LOGS:-0}"

RSYNC_OPTS="-avz --progress --ignore-existing"
RSYNC_SSH="-e ssh -p ${SSH_PORT} -i ${SSH_KEY_PATH} -o StrictHostKeyChecking=no"
REMOTE="root@${SSH_HOST}"

mkdir -p "${LOCAL_OUTPUT_DIR}/profiling"
[[ "${PULL_LOGS}" == "1" ]] && mkdir -p "${LOCAL_OUTPUT_DIR}/logs"

echo "Pulling outputs from ${SSH_HOST}:${SSH_PORT} → ${LOCAL_OUTPUT_DIR}/"
echo "  Interval: ${PULL_INTERVAL}s  (Ctrl+C to stop)"
echo

batch=0
while true; do
  batch=$((batch + 1))
  echo "[$(date '+%H:%M:%S')] Pull #${batch}"

  # Proof files — only pull new ones (--ignore-existing skips already-local files)
  rsync ${RSYNC_OPTS} ${RSYNC_SSH} \
    "${REMOTE}:/workspace/profiling/" \
    "${LOCAL_OUTPUT_DIR}/profiling/" \
    2>/dev/null &&
    echo "  profiling/: ok" ||
    echo "  profiling/: nothing new (or not created yet)"

  if [[ "${PULL_LOGS}" == "1" ]]; then
    # For logs, always pull (overwrite) so we get the latest run.jsonl
    rsync -avz ${RSYNC_SSH} \
      "${REMOTE}:/workspace/logs/" \
      "${LOCAL_OUTPUT_DIR}/logs/" \
      2>/dev/null &&
      echo "  logs/: ok" ||
      echo "  logs/: nothing yet"
  fi

  # Show what we have locally
  PROOF_COUNT=$(find "${LOCAL_OUTPUT_DIR}/profiling" -name '*.bin' 2>/dev/null | wc -l | tr -d ' ')
  TOTAL_SIZE=$(du -sh "${LOCAL_OUTPUT_DIR}" 2>/dev/null | cut -f1 || echo "?")
  echo "  Local: ${PROOF_COUNT} proof file(s), ${TOTAL_SIZE} total"

  echo "  Next pull in ${PULL_INTERVAL}s ..."
  sleep "${PULL_INTERVAL}"
done
