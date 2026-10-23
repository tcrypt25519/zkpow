#!/usr/bin/env bash
# =============================================================================
# zkpow Prover Progress & Health Monitor
#
# Real-time dashboard for Bitcoin header chain SP1 recursive proving.
# Displays current verified height, percent complete, proving velocity,
# estimated time to completion (ETA), GPU telemetry, and artifact sync status.
#
# Usage:
#   ./scripts/vast/monitor-progress.sh              # Single-shot status
#   ./scripts/vast/monitor-progress.sh -w           # Watch mode (refreshes every 5s)
#   ./scripts/vast/monitor-progress.sh -w 2         # Watch mode with custom interval (2s)
#   ./scripts/vast/monitor-progress.sh <run_id>     # Inspect a specific run
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Source optional configuration if present
if [ -f "${SCRIPT_DIR}/env.vast" ]; then
    # shellcheck disable=SC1091
    source "${SCRIPT_DIR}/env.vast"
fi

DB_PATH="${ZKPOW_DB_PATH:-${ROOT_DIR}/headers.db}"
PROOF_STORE_DIR="${ZKPOW_PROOF_STORE_DIR:-${ROOT_DIR}/proof_store}"
BATCH_SIZE="${ZKPOW_BATCH_SIZE:-2016}"

WATCH_MODE=0
WATCH_INTERVAL=5
TARGET_RUN_ID=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -w|--watch)
            WATCH_MODE=1
            if [[ $# -gt 1 && "$2" =~ ^[0-9]+$ ]]; then
                WATCH_INTERVAL="$2"
                shift
            fi
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [-w [interval_seconds]] [run_id]"
            exit 0
            ;;
        *)
            TARGET_RUN_ID="$1"
            shift
            ;;
    esac
done

render_dashboard() {
    local now
    now="$(date -u '+%Y-%m-%d %H:%M:%S UTC')"

    if [ ! -f "${DB_PATH}" ]; then
        echo "[-] Error: Database not found at ${DB_PATH}" >&2
        return 1
    fi

    # 1. Query Run Record
    local run_row
    if [ -n "${TARGET_RUN_ID}" ]; then
        run_row="$(sqlite3 -batch -noheader -separator '|' "${DB_PATH}" \
            "SELECT run_id, verified_height, proof_object, updated_at FROM runs WHERE run_id = '${TARGET_RUN_ID}';" 2>/dev/null || true)"
    else
        run_row="$(sqlite3 -batch -noheader -separator '|' "${DB_PATH}" \
            "SELECT run_id, verified_height, proof_object, updated_at FROM runs ORDER BY updated_at DESC LIMIT 1;" 2>/dev/null || true)"
    fi

    local run_id="none" verified_height=0 proof_obj="none" updated_at=0
    if [ -n "${run_row}" ]; then
        IFS='|' read -r run_id verified_height proof_obj updated_at <<< "${run_row}"
    fi

    # 2. Query Max Height in Database
    local max_height
    max_height="$(sqlite3 -batch -noheader "${DB_PATH}" "SELECT MAX(height) FROM headers;" 2>/dev/null || echo 0)"
    if [ -z "${max_height}" ] || [ "${max_height}" -eq 0 ]; then
        max_height=944565
    fi

    local total_headers=$((max_height + 1))
    local remaining_headers=0
    if [ "${verified_height}" -lt "${max_height}" ]; then
        remaining_headers=$((max_height - verified_height))
    fi

    local remaining_batches=0
    if [ "${remaining_headers}" -gt 0 ]; then
        remaining_batches=$(( (remaining_headers + BATCH_SIZE - 1) / BATCH_SIZE ))
    fi

    # 3. Calculate Progress & Bar
    local pct="0.00"
    if [ "${max_height}" -gt 0 ]; then
        pct="$(awk -v v="${verified_height}" -v m="${max_height}" 'BEGIN { printf "%.2f", (v * 100.0) / m }')"
    fi

    local bar_width=32
    local filled_len=0
    if [ "${max_height}" -gt 0 ]; then
        filled_len="$(awk -v v="${verified_height}" -v m="${max_height}" -v w="${bar_width}" 'BEGIN { printf "%d", (v * w) / m }')"
    fi
    if [ "${filled_len}" -gt "${bar_width}" ]; then filled_len="${bar_width}"; fi
    local empty_len=$((bar_width - filled_len))

    local bar=""
    for ((i=0; i<filled_len; i++)); do bar="${bar}█"; done
    for ((i=0; i<empty_len; i++)); do bar="${bar}░"; done

    # 4. Check Process Status (zkpow-host)
    local proc_status="STOPPED / IDLE"
    local pid="" rss_mb="" cpu_pct="" etime=""
    pid="$(pgrep -f "zkpow-host" 2>/dev/null | head -n 1 || true)"
    if [ -n "${pid}" ]; then
        if ps -p "${pid}" >/dev/null 2>&1; then
            local ps_info
            ps_info="$(ps -o rss=,%cpu=,etime= -p "${pid}" 2>/dev/null || true)"
            if [ -n "${ps_info}" ]; then
                local rss_kb
                read -r rss_kb cpu_pct etime <<< "${ps_info}"
                if [ -n "${rss_kb}" ]; then
                    rss_mb="$(awk -v k="${rss_kb}" 'BEGIN { printf "%.1f", k / 1024 }')"
                fi
                proc_status="RUNNING (PID ${pid}, RSS: ${rss_mb} MB, CPU: ${cpu_pct}%, Uptime: ${etime})"
            else
                proc_status="RUNNING (PID ${pid})"
            fi
        fi
    fi

    # 5. GPU Telemetry
    local gpu_telemetry="Not available (no NVIDIA GPU or nvidia-smi detected)"
    if command -v nvidia-smi >/dev/null 2>&1; then
        local smi_line
        smi_line="$(nvidia-smi --query-gpu=name,temperature.gpu,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null | head -n 1 || true)"
        if [ -n "${smi_line}" ]; then
            local g_name g_temp g_util g_mem_used g_mem_total
            IFS=',' read -r g_name g_temp g_util g_mem_used g_mem_total <<< "${smi_line}"
            # Trim whitespace
            g_name="$(echo "${g_name}" | xargs)"
            g_temp="$(echo "${g_temp}" | xargs)"
            g_util="$(echo "${g_util}" | xargs)"
            g_mem_used="$(echo "${g_mem_used}" | xargs)"
            g_mem_total="$(echo "${g_mem_total}" | xargs)"
            gpu_telemetry="${g_name} | Temp: ${g_temp}°C | GPU Core: ${g_util}% | VRAM: ${g_mem_used} / ${g_mem_total} MiB"
        fi
    fi

    # 6. Proving Velocity & Timings
    local run_log="${ROOT_DIR}/profiling/runs/${run_id}/run.log"
    if [ ! -f "${run_log}" ]; then
        local stripped_id="${run_id#chain-}"
        if [ -f "${ROOT_DIR}/profiling/runs/${stripped_id}/run.log" ]; then
            run_log="${ROOT_DIR}/profiling/runs/${stripped_id}/run.log"
        elif [ -f "${ROOT_DIR}/run.log" ]; then
            run_log="${ROOT_DIR}/run.log"
        fi
    fi
    local avg_batch_secs=0
    local last_batch_secs="N/A"
    local hdr_sec="0.0"
    local eta_str="Calculating..."

    if [ -f "${run_log}" ]; then
        local times_list
        times_list="$(grep -F "TOTAL PROVING TIME:" "${run_log}" 2>/dev/null | tail -n 5 | awk -F': ' '{print $2}' | awk '{print $1}' || true)"
        if [ -n "${times_list}" ]; then
            last_batch_secs="$(echo "${times_list}" | tail -n 1)s"
            avg_batch_secs="$(echo "${times_list}" | awk '{s+=$1} END {if (NR>0) printf "%.2f", s/NR}')"
            if (( $(echo "${avg_batch_secs} > 0" | bc -l 2>/dev/null || awk -v s="${avg_batch_secs}" 'BEGIN { print (s > 0) }') )); then
                hdr_sec="$(awk -v b="${BATCH_SIZE}" -v s="${avg_batch_secs}" 'BEGIN { printf "%.1f", b / s }')"
                if [ "${remaining_batches}" -gt 0 ]; then
                    local total_rem_secs
                    total_rem_secs="$(awk -v r="${remaining_batches}" -v s="${avg_batch_secs}" 'BEGIN { printf "%d", r * s }')"
                    local rem_h=$((total_rem_secs / 3600))
                    local rem_m=$(((total_rem_secs % 3600) / 60))
                    eta_str="${rem_h}h ${rem_m}m (~${remaining_batches} batches @ ${avg_batch_secs}s)"
                else
                    eta_str="0h 00m (Complete!)"
                fi
            fi
        fi
    fi

    # 7. Local Artifacts Status
    local local_proofs=0
    local store_size="0B"
    local latest_proof_file="None"
    local run_proof_dir="${PROOF_STORE_DIR}/${run_id}"
    if [ -d "${run_proof_dir}" ]; then
        local_proofs="$(find "${run_proof_dir}" -name "proof_*.bin" 2>/dev/null | wc -l | tr -d ' ')"
        store_size="$(du -sh "${run_proof_dir}" 2>/dev/null | cut -f1)"
        latest_proof_file="$(ls -t "${run_proof_dir}"/proof_*.bin 2>/dev/null | head -n 1 | xargs -n 1 basename 2>/dev/null || echo "None")"
    fi

    # 8. S3 Sync Status
    local s3_info="Not configured (S3_DEST not set)"
    if [ -n "${S3_DEST:-}" ]; then
        s3_info="${S3_DEST}"
    elif [ -n "${SSH_DEST:-}" ]; then
        s3_info="${SSH_DEST} (SSH rsync)"
    fi

    # Render Dashboard Box
    echo "================================================================================"
    echo "                   zkpow BITCOIN PROVER PROGRESS DASHBOARD                      "
    echo "================================================================================"
    printf "%-18s: %s\n" "Current Time"      "${now}"
    printf "%-18s: %s\n" "Active Run ID"     "${run_id}"
    printf "%-18s: %s\n" "Prover Status"     "${proc_status}"
    printf "%-18s: %s\n" "GPU Telemetry"     "${gpu_telemetry}"
    echo "--------------------------------- PROGRESS -------------------------------------"
    printf "%-18s: %'d / %'d headers (%s%%)\n" "Verified Height" "${verified_height}" "${max_height}" "${pct}"
    printf "%-18s: [%s] %s%%\n" "Chain Completion" "${bar}" "${pct}"
    printf "%-18s: %'d headers (~%d batches remaining @ %d headers/batch)\n" \
           "Remaining Work" "${remaining_headers}" "${remaining_batches}" "${BATCH_SIZE}"
    echo "--------------------------------- VELOCITY -------------------------------------"
    printf "%-18s: %s\n" "Last Batch Time"   "${last_batch_secs}"
    printf "%-18s: %ss / batch (%s headers/sec)\n" "Avg Batch Time" "${avg_batch_secs}" "${hdr_sec}"
    printf "%-18s: %s\n" "Estimated ETA"     "${eta_str}"
    echo "--------------------------------- ARTIFACTS ------------------------------------"
    printf "%-18s: %s proof files (%s on disk)\n" "Local Proof Store" "${local_proofs}" "${store_size}"
    printf "%-18s: %s (ledger pointer: %s)\n" "Latest Proof" "${latest_proof_file}" "${proof_obj}"
    printf "%-18s: %s\n" "Remote Storage"    "${s3_info}"
    echo "================================================================================"
}

if [ "${WATCH_MODE}" -eq 1 ]; then
    while true; do
        clear || true
        render_dashboard
        echo "Refreshing every ${WATCH_INTERVAL}s (Press Ctrl+C to exit)..."
        sleep "${WATCH_INTERVAL}"
    done
else
    render_dashboard
fi
