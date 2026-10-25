use zkpow_core::{
    advance_ordered_timestamp_run, calculate_next_work_required, check_proof_of_work, has_ordered_timestamp_window,
    ordered_median_timestamp_slot, timestamp_slot, BlockTimestamp,
};
use zkpow_params::{Consensus, WINDOW_SIZE};

use crate::types::{expand_target, ConnectedHeader, Header, Tip};

/// Validate and link `headers` onto `anchor`: linkage, PoW against the active target, and the
/// timestamp-above-MTP rule. Stops at the first failure and returns the connected prefix.
pub fn connect_headers<C: Consensus>(anchor: &Tip, headers: &[Header]) -> Vec<ConnectedHeader> {
    let mut connected = Vec::with_capacity(headers.len());
    let mut height = anchor.height;
    let mut prev_hash = anchor.hash;
    let mut cumulative_work = anchor.chainwork;
    let mut timestamps = anchor.timestamps;
    let mut ordered_timestamp_run = anchor.ordered_timestamp_run;
    let mut epoch_timestamp = anchor.epoch_timestamp;

    let mut active_nbits = anchor.current_nbits;
    let mut active_target = anchor.current_target;
    let mut active_work = anchor.current_block_work;

    for header in headers {
        if header.prev_blockhash() != prev_hash {
            tracing::debug!(
                expected = %prev_hash,
                got = %header.prev_blockhash(),
                "chain broken: prev_hash mismatch"
            );
            break;
        }

        let candidate_height = height + 1;
        let previous_timestamp = timestamps[timestamp_slot(height)];

        if candidate_height.is_multiple_of(C::PARAMS.epoch_length) {
            (active_nbits, active_target, active_work) = calculate_next_work_required::<C>(
                active_target,
                BlockTimestamp::new(epoch_timestamp),
                BlockTimestamp::new(previous_timestamp),
            );
        }

        if header.inner.compact_target != active_nbits {
            tracing::debug!(
                expected = ?active_nbits,
                got = ?header.inner.compact_target,
                "compact_target mismatch"
            );
            break;
        }

        let hash = header.block_hash();
        if !check_proof_of_work(hash, &active_target) {
            tracing::debug!(%hash, "PoW validation failed");
            break;
        }

        let window_len = zkpow_core::timestamp_count(height);
        let mtp = if has_ordered_timestamp_window(ordered_timestamp_run) {
            timestamps[ordered_median_timestamp_slot(height)]
        } else {
            let mut sorted = timestamps;
            sorted[..window_len].sort_unstable();
            sorted[window_len / 2]
        };

        if header.inner.timestamp.into_inner() <= mtp {
            tracing::debug!(%hash, timestamp = header.inner.timestamp.into_inner(), mtp, "timestamp not greater than MTP");
            break;
        }

        cumulative_work += active_work;
        height = candidate_height;
        if height.is_multiple_of(C::PARAMS.epoch_length) {
            epoch_timestamp = header.inner.timestamp.into_inner();
        }
        ordered_timestamp_run = advance_ordered_timestamp_run(
            ordered_timestamp_run,
            previous_timestamp,
            header.inner.timestamp.into_inner(),
        );

        connected.push(ConnectedHeader::new(
            header.clone(),
            hash,
            height,
            cumulative_work,
            mtp,
            ordered_timestamp_run,
        ));
        prev_hash = hash;

        timestamps[timestamp_slot(height)] = header.inner.timestamp.into_inner();
    }

    connected
}

/// The tip after appending `connected` to `anchor`. Panics when `connected` is empty.
pub fn tip_from_connected<C: Consensus>(anchor: &Tip, connected: &[ConnectedHeader]) -> Tip {
    let last = connected.last().expect("non-empty slice");

    let mut timestamps = anchor.timestamps;
    let tail_start = connected.len().saturating_sub(WINDOW_SIZE);
    for ch in &connected[tail_start..] {
        timestamps[timestamp_slot(ch.height)] = ch.header.inner.timestamp.into_inner();
    }

    let epoch_length = C::PARAMS.epoch_length;
    let epoch_start = (last.height / epoch_length) * epoch_length;
    let (epoch_timestamp, (current_nbits, current_target, current_block_work)) = if epoch_start <= anchor.height {
        (
            anchor.epoch_timestamp,
            (anchor.current_nbits, anchor.current_target, anchor.current_block_work),
        )
    } else {
        let idx = (epoch_start - anchor.height - 1) as usize;
        (
            connected[idx].header.inner.timestamp.into_inner(),
            expand_target(last.header.inner.compact_target),
        )
    };

    Tip::new(
        last.block_hash,
        last.height,
        last.chainwork,
        timestamps,
        last.ordered_timestamp_run,
        epoch_timestamp,
        current_nbits,
        current_target,
        current_block_work,
    )
}
