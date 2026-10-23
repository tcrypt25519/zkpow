#!/usr/bin/env bash
# =============================================================================
# Vast.ai Instance Launch Helper
#
# Searches for top verified NVIDIA RTX 4090 / A100 offers, or launches an
# instance running the self-contained zkpow-prover image. The container
# auto-starts: it pulls headers.db from S3 and begins proving with no SSH
# or manual setup required.
#
# Storage credentials (AWS_*/S3_*) from scripts/vast/env.vast are pushed to
# Vast.ai account env vars via `vastai create/update env-var` so they are
# injected into the instance at launch. Non-sensitive ZKPOW_* config goes
# into the instance's docker --env settings.
#
# Usage:
#   ./scripts/vast/launch-instance.sh              # search offers
#   ./scripts/vast/launch-instance.sh <offer_id>   # launch from offer
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DOCKER_IMAGE="tcrypt/zkpow-prover:latest"
DISK_GB=60

SENSITIVE_VARS=(
    AWS_ACCESS_KEY_ID
    AWS_SECRET_ACCESS_KEY
    AWS_ENDPOINT_URL
    AWS_DEFAULT_REGION
    S3_DEST
    ZKPOW_DB_S3_URI
    SSH_DEST
)

# Non-sensitive config injected via docker -e at launch
DOCKER_ENV_VARS=(
    "ZKPOW_DATA_DIR=/data"
    "ZKPOW_DB_PATH=/data/headers.db"
    "ZKPOW_PROOF_STORE_DIR=/data/proof_store"
    "ZKPOW_PROVER_BACKEND=${ZKPOW_PROVER_BACKEND:-cuda}"
    "ZKPOW_BATCH_SIZE=${ZKPOW_BATCH_SIZE:-2016}"
    "ZKPOW_BATCH_COUNT=${ZKPOW_BATCH_COUNT:-500}"
    "ZKPOW_MEMORY_LOGGING=1"
    "RUST_LOG=info"
    "RUST_BACKTRACE=1"
)

if ! command -v vastai >/dev/null 2>&1; then
    echo "[-] Error: vastai CLI is not installed or not in PATH." >&2
    echo "    Install with: pip install vastai" >&2
    exit 1
fi

if [ $# -eq 0 ] || [ "$1" = "--search" ] || [ "$1" = "-s" ]; then
    echo "================================================================================"
    echo " Top Available Verified RTX 4090 Offers on Vast.ai"
    echo "================================================================================"
    vastai search offers 'gpu_name=RTX_4090 num_gpus=1 verified=true inet_down>200 disk_space>50' -o 'dph'
    echo "================================================================================"
    echo "To launch an instance, choose an offer ID above and run:"
    echo "  $0 <offer_id>"
    echo "================================================================================"
    exit 0
fi

OFFER_ID="$1"
if ! [[ "${OFFER_ID}" =~ ^[0-9]+$ ]]; then
    echo "[-] Error: Offer ID must be a numeric ID. Got: ${OFFER_ID}" >&2
    echo "    Run '$0' with no arguments to search for offers." >&2
    exit 1
fi

# Load storage credentials from env.vast
if [ -f "${SCRIPT_DIR}/env.vast" ]; then
    # shellcheck disable=SC1091
    set -a; source "${SCRIPT_DIR}/env.vast"; set +a
else
    echo "[!] Warning: ${SCRIPT_DIR}/env.vast not found; skipping credential push." >&2
    echo "    Storage sync and DB pull will fail unless vars are already set on Vast.ai." >&2
fi

# Push sensitive vars to Vast.ai account env (injected into the instance)
echo "==> Upserting sensitive env vars on Vast.ai account..."
for var in "${SENSITIVE_VARS[@]}"; do
    val="${!var:-}"
    [ -z "${val}" ] && continue
    if vastai create env-var "${var}" "${val}" >/dev/null 2>&1; then
        echo "    [+] created ${var}"
    elif vastai update env-var "${var}" "${val}" >/dev/null 2>&1; then
        echo "    [~] updated ${var}"
    else
        echo "    [-] failed to set ${var}" >&2
    fi
done

ENV_ARGS=""
for kv in "${DOCKER_ENV_VARS[@]}"; do
    ENV_ARGS+="-e ${kv} "
done
# Optional run id passthrough
if [ -n "${ZKPOW_RUN_ID:-}" ]; then
    ENV_ARGS+="-e ZKPOW_RUN_ID=${ZKPOW_RUN_ID} "
done

echo "==> Creating Vast.ai instance from Offer #${OFFER_ID}..."
echo "    Image : ${DOCKER_IMAGE}"
echo "    Disk  : ${DISK_GB} GB"
echo "    Mode  : SSH Direct"
echo "    Env   : ${ENV_ARGS}"

RESULT=$(vastai create instance "${OFFER_ID}" \
    --image "${DOCKER_IMAGE}" \
    --disk "${DISK_GB}" \
    --ssh \
    --direct \
    --env "${ENV_ARGS}" \
    --raw)

echo "==> Vast.ai response: ${RESULT}"

INSTANCE_ID=$(echo "${RESULT}" | jq -r '.new_contract // empty' 2>/dev/null || true)
if [ -z "${INSTANCE_ID}" ]; then
    INSTANCE_ID=$(echo "${RESULT}" | grep -o '"new_contract": *[0-9]*' | awk -F': ' '{print $2}' || true)
fi

if [ -n "${INSTANCE_ID}" ]; then
    echo "================================================================================"
    echo "[+] Instance created successfully! Instance Contract ID: ${INSTANCE_ID}"
    echo "================================================================================"
    echo "The container starts automatically: it pulls headers.db from S3 and"
    echo "begins proving. No manual setup required."
    echo ""
    echo "1. Check provisioning status:"
    echo "     vastai show instances"
    echo ""
    echo "2. Once running, watch prover logs (SSH is for debug/monitoring only):"
    echo "     vastai ssh-url ${INSTANCE_ID}"
    echo "     ssh -p <port> root@<ip> 'docker logs -f \$(docker ps -q | head -1)'"
    echo "   Or inside the container: supervisorctl status; tail -f /var/log/supervisor/*"
    echo "================================================================================"
else
    echo "[!] Instance creation request sent. Check status with: vastai show instances"
fi
