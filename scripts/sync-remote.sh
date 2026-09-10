#!/bin/bash

REMOTE_HOST="${REMOTE_HOST:-sp1-remote}"

SRC="${1:-${PWD}}"
DST="${2:-/home/tcrypt/}"

echo "Syncing from ${SRC} to ${REMOTE_HOST}:${DST}"

# Execute the sync
rsync -azuv --exclude="target" --exclude="**/target" -P -e "ssh" "${SRC}" "${REMOTE_HOST}:${DST}"
