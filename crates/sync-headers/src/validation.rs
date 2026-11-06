use zkpow_core::{calculate_next_work_required, check_proof_of_work};
use zkpow_mtp::{advance_ordered_timestamp_run, median_timestamp, record_timestamp, timestamp_at};
use zkpow_params::Consensus;
use zkpow_types::BlockTimestamp;

use crate::types::{ConnectedHeader, Header, Tip};

pub enum ValidationError {
    ChainBroken,
    CompactTargetMismatch,
    ProofOfWorkFailed,
    TimestampNotGreater,
}

/// Append the valid header prefix to `tip` and return its connected headers.
pub fn connect_headers<C: Consensus>(tip: &mut Tip, headers: impl Iterator<Item = Header>) -> Vec<ConnectedHeader> {
    let mut connected = Vec::new();

    for header in headers {
        validate_header::<C>(tip, &header)?;

        tip.chainwork += target_triplet.block_work();
        tip.height += 1;
        tip.target_triplet = target_triplet;
        if tip.height.is_multiple_of(C::PARAMS.epoch_length) {
            tip.epoch_timestamp = header.inner.timestamp.into_inner();
        }
        tip.ordered_timestamp_run = advance_ordered_timestamp_run(
            tip.ordered_timestamp_run,
            tip.timestamps(),
            header.inner.timestamp.into_inner(),
        );

        let connected_header =
            ConnectedHeader::new(header.clone(), hash, tip.height, tip.chainwork, mtp, tip.ordered_timestamp_run);
        tip.hash = hash;
        record_timestamp(&mut tip.timestamps, tip.height, header.inner.timestamp.into_inner());
    }
    connected
}

pub fn validate_header<C: Consensus>(tip: &mut Tip, header: &Header) -> Result<(), ValidationError> {
    if header.prev_blockhash() != tip.hash {
        tracing::debug!(
            expected = %tip.hash,
            got = %header.prev_blockhash(),
            "chain broken: prev_hash mismatch"
        );
        return Err(ValidationError::ChainBroken);
    }

    let candidate_height = tip.height + 1;
    let previous_timestamp = timestamp_at(&tip.timestamps, tip.height);

    let target_triplet = if candidate_height.is_multiple_of(C::PARAMS.epoch_length) {
        calculate_next_work_required::<C>(
            tip.target_triplet.target(),
            BlockTimestamp::new(tip.epoch_timestamp),
            BlockTimestamp::new(previous_timestamp),
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

    let mtp = median_timestamp(&tip.timestamps, tip.height, tip.ordered_timestamp_run);

    if header.inner.timestamp.into_inner() <= mtp {
        tracing::debug!(%hash, timestamp = header.inner.timestamp.into_inner(), mtp, "timestamp not greater than MTP");
        return Err(ValidationError::TimestampNotGreater);
    };

    // hash, target_triple,

    Ok(())
}
