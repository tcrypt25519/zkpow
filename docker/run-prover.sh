#!/usr/bin/env bash
set -uo pipefail

HOST_BIN=/usr/local/bin/zkpow-host
RETRY_DELAY=5
RETRIES=0
MAX_RETRIES=10

while true; do
    echo "==> $(date -u +%Y-%m-%dT%H:%M:%SZ) Launching zkpow-host (run: ${ZKPOW_RUN_ID})..."
    "${HOST_BIN}"
    EXIT_CODE=$?

    if [ ${EXIT_CODE} -eq 0 ]; then
        echo "[+] zkpow-host exited normally."
        exit 0
    fi

    RETRIES=$((RETRIES + 1))
    if [ ${RETRIES} -ge ${MAX_RETRIES} ]; then
        echo "[-] zkpow-host failed ${MAX_RETRIES} times; giving up (last exit ${EXIT_CODE})." >&2
        exit "${EXIT_CODE}"
    fi

    echo "[!] zkpow-host exited with code ${EXIT_CODE}; resuming from ledger in ${RETRY_DELAY}s (${RETRIES}/${MAX_RETRIES})..."
    sleep ${RETRY_DELAY}
done
