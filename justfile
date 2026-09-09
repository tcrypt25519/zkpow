# https://just.systems
set unstable
set positional-arguments


do_env(name, default) := env(f"ZKVM_DO_{{name}}", default)

# Configuration Defaults
DROPLET_REGION := do_env("REGION", "atl1")
DROPLET_SIZE := do_env("SIZE", "s-2vcpu-4gb-120gb-intel")
DROPLET_IMAGE := do_env("IMAGE", "ubuntu-24-04-x64")
DROPLET_NAME := env("NAME", "zkvm-remote-1")
SSH_USER := do_env("USER", "tcrypt")
SSH_KEY := env("SSH_KEY", "$HOME/.ssh/keys/id_remote_dev")
INVENTORY := "contrib/ansible/inventory.ini"
USER_DATA := "contrib/ansible/cloud-init/user-data.sh"

_default:
    @just --list

# =============================================================================
# Local Development & Testing
# =============================================================================

# Build zkvm-test in release mode
build-zkvm-test:
    cargo build -p zkvm-test --release

# Install zkvm-test to cargo bin
install-zkvm-test:
    cargo install --path crates/zkvm-test --locked

# =============================================================================
# Cross-Compilation (macOS -> Ubuntu x86_64)
# =============================================================================

# Cross-compile a package for Ubuntu x86_64 (e.g. just build-linux zkvm-test-runner release)
build-linux pkg="zkvm-test-runner" profile="release":
    #!/usr/bin/env bash
    set -eo pipefail
    profile_flag=""
    if [ "{{profile}}" = "release" ]; then
        profile_flag="--release"
    fi
    echo "==> Cross-compiling {{pkg}} ({{profile}}) for Ubuntu x86_64..."
    cargo +stable zigbuild ${profile_flag} --target x86_64-unknown-linux-gnu.2.35 -p {{pkg}}
    echo "==> Binary built: target/x86_64-unknown-linux-gnu/{{profile}}/{{pkg}}"

# =============================================================================
# DigitalOcean Droplet Lifecycle (doctl + Ansible)
# =============================================================================

# Provision a DigitalOcean Droplet with doctl and update inventory.ini (skips if IP exists)
droplet-provision name=DROPLET_NAME size=DROPLET_SIZE region=DROPLET_REGION:
    #!/usr/bin/env bash
    set -eo pipefail
    
    # Check if inventory already contains an active IP
    if [ -f "{{INVENTORY}}" ]; then
        EXISTING_IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1 || true)
        if [ -n "${EXISTING_IP}" ] && [ "${FORCE_NEW:-0}" != "1" ]; then
            echo "==> Droplet already configured in {{INVENTORY}} with IP: ${EXISTING_IP}"
            echo "    Run 'FORCE_NEW=1 just droplet-provision' to create another droplet."
            exit 0
        fi
    fi

    if ! command -v doctl >/dev/null 2>&1; then
        echo "Error: 'doctl' CLI tool is required but not installed or linked."
        echo "Install with: brew install doctl && brew link --overwrite doctl"
        exit 1
    fi

    echo "==> Fetching SSH key fingerprints from DigitalOcean..."
    SSH_KEYS=$(doctl compute ssh-key list --format FingerPrint --no-header 2>/dev/null | tr '\n' ',' | sed 's/,$//' || true)
    SSH_KEY_ARG=""
    if [ -n "${SSH_KEYS}" ]; then
        echo "==> Attaching SSH keys: ${SSH_KEYS}"
        SSH_KEY_ARG="--ssh-keys ${SSH_KEYS}"
    else
        echo "[!] Warning: No SSH keys found in doctl account. Passwords may be required."
    fi

    if [ ! -f "{{USER_DATA}}" ]; then
        echo "Error: user data script not found at {{USER_DATA}}."
        exit 1
    fi

    echo "==> Provisioning Droplet '{{name}}' ({{size}} in {{region}})..."
    DROPLET_IP=$(doctl compute droplet create \
        --image "{{DROPLET_IMAGE}}" \
        --size "{{size}}" \
        --region "{{region}}" \
        --tag-names "sp1-builder" \
        --user-data-file "{{USER_DATA}}" \
        --wait \
        ${SSH_KEY_ARG} \
        --format PublicIPv4 \
        --no-header \
        "{{name}}")

    if [ -z "${DROPLET_IP}" ]; then
        echo "Error: Failed to retrieve Public IPv4 for created droplet."
        exit 1
    fi

    echo "==> Successfully created droplet! Public IP: ${DROPLET_IP}"
    echo "==> Updating {{INVENTORY}}..."
    mkdir -p "$(dirname "{{INVENTORY}}")"
    printf "%s\n%s\n\n%s\n%s\n" \
        "[sp1_droplets]" \
        "sp1-builder ansible_host=${DROPLET_IP} ansible_user={{SSH_USER}} ansible_ssh_private_key_file={{SSH_KEY}}" \
        "[sp1_droplets:vars]" \
        "ansible_python_interpreter=/usr/bin/python3" > "{{INVENTORY}}"
    echo "==> Inventory saved to {{INVENTORY}}:"
    cat "{{INVENTORY}}"
    just droplet-wait "${DROPLET_IP}"

