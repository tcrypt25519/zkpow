#!/usr/bin/env bash
# =============================================================================
# Package headers.db with zstd compression for ultra-fast transfer to Vast.ai
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DB_PATH="${ROOT_DIR}/headers.db"
OUT_TAR="${ROOT_DIR}/headers.db.tar.zst"

if [ ! -f "${DB_PATH}" ]; then
    echo "[-] Error: headers.db not found at ${DB_PATH}" >&2
    exit 1
fi

echo "==> Packaging ${DB_PATH} ($(du -h "${DB_PATH}" | cut -f1)) with zstd..."

# First checkpoint SQLite WAL into main database
if command -v sqlite3 >/dev/null 2>&1; then
    echo "==> Checkpointing SQLite WAL..."
    sqlite3 "${DB_PATH}" "PRAGMA wal_checkpoint(TRUNCATE);"
fi

tar -cf - -C "${ROOT_DIR}" headers.db | zstd -T0 -3 -o "${OUT_TAR}"

echo "[+] Done! Compressed database: ${OUT_TAR} ($(du -h "${OUT_TAR}" | cut -f1))"
echo "    Transfer to Vast instance with:"
echo "      scp -P <port> ${OUT_TAR} root@<vast-ip>:/workspace/"
echo "    Or upload to S3/R2 and curl inside container."
