#!/usr/bin/env zsh
set -euo pipefail

ENV_FILE="${ENV_FILE:-"${0:A:h}/env"}"
source "${ENV_FILE}"

typeset -a required_env_vars=( "ROOT" "OUT_DIR" "BUILD_PROFILE" "BATCH_SIZE" "BATCH_COUNT" "LOG_DIR" "PROOF_STORE_DIR" "DB_PATH" "RUN_ID" )

for var in "${required_env_vars[@]}"; do
  var="ZKPOW_${var}"
  if [ -z "${(P)var}" ]; then
    echo "Error: required environment variable $var is not set" >&2
    exit 1
  fi
done

printf "%-15s %s \n" "Run id:" "${ZKPOW_RUN_ID}"
printf "%-15s %s \n" "Batch size:" "${ZKPOW_BATCH_SIZE}"
printf "%-15s %s \n" "Batch count:" "${ZKPOW_BATCH_COUNT}"
printf "%-15s %s \n" "Profile:" "${ZKPOW_BUILD_PROFILE}"
printf "%-15s %s \n" "RUST_LOG:" "${RUST_LOG}"
printf "%-15s %s \n" "RUST_LOGGER:" "${RUST_LOGGER}"
printf "%-15s %s \n" "RUST_BACKTRACE:" "${RUST_BACKTRACE}"
printf "%-15s %s \n" "Root:" "${ZKPOW_ROOT}"
printf "%-15s %s \n" "DB path:" "${ZKPOW_DB_PATH}"
printf "%-15s %s \n" "Output root:" "${ZKPOW_OUT_DIR}"
printf "%-15s %s \n" "Output logs:" "${ZKPOW_LOG_DIR}"
printf "%-15s %s \n" "Proof store:" "${ZKPOW_PROOF_STORE_DIR}"

mkdir -p "${ZKPOW_OUT_DIR}"
mkdir -p "${ZKPOW_LOG_DIR}"
mkdir -p "${ZKPOW_PROOF_STORE_DIR}"

make -C "${ZKPOW_ROOT}" BINS=zkpow-host build || { echo "Build failed" >&2; exit 1; }

"${ZKPOW_ROOT}/target/${ZKPOW_BUILD_PROFILE}/zkpow-host" > >(tee -a "${ZKPOW_OUT_DIR}/run.log") 2> >(tee -a "${ZKPOW_OUT_DIR}/run.log" >&2)