# Block until cloud-init has finished running the user data bootstrap
droplet-wait ip="" timeout="600":
    #!/usr/bin/env bash
    set -eo pipefail
    TARGET_IP="{{ip}}"
    if [ -z "${TARGET_IP}" ] && [ -f "{{INVENTORY}}" ]; then
        TARGET_IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1 || true)
    fi
    if [ -z "${TARGET_IP}" ]; then
        echo "Error: No Droplet IP provided and none found in {{INVENTORY}}."
        exit 1
    fi

    echo "==> Waiting for droplet bootstrap on {{SSH_USER}}@${TARGET_IP} (up to {{timeout}}s)..."
    DEADLINE=$(( $(date +%s) + {{timeout}} ))
    until ssh -i "{{SSH_KEY}}" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=5 \
        "{{SSH_USER}}@${TARGET_IP}" "test -f /var/lib/droplet-bootstrap.done" 2>/dev/null; do
        if [ "$(date +%s)" -ge "${DEADLINE}" ]; then
            echo "Error: bootstrap did not complete within {{timeout}}s."
            echo "       Check /var/log/cloud-init-output.log on the droplet."
            exit 1
        fi
        sleep 10
    done
    echo "==> Droplet bootstrap complete; ready for 'just droplet-setup'."

# Run Ansible provisioning playbook against the Droplet
droplet-setup:
    #!/usr/bin/env bash
    set -eo pipefail
    if [ -f "{{INVENTORY}}" ]; then
        TARGET_IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1 || true)
    fi

    if [ -z "${TARGET_IP}" ]; then
        echo "Error: No Droplet IP provided and no IP found in {{INVENTORY}}."
        echo "Usage: just droplet-setup [IP] or run 'just droplet-provision' first."
        exit 1
    fi

    echo "==> Running Ansible setup on ${TARGET_IP}..."
    ./contrib/ansible/run.sh "${TARGET_IP}" "{{SSH_KEY}}" "{{SSH_USER}}"

# SSH into the configured Droplet
droplet-ssh:
    #!/usr/bin/env bash
    set -eo pipefail
    if [ ! -f "{{INVENTORY}}" ]; then
        echo "Error: {{INVENTORY}} not found. Run 'just droplet-provision' first."
        exit 1
    fi
    IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1)
    if [ -z "${IP}" ]; then
        echo "Error: No IP found in {{INVENTORY}}."
        exit 1
    fi
    echo "==> Connecting to {{SSH_USER}}@${IP}..."
    ssh -i "{{SSH_KEY}}" "{{SSH_USER}}@${IP}"

# =============================================================================
# Deployment & Code Sync
# =============================================================================

# Cross-compile and deploy a single binary to the Droplet (e.g. just deploy-bin zkvm-test-runner /usr/local/bin)
deploy-bin pkg="zkvm-test-runner" dest="/usr/local/bin" profile="release":
    #!/usr/bin/env bash
    set -eo pipefail
    if [ ! -f "{{INVENTORY}}" ]; then
        echo "Error: {{INVENTORY}} not found. Run 'just droplet-provision' first."
        exit 1
    fi
    IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1)
    if [ -z "${IP}" ]; then
        echo "Error: No IP found in {{INVENTORY}}."
        exit 1
    fi

    # 1. Cross-compile locally on MacBook
    just build-linux "{{pkg}}" "{{profile}}"

    BIN_PATH="target/x86_64-unknown-linux-gnu/{{profile}}/{{pkg}}"
    if [ ! -f "${BIN_PATH}" ]; then
        echo "Error: Built binary not found at ${BIN_PATH}"
        exit 1
    fi

    # 2. Transfer binary to Droplet
    echo "==> Deploying ${BIN_PATH} to {{SSH_USER}}@${IP}:{{dest}}/"
    scp -i "{{SSH_KEY}}" "${BIN_PATH}" "{{SSH_USER}}@${IP}:/tmp/{{pkg}}"
    ssh -i "{{SSH_KEY}}" "{{SSH_USER}}@${IP}" \
        "sudo install -m 0755 /tmp/{{pkg}} {{dest}}/{{pkg}} && rm -f /tmp/{{pkg}}"
    echo "==> Deployment complete: {{dest}}/{{pkg}} is ready on Droplet."

# Sync the repository directly to the Droplet using rsync
sync-repo dest="~/bitcoin-header-chain":
    #!/usr/bin/env bash
    set -eo pipefail
    if [ ! -f "{{INVENTORY}}" ]; then
        echo "Error: {{INVENTORY}} not found. Run 'just droplet-provision' first."
        exit 1
    fi
    IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1)
    if [ -z "${IP}" ]; then
        echo "Error: No IP found in {{INVENTORY}}."
        exit 1
    fi

    echo "==> Syncing workspace to {{SSH_USER}}@${IP}:{{dest}}..."
    ssh -i "{{SSH_KEY}}" "{{SSH_USER}}@${IP}" "mkdir -p {{dest}}"
    rsync -avz --delete \
        --exclude 'target/' \
        --exclude '.git/' \
        --exclude '.cargo/registry/' \
        --exclude 'headers.db' \
        -e "ssh -i {{SSH_KEY}}" \
        ./ "{{SSH_USER}}@${IP}:{{dest}}/"
    echo "==> Repository synced successfully."

# Run tests directly on the Droplet over SSH
droplet-test pkg="zkvm-test-runner":
    #!/usr/bin/env bash
    set -eo pipefail
    if [ ! -f "{{INVENTORY}}" ]; then
        echo "Error: {{INVENTORY}} not found. Run 'just droplet-provision' first."
        exit 1
    fi
    IP=$(grep -oE 'ansible_host=[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "{{INVENTORY}}" | cut -d= -f2 | head -n1)
    echo "==> Running 'cargo test -p {{pkg}}' on {{SSH_USER}}@${IP}..."
    ssh -i "{{SSH_KEY}}" "{{SSH_USER}}@${IP}" "cd ~/bitcoin-header-chain && . /etc/profile && cargo test -p {{pkg}}"
