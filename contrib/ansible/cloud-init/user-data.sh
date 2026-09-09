#!/usr/bin/env bash
# =============================================================================
# DigitalOcean droplet user data: baseline Ubuntu hardening.
#
# Runs once as root on first boot. Creates an unprivileged admin user, moves
# the SSH keys DigitalOcean injected for root over to it, turns off password
# and root logins, puts a default-deny firewall in front of everything but
# SSH, and applies pending package updates.
#
# Attach with:  doctl compute droplet create --user-data-file <this file>
# Progress:     /var/log/cloud-init-output.log on the droplet
# Completion:   /var/lib/droplet-bootstrap.done
# =============================================================================

set -euo pipefail

ADMIN_USER="tcrypt"
# Ubuntu 24.04 activates sshd through ssh.socket, which owns the listening
# port. Moving off 22 means editing the socket unit as well as this value.
SSH_PORT=22
SENTINEL="/var/lib/droplet-bootstrap.done"

export DEBIAN_FRONTEND=noninteractive
APT_OPTS=(-y -o DPkg::Lock::Timeout=600 -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold)

log() { echo "[bootstrap] $*"; }

# -----------------------------------------------------------------------------
# 1. Package updates
# -----------------------------------------------------------------------------
log "updating packages"
apt-get update "${APT_OPTS[@]}"
apt-get upgrade "${APT_OPTS[@]}"
apt-get install "${APT_OPTS[@]}" sudo ufw ca-certificates
apt-get autoremove "${APT_OPTS[@]}"

# -----------------------------------------------------------------------------
# 2. Admin user
# -----------------------------------------------------------------------------
if id -u "${ADMIN_USER}" >/dev/null 2>&1; then
    log "user ${ADMIN_USER} already exists"
else
    log "creating user ${ADMIN_USER}"
    adduser --disabled-password --gecos "" "${ADMIN_USER}"
fi

# No password is ever set, so sudo has to be passwordless or the account is
# locked out of root entirely.
usermod -aG sudo "${ADMIN_USER}"
printf '%s ALL=(ALL) NOPASSWD:ALL\n' "${ADMIN_USER}" > "/etc/sudoers.d/90-${ADMIN_USER}"
chmod 0440 "/etc/sudoers.d/90-${ADMIN_USER}"
visudo -cf "/etc/sudoers.d/90-${ADMIN_USER}"

# -----------------------------------------------------------------------------
# 3. Authorized keys, copied from the keys DigitalOcean injected for root
# -----------------------------------------------------------------------------
ADMIN_HOME="$(getent passwd "${ADMIN_USER}" | cut -d: -f6)"
install -d -m 0700 -o "${ADMIN_USER}" -g "${ADMIN_USER}" "${ADMIN_HOME}/.ssh"

if [ -s /root/.ssh/authorized_keys ]; then
    log "copying root's authorized keys to ${ADMIN_USER}"
    install -m 0600 -o "${ADMIN_USER}" -g "${ADMIN_USER}" \
        /root/.ssh/authorized_keys "${ADMIN_HOME}/.ssh/authorized_keys"
else
    touch "${ADMIN_HOME}/.ssh/authorized_keys"
    chown "${ADMIN_USER}:${ADMIN_USER}" "${ADMIN_HOME}/.ssh/authorized_keys"
    chmod 0600 "${ADMIN_HOME}/.ssh/authorized_keys"
fi

KEY_COUNT="$(grep -cvE '^\s*(#|$)' "${ADMIN_HOME}/.ssh/authorized_keys" || true)"
log "${ADMIN_USER} has ${KEY_COUNT} authorized key(s)"

# -----------------------------------------------------------------------------
# 4. Firewall
#
# Ahead of the SSH changes so a rejected sshd config never leaves the droplet
# sitting there with no firewall at all.
# -----------------------------------------------------------------------------
log "configuring ufw"
ufw --force reset >/dev/null
ufw default deny incoming
ufw default allow outgoing
ufw allow "${SSH_PORT}/tcp" comment 'ssh'
ufw --force enable
ufw status verbose

# -----------------------------------------------------------------------------
# 5. SSH: keys only, no root login
#
# Skipped when no key made it across, because locking out password auth on a
# droplet with no usable key leaves nobody a way in.
# -----------------------------------------------------------------------------
if [ "${KEY_COUNT}" -gt 0 ]; then
    HARDENING=$(cat <<EOF
# Managed by droplet user data
Port ${SSH_PORT}
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitEmptyPasswords no
PubkeyAuthentication yes
UsePAM yes
X11Forwarding no
EOF
)
    # sshd takes the first value it sees for a keyword, and the Include sits at
    # the top of sshd_config, so a low-numbered drop-in outranks both the main
    # file and cloud-init's own 50-cloud-init.conf.
    if grep -Eq '^\s*Include\s+/etc/ssh/sshd_config\.d/\*\.conf' /etc/ssh/sshd_config; then
        install -d -m 0755 /etc/ssh/sshd_config.d
        printf '%s\n' "${HARDENING}" > /etc/ssh/sshd_config.d/00-hardening.conf
        chmod 0644 /etc/ssh/sshd_config.d/00-hardening.conf
    else
        printf '\n%s\n' "${HARDENING}" >> /etc/ssh/sshd_config
    fi

    if /usr/sbin/sshd -t; then
        log "restarting sshd"
        systemctl restart ssh 2>/dev/null || systemctl restart sshd
    else
        log "sshd config rejected; leaving the running config alone"
        exit 1
    fi
else
    log "no authorized keys found; leaving SSH auth settings untouched"
fi

# -----------------------------------------------------------------------------
# 6. Done
# -----------------------------------------------------------------------------
date -Is > "${SENTINEL}"
log "bootstrap complete: ssh ${ADMIN_USER}@<droplet-ip>"
