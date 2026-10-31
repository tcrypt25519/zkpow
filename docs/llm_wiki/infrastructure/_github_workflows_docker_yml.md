# GitHub Actions: docker

**Path:** `.github/workflows/docker.yml`
**Type:** `github_actions`

## Triggers

- `workflow_dispatch`

## Jobs

| Job | Display Name | Runs On | Needs | Steps |
|---|---|---|---|---:|
| `push` | — | `ubuntu-latest` | — | 6 |

### push

- actions/checkout@v4 - uses `actions/checkout@v4`
- webfactory/ssh-agent@v0.9.0 - uses `webfactory/ssh-agent@v0.9.0`
- docker/setup-buildx-action@v3 - uses `docker/setup-buildx-action@v3`
- docker/login-action@v3 - uses `docker/login-action@v3`
- make docker DOCKER_IMAGE=tcrypt/zkpow-prover - runs `make docker DOCKER_IMAGE=tcrypt/zkpow-prover`
- docker push tcrypt/zkpow-prover:latest - runs `docker push tcrypt/zkpow-prover:latest`

## Notes

Manual trigger only. `make docker` in `docker/` builds [`Dockerfile.vast-cuda`](docker_Dockerfile_vast-cuda.md) with a registry build cache at `tcrypt/zkpow-prover:buildcache`, then the job pushes `tcrypt/zkpow-prover:latest`. Needs the `ZKVM_TEST_DEPLOY_SSH_KEY` and `DOCKER_HUB_TOKEN` secrets.
