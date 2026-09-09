# Droplet provisioning

Bootstraps a DigitalOcean Ubuntu droplet into an SP1 build host in two stages:
cloud-init hardens the box at first boot, then Ansible installs the toolchains.

```
cloud-init/user-data.sh    first boot: admin user, SSH hardening, ufw, updates
setup-sp1-droplet.yml      the playbook, which just runs the two roles
roles/rust_dev             rustup, toolchains, editor and CLI tooling, zsh
roles/sp1_builder          host tuning, SP1 build deps, sp1up, verification
```

## Stage 1: cloud-init

`cloud-init/user-data.sh` runs once as root on first boot. It:

- Applies pending package updates.
- Creates the `tcrypt` admin account with passwordless sudo.
- Copies the SSH keys DigitalOcean injected for root over to `tcrypt`.
- Turns off password authentication and root login, leaving key auth only.
- Sets ufw to deny inbound traffic except SSH on port 22.
- Writes `/var/lib/droplet-bootstrap.done` when it finishes.

SSH is only hardened once at least one authorized key has landed on the admin
account. Without that check a droplet created with no SSH key would lock
everyone out.

Progress is in `/var/log/cloud-init-output.log` on the droplet.

## Stage 2: Ansible

Ansible connects as `tcrypt` and escalates with sudo. `ansible.posix` is the
only collection dependency:

```bash
ansible-galaxy collection install -r requirements.yml
```

### rust_dev

rustup with no PATH edits of its own, the `1.97.1` toolchain from
`rust-toolchain.toml` plus stable and nightly, each with cargo, clippy,
rustfmt, rust-analyzer and rust-src. Alongside it: git, neovim, zsh as the
login shell with a starter `~/.zshrc`, zellij, ripgrep, fd, bat, jq, htop,
tmux, rsync, and the base build toolchain. Ubuntu ships `bat` and `fd` as
`batcat` and `fdfind`, so both are symlinked under their usual names in
`~/.local/bin`.

`/etc/profile.d/rust-env.sh` puts `~/.cargo/bin` and `~/.local/bin` on PATH for
every login shell.

### sp1_builder

Assumes rust_dev has run and fails fast if rustup is missing.

- An 8 GB swapfile, since droplets ship with none and release builds OOM.
- `vm.max_map_count=524288`, `fs.file-max=2097152`, `vm.swappiness=10`.
- `/dev/shm` resized to 8 GB for multi-threaded trace rings.
- clang, lld, cmake, protobuf, golang, and the rest of the SP1 build deps.
- sp1up, `cargo-prove`, and the succinct RISC-V toolchain.
- `/etc/profile.d/sp1.sh` for `~/.sp1/bin`, `RUSTUP_TOOLCHAIN`, `RUST_LOG`.
- A `memfd_create` probe confirming the kernel will back the x86_64 JIT.

Both roles take `target_user` and `target_home`, which default to the
inventory's `ansible_user`.

## Running it

From the repository root:

```bash
just droplet-provision    # create the droplet, attach the user data, wait for it
just droplet-setup        # run the playbook
just droplet-ssh
```

`droplet-provision` writes the droplet IP into `inventory.ini` and blocks until
the bootstrap sentinel appears, so `droplet-setup` never races cloud-init.

Directly, without just:

```bash
./run.sh <DROPLET_IP>                            # one-off host
ansible-playbook setup-sp1-droplet.yml           # using inventory.ini
```

## Why a Linux droplet

SP1 compiles RISC-V instructions to x86_64 machine code at runtime through
`dynasmrt`. On macOS ARM64 that JIT is disabled upstream and execution falls
back to an interpreter 5 to 10x slower. A native x86_64 droplet gets the JIT,
real `memfd_create` page protections, and a `/dev/shm` big enough for
multi-threaded gas estimation.

Size the droplet at 4 or more vCPUs and 8 GB RAM; CPU-optimized plans are worth
it for benchmark runs.
