#!/usr/bin/env bash
# vast_deploy.sh — build, push, and launch a Vast.ai GPU instance for continuous-prover.
#
# Required env vars (copy .env.example → .env.vast and fill in):
#   VAST_API_KEY     Vast.ai API key (https://console.vast.ai/manage-keys/)
#   DOCKER_IMAGE     Full image name to push, e.g. docker.io/youruser/zkpow-prover:latest
#
# Optional env vars:
#   HEADERS_DB       Local path to headers.db (default: ./headers.db)
#   SSH_KEY_PATH     Local SSH private key for rsync (default: ~/.ssh/id_ed25519)
#   GPU_QUERY        Vast.ai offer filter (default targets RTX 3090/4090/A6000 class)
#   DISK_GB          Instance disk in GB (default: 80)
#   NUM_HEADERS      Headers per batch (default: 2016)
#   CUDA_DEVICE_ID   GPU device ID (default: 0)
#   RUST_LOG         Log level (default: info)
#   SKIP_BUILD       Set to 1 to skip docker build+push (image must already exist)

set -euo pipefail

# ---- Fail fast on missing required vars -------------------------------------
: "${VAST_API_KEY:?Set VAST_API_KEY to your Vast.ai API key}"
: "${DOCKER_IMAGE:?Set DOCKER_IMAGE to the full image name, e.g. docker.io/user/zkpow-prover:latest}"

HEADERS_DB="${HEADERS_DB:-./headers.db}"
SSH_KEY_PATH="${SSH_KEY_PATH:-$HOME/.ssh/id_ed25519}"
GPU_QUERY="${GPU_QUERY:-num_gpus=1 gpu_ram>=24 compute_cap>=860 reliability>0.90 rentable=true direct_port_count>=1}"
DISK_GB="${DISK_GB:-80}"
NUM_HEADERS="${NUM_HEADERS:-2016}"
CUDA_DEVICE_ID="${CUDA_DEVICE_ID:-0}"
RUST_LOG="${RUST_LOG:-info}"
SKIP_BUILD="${SKIP_BUILD:-0}"

# ---- Helpers ----------------------------------------------------------------
step() {
  echo
  echo "==> $*"
}
die() {
  echo "ERROR: $*" >&2
  exit 1
}
require() { command -v "$1" >/dev/null 2>&1 || die "Required tool not found: $1"; }

require docker
require vastai
require rsync
require jq

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ---- 1. Build + push image --------------------------------------------------
if [[ "${SKIP_BUILD}" == "1" ]]; then
  echo "SKIP_BUILD=1; skipping docker build+push"
else
  step "Building Docker image: ${DOCKER_IMAGE}"
  echo "  (First build takes 30-60 min due to SP1 + CUDA compilation)"
  DOCKER_BUILDKIT=1 docker build -f Dockerfile.vast -t "${DOCKER_IMAGE}" .

  step "Pushing ${DOCKER_IMAGE}"
  docker push "${DOCKER_IMAGE}"
fi

# ---- 2. Set Vast.ai API key -------------------------------------------------
step "Authenticating with Vast.ai"
vastai set api-key "${VAST_API_KEY}"

# ---- 3. Ensure SSH key is registered ----------------------------------------
step "Registering SSH key with Vast.ai"
if [[ ! -f "${SSH_KEY_PATH}.pub" ]]; then
  die "SSH public key not found: ${SSH_KEY_PATH}.pub  (set SSH_KEY_PATH)"
fi
# Add key (fails silently if already registered — that's fine)
vastai create ssh-key "${SSH_KEY_PATH}.pub" 2>/dev/null || true

# ---- 4. Find the cheapest matching GPU offer --------------------------------
step "Searching for GPU offer: ${GPU_QUERY}"
OFFER_JSON=$(vastai search offers "${GPU_QUERY}" -o 'dph_total' --raw 2>/dev/null)
OFFER_COUNT=$(echo "${OFFER_JSON}" | jq 'length')
if [[ "${OFFER_COUNT}" -eq 0 ]]; then
  die "No offers match '${GPU_QUERY}'. Try relaxing GPU_QUERY (e.g. remove compute_cap filter)."
fi
OFFER_ID=$(echo "${OFFER_JSON}" | jq -r '.[0].id')
OFFER_GPU=$(echo "${OFFER_JSON}" | jq -r '.[0].gpu_name')
OFFER_PRICE=$(echo "${OFFER_JSON}" | jq -r '.[0].dph_total')
echo "  Using offer ${OFFER_ID}: ${OFFER_GPU} @ \$${OFFER_PRICE}/hr"

