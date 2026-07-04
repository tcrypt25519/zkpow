#!/usr/bin/env bash
# vast_monitor.sh — show status of the running continuous-prover experiment.
#
# Reads connection details from .vast_instance (written by vast_deploy.sh)
# or from env vars:
#   INSTANCE_ID   Vast.ai instance ID
#   SSH_HOST      SSH hostname
#   SSH_PORT      SSH port
#   SSH_KEY_PATH  Path to SSH private key (default: ~/.ssh/id_ed25519)

set -euo pipefail

# ---- Load connection details ------------------------------------------------
if [[ -f .vast_instance ]]; then
  # shellcheck disable=SC1091
  source .vast_instance
fi

: "${SSH_HOST:?SSH_HOST not set. Run vast_deploy.sh first or set manually.}"
: "${SSH_PORT:?SSH_PORT not set.}"
SSH_KEY_PATH="${SSH_KEY_PATH:-$HOME/.ssh/id_ed25519}"

SSH_OPTS="-p ${SSH_PORT} -i ${SSH_KEY_PATH} -o StrictHostKeyChecking=no -o ConnectTimeout=10"
SSH="ssh ${SSH_OPTS} root@${SSH_HOST}"

echo "====================================================================="
echo "  zkpow continuous-prover — instance ${INSTANCE_ID:-unknown}"
echo "  ${SSH_HOST}:${SSH_PORT}"
echo "====================================================================="

# ---- Prover process status --------------------------------------------------
echo
echo "--- Process status (supervisorctl) ---"
$SSH 'supervisorctl status zkpow-prover 2>/dev/null || echo "(supervisor not yet running)"'

echo
echo "--- continuous-prover process ---"
$SSH 'ps aux | grep -E "[c]ontinuous-prover|[z]kpow" | head -5 || echo "(not running)"'

# ---- GPU usage --------------------------------------------------------------
echo
echo "--- GPU (nvidia-smi) ---"
$SSH 'nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu \
    --format=csv,noheader 2>/dev/null | column -t -s, || echo "(nvidia-smi not available)"'

# ---- Disk usage -------------------------------------------------------------
echo
echo "--- Disk usage ---"
$SSH 'df -h /workspace /app 2>/dev/null || df -h /'

# ---- Output file growth -----------------------------------------------------
echo
echo "--- Proof output files ---"
$SSH 'find /workspace/profiling -name "*.bin" 2>/dev/null \
    | sort | while read -r f; do ls -lh "$f"; done \
    | head -20 || echo "(no proof files yet)"'

echo
echo "--- Output directory sizes ---"
$SSH 'du -sh /workspace/profiling /workspace/logs 2>/dev/null || echo "(nothing yet)"'

# ---- Recent log entries (last 30 lines from JSONL) --------------------------
echo
echo "--- Recent log entries (logs/run.jsonl, last 30) ---"
$SSH 'if [[ -f /workspace/logs/run.jsonl ]]; then
    tail -30 /workspace/logs/run.jsonl \
    | python3 -c "
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    try:
        obj = json.loads(line)
        ts  = obj.get(\"timestamp\",\"\")[:19]
        lvl = obj.get(\"level\",\"\")
        msg = obj.get(\"fields\",{}).get(\"message\",\"\")
        print(f\"{ts}  {lvl:<5}  {msg}\")
    except Exception:
        print(line)
" 2>/dev/null || tail -30 /workspace/logs/run.jsonl
else
    echo "(logs/run.jsonl not yet created)"
fi'

echo
echo "====================================================================="
echo "Connect:  ssh ${SSH_OPTS} root@${SSH_HOST}"
echo "Restart:  $SSH 'supervisorctl restart zkpow-prover'"
echo "Logs raw: $SSH 'tail -f /workspace/logs/run.jsonl'"
echo "====================================================================="
