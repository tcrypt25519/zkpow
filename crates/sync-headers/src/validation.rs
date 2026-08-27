use zkpow_core::{
    advance_ordered_timestamp_run, check_proof_of_work, has_ordered_timestamp_window, ordered_median_timestamp_slot,
    timestamp_slot,
};
use zkpow_params::{Consensus, Mainnet};

use crate::types::{expand_target, ConnectedHeader, Header, Tip};

pub fn connect_headers<C: Consensus>(anchor: &Tip, headers: &[Header]) -> Vec<ConnectedHeader> {
    let mut connected = Vec::with_capacity(headers.len());
    let mut prev_hash = anchor.hash;
    let mut height = anchor.height;
    let mut cumulative_work = anchor.chainwork;
    let mut timestamps = anchor.timestamps;
    let mut ordered_timestamp_run = anchor.ordered_timestamp_run;

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

        let hash = header.block_hash();
        let candidate_height = height + 1;
        let previous_timestamp = timestamps[timestamp_slot(height)];

        if candidate_height.is_multiple_of(C::PARAMS.epoch_length as u64) {
            let (_, target, work) = expand_target(header.inner.compact_target);
            active_target = target;
            active_work = work;
        }

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

        cumulative_work = cumulative_work + active_work;
        height = candidate_height;
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

pub fn tip_from_connected(anchor: &Tip, connected: &[ConnectedHeader]) -> Tip {
    let last = connected.last().expect("non-empty slice");

    let mut timestamps = anchor.timestamps;
    let tail_start = connected.len().saturating_sub(Mainnet::PARAMS.window_size);
    for ch in &connected[tail_start..] {
        timestamps[timestamp_slot(ch.height)] = ch.header.inner.timestamp.into_inner();
    }

    let (current_nbits, current_target, current_block_work) = expand_target(last.header.inner.compact_target);

    Tip::new(
        last.block_hash,
        last.height,
        last.chainwork,
        timestamps,
        last.ordered_timestamp_run,
        current_nbits,
        current_target,
        current_block_work,
    )
}
