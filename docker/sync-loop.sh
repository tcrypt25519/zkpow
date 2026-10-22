#!/usr/bin/env bash
set -uo pipefail

if [ -z "${S3_DEST:-}" ] && [ -z "${SSH_DEST:-}" ]; then
    echo "[*] No S3_DEST or SSH_DEST set; proof sync disabled. Exiting."
    exit 0
fi

SYNC=/opt/zkpow/scripts/vast/sync-proofs.sh
INTERVAL="${ZKPOW_SYNC_INTERVAL:-30}"

while true; do
    sleep "${INTERVAL}"
    "${SYNC}" || true
done
