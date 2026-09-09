#!/usr/bin/env bash
# =============================================================================
# scripts/sp1-docker-jit.sh
#
# Helper script to build and run SP1 programs in a Linux x86_64 container with
# the native JIT engine enabled.
#
# Usage:
#   ./scripts/sp1-docker-jit.sh build                # Build the docker image
#   ./scripts/sp1-docker-jit.sh check                # Verify JIT compatibility
#   ./scripts/sp1-docker-jit.sh shell                # Interactive bash shell
#   ./scripts/sp1-docker-jit.sh run <command...>     # Run arbitrary command
#   ./scripts/sp1-docker-jit.sh test [args...]       # Run cargo test
# =============================================================================

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

IMAGE_NAME="${SP1_JIT_IMAGE:-sp1-jit:latest}"
DOCKERFILE="${REPO_ROOT}/Dockerfile.sp1-jit"
SHM_SIZE="${SP1_SHM_SIZE:-8g}"
PLATFORM="linux/amd64"
TARGET_DIR="${REPO_ROOT}/target/docker-linux-x86_64"

# Ensure target directory exists on host
mkdir -p "${TARGET_DIR}"

run_container() {
    local it_flag=""
    if [ -t 0 ] && [ -t 1 ]; then
        it_flag="-it"
    elif [ -t 0 ] || [ -t 1 ]; then
        it_flag="-i"
    fi

    # Create cargo cache volume if it doesn't exist
    docker volume create sp1-jit-cargo-cache >/dev/null 2>&1 || true
    docker volume create sp1-jit-cargo-git >/dev/null 2>&1 || true

    docker run --platform "${PLATFORM}" --rm ${it_flag} \
        --shm-size="${SHM_SIZE}" \
        -v "${REPO_ROOT}:/workspace" \
        -v sp1-jit-cargo-cache:/usr/local/cargo/registry \
        -v sp1-jit-cargo-git:/usr/local/cargo/git \
        -w /workspace \
        -e RUST_LOG="${RUST_LOG:-info}" \
        -e RUST_BACKTRACE="${RUST_BACKTRACE:-1}" \
        -e RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.97.1}" \
        -e CARGO_TARGET_DIR="/workspace/target/docker-linux-x86_64" \
        "${IMAGE_NAME}" \
        "$@"
}

cmd="${1:-shell}"
if [ $# -gt 0 ]; then
    shift
fi

case "$cmd" in
    build)
        echo "==> Building ${IMAGE_NAME} for platform ${PLATFORM}..."
        DOCKER_BUILDKIT=1 docker build \
            --platform "${PLATFORM}" \
            -t "${IMAGE_NAME}" \
            -f "${DOCKERFILE}" \
            "${REPO_ROOT}"
        echo "==> Build complete: ${IMAGE_NAME}"
        ;;
    check)
        echo "==> Verifying SP1 JIT container environment..."
        run_container /usr/local/bin/sp1-jit-check
        ;;
    shell)
        echo "==> Starting interactive SP1 Linux JIT shell..."
        run_container /bin/bash
        ;;
    run)
        if [ $# -eq 0 ]; then
            echo "Error: 'run' requires a command to execute."
            echo "Example: ./scripts/sp1-docker-jit.sh run cargo prove --version"
            exit 1
        fi
        run_container "$@"
        ;;
    test)
        echo "==> Running tests in Linux x86_64 container..."
        run_container cargo test "$@"
        ;;
    help|--help|-h)
        echo "SP1 Linux JIT Docker Runner"
        echo ""
        echo "Usage:"
        echo "  $0 build               Build the sp1-jit:latest docker image"
        echo "  $0 check               Verify JIT compatibility and environment"
        echo "  $0 shell               Open interactive bash prompt in container"
        echo "  $0 run <command...>    Execute a command inside the container"
        echo "  $0 test [args...]      Run cargo test inside the container"
        echo ""
        echo "Environment overrides:"
        echo "  SP1_JIT_IMAGE   Docker image tag (default: sp1-jit:latest)"
        echo "  SP1_SHM_SIZE    Shared memory size (default: 8g)"
        ;;
    *)
        # Default to running the arguments as command if unknown subcommand
        run_container "$cmd" "$@"
        ;;
esac