# ---- 5. Create instance -----------------------------------------------------
step "Creating Vast.ai instance"
CREATE_RESULT=$(vastai create instance "${OFFER_ID}" \
  --image "${DOCKER_IMAGE}" \
  --disk "${DISK_GB}" \
  --ssh \
  --direct \
  --env "-e ZKPOW_USE_CUDA=1 -e ZKPOW_CUDA_DEVICE_ID=${CUDA_DEVICE_ID} -e ZKPOW_BATCH_SIZE=${NUM_HEADERS} -e RUST_LOG=${RUST_LOG} -e ZKPOW_GENERATE_GROTH16=${GENERATE_GROTH16}" \
  --raw)

INSTANCE_ID=$(echo "${CREATE_RESULT}" | jq -r '.new_contract // empty')
if [[ -z "${INSTANCE_ID}" ]]; then
  echo "Create response: ${CREATE_RESULT}"
  die "Failed to parse instance ID from create response"
fi
echo "  Instance created: ${INSTANCE_ID}"
echo "  INSTANCE_ID=${INSTANCE_ID}" >>.vast_instance # save for other scripts

# ---- 6. Poll until running --------------------------------------------------
step "Waiting for instance ${INSTANCE_ID} to reach 'running' state (may take a few minutes)"
for i in $(seq 1 60); do
  STATUS=$(vastai show instance "${INSTANCE_ID}" --raw | jq -r '.actual_status // "null"')
  echo "  [${i}/60] actual_status = ${STATUS}"
  if [[ "${STATUS}" == "running" ]]; then
    break
  fi
  # Bail out on terminal failure states to avoid wasting money
  if [[ "${STATUS}" == "exited" || "${STATUS}" == "offline" ]]; then
    die "Instance entered '${STATUS}' state. Check Vast.ai console and try a different offer."
  fi
  sleep 15
done

if [[ "${STATUS}" != "running" ]]; then
  die "Instance did not reach 'running' after 15 min. Check: vastai show instance ${INSTANCE_ID}"
fi

# ---- 7. Get SSH connection details ------------------------------------------
step "Getting SSH connection details"
INSTANCE_JSON=$(vastai show instance "${INSTANCE_ID}" --raw)
SSH_HOST=$(echo "${INSTANCE_JSON}" | jq -r '.ssh_host // .public_ipaddr')
SSH_PORT=$(echo "${INSTANCE_JSON}" | jq -r '.ssh_port // 22')
echo "  SSH: ssh -p ${SSH_PORT} -i ${SSH_KEY_PATH} root@${SSH_HOST}"
# Save for monitor + pull scripts
{
  echo "INSTANCE_ID=${INSTANCE_ID}"
  echo "SSH_HOST=${SSH_HOST}"
  echo "SSH_PORT=${SSH_PORT}"
  echo "SSH_KEY_PATH=${SSH_KEY_PATH}"
} >.vast_instance
chmod 600 .vast_instance

# ---- 8. Wait for SSH to accept connections ----------------------------------
step "Waiting for SSH to become available"
for i in $(seq 1 30); do
  if ssh -p "${SSH_PORT}" -i "${SSH_KEY_PATH}" \
    -o StrictHostKeyChecking=no -o ConnectTimeout=5 \
    root@"${SSH_HOST}" 'echo ok' 2>/dev/null; then
    break
  fi
  echo "  [${i}/30] SSH not ready yet, retrying in 10s..."
  sleep 10
done

# ---- 9. Upload headers.db ---------------------------------------------------
step "Uploading headers.db to /app/headers.db on the instance"
if [[ ! -f "${HEADERS_DB}" ]]; then
  die "headers.db not found at '${HEADERS_DB}'. Set HEADERS_DB= to its path."
fi
# DESTRUCTIVE: overwrites /app/headers.db on the remote instance
rsync -avz --progress \
  -e "ssh -p ${SSH_PORT} -i ${SSH_KEY_PATH} -o StrictHostKeyChecking=no" \
  "${HEADERS_DB}" \
  root@"${SSH_HOST}":/app/headers.db

step "Done"
echo
echo "  Instance ID : ${INSTANCE_ID}"
echo "  SSH         : ssh -p ${SSH_PORT} -i ${SSH_KEY_PATH} root@${SSH_HOST}"
echo "  GPU         : ${OFFER_GPU}"
echo "  Price       : \$${OFFER_PRICE}/hr"
echo
echo "The prover will start automatically once headers.db appears at /app/headers.db."
echo "Monitor with:   ./scripts/vast_monitor.sh"
echo "Pull outputs:   ./scripts/pull_outputs_loop.sh"
echo
echo "To destroy when done (stops all charges):"
echo "  vastai destroy instance ${INSTANCE_ID} -y"
