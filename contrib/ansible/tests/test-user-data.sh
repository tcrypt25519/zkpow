#!/usr/bin/env bash
# =============================================================================
# Exercises cloud-init/user-data.sh against a real ubuntu:24.04 container.
#
#   ./tests/test-user-data.sh
#
# Three scenarios: keys injected (the normal droplet case), no keys injected
# (the lockout guard), and a second run over the first (idempotency).
#
# systemctl and ufw are stubbed, since neither works without systemd or
# netfilter. Every call is logged so the assertions can check what was asked
# for. Everything else, including sshd config validation, is the real thing.
# =============================================================================

set -euo pipefail

ANSIBLE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${TEST_IMAGE:-ubuntu:24.04}"

if ! docker info >/dev/null 2>&1; then
    echo "Error: docker is not running."
    exit 1
fi

# -----------------------------------------------------------------------------
# Runs inside the container: stub what containers cannot do, fake the state a
# DigitalOcean image arrives in, then run the script under test.
# -----------------------------------------------------------------------------
read -r -d '' SETUP <<'INNER' || true
set -euo pipefail
WITH_KEYS="$1"

install -d /usr/local/sbin
for stub in systemctl ufw; do
    printf '#!/bin/sh\necho "%s $*" >> /tmp/stub.log\nexit 0\n' "${stub}" > "/usr/local/sbin/${stub}"
    chmod 0755 "/usr/local/sbin/${stub}"
done
: > /tmp/stub.log

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
# Present on the DigitalOcean image, absent from the base container image.
apt-get install -y -qq openssh-server sudo >/dev/null
# systemd-tmpfiles creates this on a real host, and sshd -t needs it.
install -d -m 0755 /run/sshd

install -d -m 0700 /root/.ssh
if [ "${WITH_KEYS}" = "yes" ]; then
    cat > /root/.ssh/authorized_keys <<'KEYS'
# a comment, which must not be counted as a key

ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBogusKeyForTestingOnlyDoNotUse test@harness
KEYS
    chmod 0600 /root/.ssh/authorized_keys
fi

bash /work/cloud-init/user-data.sh
if [ "${2:-}" = "twice" ]; then
    echo "### second run ###"
    bash /work/cloud-init/user-data.sh
fi
INNER

# -----------------------------------------------------------------------------
# Assertions, also run inside the container.
# -----------------------------------------------------------------------------
read -r -d '' ASSERT <<'INNER' || true
set -u
WITH_KEYS="$1"
FAILED=0
ok()   { echo "  PASS  $1"; }
fail() { echo "  FAIL  $1"; FAILED=1; }
check() { if eval "$2" >/dev/null 2>&1; then ok "$1"; else fail "$1"; fi; }

# Dumped to a file rather than piped: grep -q closes the pipe early and the
# resulting SIGPIPE would trip pipefail in the caller.
/usr/sbin/sshd -T > /tmp/sshd-effective.conf 2>/dev/null || : > /tmp/sshd-effective.conf

check "tcrypt account exists"           'id -u tcrypt'
check "tcrypt is in the sudo group"     'id -nG tcrypt | tr " " "\n" | grep -qx sudo'
check "sudoers drop-in parses"          'visudo -cf /etc/sudoers.d/90-tcrypt'
check "sudoers drop-in is 0440"         '[ "$(stat -c %a /etc/sudoers.d/90-tcrypt)" = 440 ]'
check "passwordless sudo resolves"      'sudo -l -U tcrypt | grep -q "NOPASSWD: ALL"'
check "tcrypt has no password"          'passwd -S tcrypt | cut -d" " -f2 | grep -qE "^(L|NP)$"'
check ".ssh is 0700 and owned"          '[ "$(stat -c "%a %U" /home/tcrypt/.ssh)" = "700 tcrypt" ]'
check "authorized_keys is 0600, owned"  '[ "$(stat -c "%a %U" /home/tcrypt/.ssh/authorized_keys)" = "600 tcrypt" ]'
check "ufw denies incoming by default"  'grep -qx "ufw default deny incoming" /tmp/stub.log'
check "ufw allows outgoing"             'grep -qx "ufw default allow outgoing" /tmp/stub.log'
check "ufw opens ssh on 22"             'grep -q "ufw allow 22/tcp" /tmp/stub.log'
check "ufw is enabled"                  'grep -qx "ufw --force enable" /tmp/stub.log'
check "bootstrap sentinel written"      'test -s /var/lib/droplet-bootstrap.done'

if [ "${WITH_KEYS}" = "yes" ]; then
    check "key copied to tcrypt"        'grep -q "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBogusKey" /home/tcrypt/.ssh/authorized_keys'
    check "comments are not counted"    'grep -q "tcrypt has 1 authorized key" /tmp/bootstrap.log'
    check "hardening drop-in written"   'test -f /etc/ssh/sshd_config.d/00-hardening.conf'
    check "drop-in outranks cloud-init" '[ "$(ls /etc/ssh/sshd_config.d/ | sort | head -n1)" = "00-hardening.conf" ]'
    check "sshd accepts the config"     '/usr/sbin/sshd -t'
    check "effective: no password auth" 'grep -qx "passwordauthentication no" /tmp/sshd-effective.conf'
    check "effective: no kbd-int auth"  'grep -qx "kbdinteractiveauthentication no" /tmp/sshd-effective.conf'
    check "effective: no root login"    'grep -qx "permitrootlogin no" /tmp/sshd-effective.conf'
    check "effective: pubkey auth on"   'grep -qx "pubkeyauthentication yes" /tmp/sshd-effective.conf'
    check "effective: port 22"          'grep -qx "port 22" /tmp/sshd-effective.conf'
    check "sshd was restarted"          'grep -q "systemctl restart ssh" /tmp/stub.log'
else
    check "empty authorized_keys made"  'test -f /home/tcrypt/.ssh/authorized_keys'
    check "no hardening drop-in"        '! test -f /etc/ssh/sshd_config.d/00-hardening.conf'
    check "sshd was left alone"         '! grep -q "systemctl restart ssh" /tmp/stub.log'
    check "password auth still default" 'grep -qx "passwordauthentication yes" /tmp/sshd-effective.conf'
fi

exit $FAILED
INNER

TMPDIR_HARNESS="$(mktemp -d)"
trap 'rm -rf "${TMPDIR_HARNESS}"' EXIT
printf '%s\n' "${SETUP}"  > "${TMPDIR_HARNESS}/setup.sh"
printf '%s\n' "${ASSERT}" > "${TMPDIR_HARNESS}/assert.sh"

run_scenario() {
    local name="$1" keys="$2" twice="${3:-once}"
    echo "=== ${name} ==="
    docker run --rm --platform linux/amd64 \
        -v "${ANSIBLE_DIR}:/work:ro" -v "${TMPDIR_HARNESS}:/harness:ro" \
        "${IMAGE}" bash -c \
        'bash /harness/setup.sh "$1" "$2" > /tmp/bootstrap.log 2>&1 || {
             echo "  bootstrap script failed:"; tail -20 /tmp/bootstrap.log; exit 1; }
         bash /harness/assert.sh "$1"' bash "${keys}" "${twice}"
    echo
}

FAILURES=0
run_scenario "keys injected"   yes       || FAILURES=1
run_scenario "no keys injected" no       || FAILURES=1
run_scenario "run twice"       yes twice || FAILURES=1

if [ "${FAILURES}" -eq 0 ]; then
    echo "all scenarios passed"
else
    echo "FAILURES"
fi
exit "${FAILURES}"
