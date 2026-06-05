#!/bin/bash
set -euo pipefail
# zkpow — Docker Helper Script
#
# Usage:
#   ./scripts/docker-prover.sh build
#   ./scripts/docker-prover.sh run
#   ./scripts/docker-prover.sh run-cuda
#   ./scripts/docker-prover.sh upload-proof <proof-file> <s3-bucket>
#   ./scripts/docker-prover.sh logs
#   ./scripts/docker-prover.sh clean

set -e

IMAGE_NAME="zkpow:latest"
CONTAINER_NAME="zkpow"
INPUT_VOLUME="zkpow-input"
OUTPUT_VOLUME="zkpow-output"

case "${1:-}" in
build)
  echo "Building prover image..."
  docker build -t "$IMAGE_NAME" .
  echo "Build complete: $IMAGE_NAME"
  ;;

run)
  echo "Starting prover (CPU mode)..."
  docker run -d \
    --name "$CONTAINER_NAME" \
    --restart unless-stopped \
    -v "$INPUT_VOLUME:/input" \
    -v "$OUTPUT_VOLUME:/output" \
    -e RUST_LOG="${RUST_LOG:-info}" \
    "$IMAGE_NAME"
  echo "Container started. View logs with: docker logs -f $CONTAINER_NAME"
  ;;

run-cuda)
  echo "Starting prover (CUDA mode)..."
  docker run -d \
    --name "$CONTAINER_NAME" \
    --gpus all \
    --restart unless-stopped \
    -v "$INPUT_VOLUME:/input" \
    -v "$OUTPUT_VOLUME:/output" \
    -e CUDA=1 \
    -e CUDA_DEVICE_ID="${CUDA_DEVICE_ID:-0}" \
    -e RUST_LOG="${RUST_LOG:-info}" \
    "$IMAGE_NAME"
  echo "Container started. View logs with: docker logs -f $CONTAINER_NAME"
  ;;

run-compose)
  echo "Starting with docker-compose..."
  docker compose up -d
  ;;

logs)
  docker logs -f "$CONTAINER_NAME"
  ;;

stop)
  echo "Stopping prover..."
  docker stop "$CONTAINER_NAME" 2>/dev/null || true
  docker compose down 2>/dev/null || true
  ;;

clean)
  echo "Cleaning up containers, volumes, and images..."
  docker stop "$CONTAINER_NAME" 2>/dev/null || true
  docker rm "$CONTAINER_NAME" 2>/dev/null || true
  docker volume rm "$INPUT_VOLUME" "$OUTPUT_VOLUME" 2>/dev/null || true
  docker rmi "$IMAGE_NAME" 2>/dev/null || true
  echo "Cleanup complete"
  ;;

upload-proof)
  if [ -z "${2:-}" ] || [ -z "${3:-}" ]; then
    echo "Usage: $0 upload-proof <proof-file> <s3-bucket> [s3-prefix]"
    exit 1
  fi
  PROOF_FILE="$2"
  S3_BUCKET="$3"
  S3_PREFIX="${4:-proofs}"

  if [ ! -f "$PROOF_FILE" ]; then
    echo "Error: Proof file not found: $PROOF_FILE"
    exit 1
  fi

  echo "Uploading $PROOF_FILE to s3://$S3_BUCKET/$S3_PREFIX/..."
  aws s3 cp "$PROOF_FILE" "s3://$S3_BUCKET/$S3_PREFIX/$(basename "$PROOF_FILE")"
  echo "Upload complete"
  ;;

list-proofs)
  echo "Proofs in output volume:"
  docker run --rm -v "$OUTPUT_VOLUME:/output" "$IMAGE_NAME" ls -lh /output/*.bin 2>/dev/null || echo "No proofs found"
  ;;

shell)
  if docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME" 2>/dev/null | grep -q true; then
    echo "Container is running, attaching to it..."
    docker exec -it "$CONTAINER_NAME" /bin/bash
  else
    echo "Container is not running, starting a new one..."
    docker run -it --rm \
      -v "$INPUT_VOLUME:/input" \
      -v "$OUTPUT_VOLUME:/output" \
      "$IMAGE_NAME" /bin/bash
  fi
  ;;

*)
  cat <<EOF
zkpow — Docker Helper

Usage: $0 <command>

Commands:
    build           Build the prover Docker image
    run             Start prover in CPU mode (detached)
    run-cuda        Start prover with GPU support (requires NVIDIA Container Toolkit)
    run-compose     Start using docker-compose
    logs            Follow container logs
    stop            Stop the prover
    clean           Remove containers, volumes, and images
    upload-proof    Upload a proof file to S3
    list-proofs     List proofs in the output volume
    shell           Open shell in running container (or new ephemeral)

Examples:
    $0 build
    $0 run-cuda
    $0 upload-proof proof_height_0_to_100.bin my-bucket
EOF
  ;;
esac
