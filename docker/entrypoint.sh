#!/usr/bin/env bash
set -euo pipefail

export ZKPOW_DATA_DIR="${ZKPOW_DATA_DIR:-/data}"
export ZKPOW_DB_PATH="${ZKPOW_DB_PATH:-${ZKPOW_DATA_DIR}/headers.db}"
export ZKPOW_PROOF_STORE_DIR="${ZKPOW_PROOF_STORE_DIR:-${ZKPOW_DATA_DIR}/proof_store}"
export ZKPOW_RUN_ID="${ZKPOW_RUN_ID:-chain-$(date -u +%Y%m%dT%H%M%SZ)}"

mkdir -p "${ZKPOW_PROOF_STORE_DIR}"
command -v nvidia-smi >/dev/null && nvidia-smi || echo "[!] no GPU detected" >&2

DB_TAR="${ZKPOW_DATA_DIR}/headers.db.tar.zst"
DB_URI="${ZKPOW_DB_S3_URI:-${S3_DEST:+$S3_DEST/headers.db.tar.zst}}"

if [ ! -f "${ZKPOW_DB_PATH}" ] && [ -n "${DB_URI}" ]; then
    echo "==> Pulling ${DB_URI}"
    aws s3 cp "${DB_URI}" "${DB_TAR}" --no-progress ${AWS_ENDPOINT_URL:+--endpoint-url "${AWS_ENDPOINT_URL}"}
    zstd -d -c "${DB_TAR}" | tar -x -C "${ZKPOW_DATA_DIR}"
fi

if [ ! -f "${ZKPOW_DB_PATH}" ]; then
    echo "[-] no headers.db; set ZKPOW_DB_S3_URI or S3_DEST, or mount it at ${ZKPOW_DB_PATH}" >&2
    exit 1
fi

exec supervisord -n -c /etc/supervisor/supervisord.conf
