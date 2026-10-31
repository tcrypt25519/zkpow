# GitHub Actions: CI

**Path:** `.github/workflows/ci.yml`
**Type:** `github_actions`

## Triggers

- `push`
- `pull_request`

## Jobs

| Job | Display Name | Runs On | Needs | Steps |
|---|---|---|---|---:|
| `correctness` | `Correctness Gate` | `ubuntu-latest` | — | 7 |

### correctness

- Checkout repository - uses `actions/checkout@v4`
- Setup SSH for private dependencies - runs `mkdir -p ~/.ssh echo "$ZKVM_TEST_DEPLOY_SSH_KEY" > ~/.ssh/id_ed25519 chmod 600 ~/.ssh/id_ed25519 ssh-keyscan github.com >> ~/.ssh/known_hosts`
- Install Rust toolchain - uses `dtolnay/rust-toolchain@stable`
- Rust cache - uses `Swatinem/rust-cache@v2`
- Check code formatting - runs `cargo fmt --all -- --check`
- Workspace clippy lints - runs `cargo clippy --workspace --all-targets -- -D warnings`
- Run workspace tests - runs `cargo test --workspace`

## Notes

Runs on pushes and pull requests to `main` and `dev`. The SSH step installs a deploy key so Cargo can fetch private git dependencies, and is skipped when the secret is absent. Tests run on the host target, so `cfg(target_os = "zkvm")` code and zkVM-only tests are not exercised here.
