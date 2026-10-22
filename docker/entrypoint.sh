#!/usr/bin/env bash
set -euo pipefail

DATA_DIR="${ZKPOW_DATA_DIR:-/data}"
DB_PATH="${ZKPOW_DB_PATH:-${DATA_DIR}/headers.db}"
DB_ZST_PATH="${DATA_DIR}/headers.db.tar.zst"
DB_S3_URI="${ZKPOW_DB_S3_URI:-${S3_DEST:+$S3_DEST/headers.db.tar.zst}}"

mkdir -p "${DATA_DIR}" "${ZKPOW_PROOF_STORE_DIR:-${DATA_DIR}/proof_store}"

/usr/local/bin/vast-cuda-check || true

aws_args=()
[ -n "${AWS_ENDPOINT_URL:-}" ] && aws_args+=(--endpoint-url "${AWS_ENDPOINT_URL}")
[ -n "${AWS_DEFAULT_REGION:-}" ] && aws_args+=(--region "${AWS_DEFAULT_REGION}")

if [ ! -f "${DB_PATH}" ] && [ ! -f "${DB_ZST_PATH}" ] && [ -n "${DB_S3_URI}" ]; then
    echo "==> Fetching headers.db.tar.zst from ${DB_S3_URI}..."
    aws s3 cp "${DB_S3_URI}" "${DB_ZST_PATH}" --no-progress "${aws_args[@]}"
fi

if [ ! -f "${DB_PATH}" ] && [ -f "${DB_ZST_PATH}" ]; then
    echo "==> Decompressing headers.db..."
    zstd -d -c "${DB_ZST_PATH}" | tar -xf - -C "${DATA_DIR}"
    echo "[+] Extracted $(du -h "${DB_PATH}" | cut -f1) to ${DB_PATH}"
fi

if [ ! -f "${DB_PATH}" ]; then
    echo "[-] FATAL: headers.db not found at ${DB_PATH} and no source available." >&2
    echo "    Set ZKPOW_DB_S3_URI (or S3_DEST) to an S3 URI containing headers.db.tar.zst," >&2
    echo "    or mount a volume with headers.db at ${DATA_DIR}." >&2
    exit 1
fi

export ZKPOW_RUN_ID="${ZKPOW_RUN_ID:-chain-vast-$(date -u +%Y%m%dT%H%M%SZ)}"
echo "==> Run ID: ${ZKPOW_RUN_ID}"
echo "==> Starting supervisord..."
exec /usr/bin/supervisord -n -c /etc/supervisor/supervisord.conf
