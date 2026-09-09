#!/usr/bin/env bash
# =============================================================================
# Helper script to run setup-sp1-droplet.yml against a DigitalOcean Droplet
#
# Usage:
#   ./run.sh <DROPLET_IP> [SSH_KEY_PATH] [USER]
#
# Examples:
#   ./run.sh 159.203.123.45
#   ./run.sh 159.203.123.45 ~/.ssh/id_ed25519
#   ./run.sh 159.203.123.45 ~/.ssh/id_rsa tcrypt
# =============================================================================

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

DROPLET_IP="${1:-$DROPLET_IP}"
KEY_PATH="${2:-$HOME/.ssh/id_ed25519}"
SSH_USER="${3:-tcrypt}"

if [ -z "${DROPLET_IP}" ]; then
    if [ -f "inventory.ini" ]; then
        echo "==> Running playbook using existing inventory.ini..."
        ansible-playbook setup-sp1-droplet.yml
        exit 0
    fi

    echo "Error: No Droplet IP specified and no inventory.ini found."
    echo ""
    echo "Usage:"
    echo "  $0 <DROPLET_IP> [SSH_KEY_PATH] [USER]"
    echo ""
    echo "Or copy inventory.ini.example to inventory.ini and run: ansible-playbook setup-sp1-droplet.yml"
    exit 1
fi

# Fallback to id_rsa if id_ed25519 doesn't exist and wasn't explicitly passed
if [ "$#" -lt 2 ] && [ ! -f "${KEY_PATH}" ] && [ -f "$HOME/.ssh/id_rsa" ]; then
    KEY_PATH="$HOME/.ssh/id_rsa"
fi

echo "==> Target Droplet: ${SSH_USER}@${DROPLET_IP}"
if [ -f "${KEY_PATH}" ]; then
    echo "==> SSH Key:        ${KEY_PATH}"
    KEY_ARG="--private-key=${KEY_PATH}"
else
    echo "==> SSH Key:        default SSH agent / config"
    KEY_ARG=""
fi

ansible-playbook -i inventory.ini setup-sp1-droplet.yml 
