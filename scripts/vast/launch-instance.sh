#!/usr/bin/env bash
# Search vast.ai offers, or launch a self-contained zkpow-prover instance.
#
# Credentials from scripts/vast/env.vast are pushed to Vast.ai account env
# vars (injected into the instance); non-sensitive config goes via --env.
# The container starts itself: pulls headers.db, runs zkpow-host.
#
# Usage:
#   ./scripts/vast/launch-instance.sh              # search RTX 4090 offers
#   ./scripts/vast/launch-instance.sh <offer_id>   # launch
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_IMAGE="tcrypt/zkpow-prover:latest"
DISK_GB=60

command -v vastai >/dev/null || { echo "install vastai cli: pip install vastai" >&2; exit 1; }

if [ $# -eq 0 ]; then
    vastai search offers 'gpu_name=RTX_4090 num_gpus=1 verified=true inet_down>200 disk_space>50' -o 'dph'
    exit 0
fi

OFFER_ID="$1"
[[ "${OFFER_ID}" =~ ^[0-9]+$ ]] || { echo "offer id must be numeric" >&2; exit 1; }

# shellcheck disable=SC1091
if [ -f "${SCRIPT_DIR}/env.vast" ]; then
    set -a; source "${SCRIPT_DIR}/env.vast"; set +a
fi

for var in AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_ENDPOINT_URL AWS_DEFAULT_REGION S3_DEST ZKPOW_DB_S3_URI SSH_DEST; do
    val="${!var:-}"
    [ -z "${val}" ] && continue
    vastai create env-var "${var}" "${val}" >/dev/null 2>&1 \
        || vastai update env-var "${var}" "${val}" >/dev/null 2>&1 \
        || echo "[-] failed to set ${var}" >&2
done

ENV_ARGS="-e ZKPOW_PROVER_BACKEND=${ZKPOW_PROVER_BACKEND:-cuda} -e RUST_LOG=info"
[ -n "${ZKPOW_RUN_ID:-}" ] && ENV_ARGS+=" -e ZKPOW_RUN_ID=${ZKPOW_RUN_ID}"

RESULT=$(vastai create instance "${OFFER_ID}" \
    --image "${DOCKER_IMAGE}" --disk "${DISK_GB}" \
    --ssh --direct --env "${ENV_ARGS}" --raw)
echo "${RESULT}"

INSTANCE_ID=$(echo "${RESULT}" | jq -r '.new_contract // empty' 2>/dev/null \
    || echo "${RESULT}" | grep -o '"new_contract": *[0-9]*' | awk -F': ' '{print $2}')
echo "[+] instance ${INSTANCE_ID:-?} — auto-starts proving. Debug: vastai ssh-url ${INSTANCE_ID:-<id>}"
