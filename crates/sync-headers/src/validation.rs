use zkpow_core::{calculate_next_work_required, check_proof_of_work};
use zkpow_mtp::{Window, WindowState};
use zkpow_params::Consensus;
use zkpow_types::{BlockHash, BlockTimestamp, TargetTriplet};

use crate::types::{ConnectedHeader, Header, Tip};

pub enum ValidationError {
    ChainBroken,
    CompactTargetMismatch,
    ProofOfWorkFailed,
    TimestampNotGreater,
}

/// Append the valid header prefix to `tip` and return its connected headers.
pub fn connect_headers<'a, C: Consensus>(
    tip: &mut Tip,
    headers: impl IntoIterator<Item = &'a Header>,
) -> Vec<ConnectedHeader> {
    let mut connected = Vec::new();
    let mut headers = headers.into_iter();
    let mut ring = match tip.timestamps {
        WindowState::Cold(mut ring) => {
            let cold_headers = ring.take_cold_headers(headers.by_ref());
            if !connect_batch::<C, _>(tip, &mut ring, cold_headers, &mut connected, |ring| ring.median()) {
                tip.timestamps = WindowState::Cold(ring);
                return connected;
            }
            let Some(warm) = ring.warm() else {
                tip.timestamps = WindowState::Cold(ring);
                return connected;
            };
            warm
        },
        WindowState::Warm(ring) => ring,
    };
    connect_batch::<C, _>(tip, &mut ring, headers, &mut connected, |ring| ring.median_by_sort());
    tip.timestamps = WindowState::Warm(ring);
    connected
}

fn connect_batch<'a, C: Consensus, F>(
    tip: &mut Tip,
    ring: &mut Window<F>,
    headers: impl Iterator<Item = &'a Header>,
    connected: &mut Vec<ConnectedHeader>,
    median: impl Fn(&Window<F>) -> BlockTimestamp,
) -> bool {
    for header in headers {
        let Ok((hash, target_triplet, mtp)) = validate_header::<C>(tip, header, ring.newest(), || median(ring)) else {
            return false;
        };
        ring.push(header.inner.timestamp);
        tip.chainwork += target_triplet.block_work();
        tip.height += 1;
        tip.target_triplet = target_triplet;
        if tip.height.is_multiple_of(C::PARAMS.epoch_length) {
            tip.epoch_timestamp = header.inner.timestamp.into_inner();
        }
        connected.push(ConnectedHeader::new(header.clone(), hash, tip.height, tip.chainwork, mtp));
        tip.hash = hash;
    }
    true
}

fn validate_header<C: Consensus>(
    tip: &Tip,
    header: &Header,
    previous_timestamp: BlockTimestamp,
    median: impl FnOnce() -> BlockTimestamp,
) -> Result<(BlockHash, TargetTriplet, u32), ValidationError> {
    if header.prev_blockhash() != tip.hash {
        tracing::debug!(
            expected = %tip.hash,
            got = %header.prev_blockhash(),
            "chain broken: prev_hash mismatch"
        );
        return Err(ValidationError::ChainBroken);
    }

    let candidate_height = tip.height + 1;

    let target_triplet = if candidate_height.is_multiple_of(C::PARAMS.epoch_length) {
        calculate_next_work_required::<C>(
            tip.target_triplet.target(),
            BlockTimestamp::new(tip.epoch_timestamp),
            previous_timestamp,
        )
    } else {
        tip.target_triplet
    };

    if header.inner.compact_target != target_triplet.compact_target() {
        tracing::debug!(
            expected = ?target_triplet.compact_target(),
            got = ?header.inner.compact_target,
            "compact_target mismatch"
        );
        return Err(ValidationError::CompactTargetMismatch);
    }

    let hash = header.block_hash();
    if !check_proof_of_work(hash, &target_triplet.target()) {
        tracing::debug!(%hash, "PoW validation failed");
        return Err(ValidationError::ProofOfWorkFailed);
    }

    let mtp = median().into_inner();

    if header.inner.timestamp.into_inner() <= mtp {
        tracing::debug!(%hash, timestamp = header.inner.timestamp.into_inner(), mtp, "timestamp not greater than MTP");
        return Err(ValidationError::TimestampNotGreater);
    };

    // hash, target_triple,

    Ok((hash, target_triplet, mtp))
}
