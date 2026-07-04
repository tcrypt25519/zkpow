use zkpow_core::{check_proof_of_work, EPOCH_LENGTH, WINDOW_SIZE};

use crate::types::{current_difficulty, ConnectedHeader, Header, Tip};

pub fn connect_headers(anchor: &Tip, headers: &[Header]) -> Vec<ConnectedHeader> {
    let mut connected = Vec::with_capacity(headers.len());
    let mut prev_hash = anchor.hash;
    let mut height = anchor.height;
    let mut cumulative_work = anchor.chainwork;
    let mut timestamps = anchor.timestamps;

    // Cached exactly like `zkpow_core::State::{current_target,current_work}`:
    // carried forward from the anchor and only re-derived at a retarget
    // boundary (`candidate_height % EPOCH_LENGTH == 0`) — every header in
    // between carries the identical `compact_target`, so re-expanding the
    // full 256-bit target and its work on every single header instead of
    // once per 2016 is pure waste.
    let mut active_target = anchor.current_target;
    let mut active_work = anchor.current_work;

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

        if candidate_height.is_multiple_of(EPOCH_LENGTH as u64) {
            let (_, target, work) = current_difficulty(header.inner.compact_target);
            active_target = target;
            active_work = work;
        }

        if !check_proof_of_work(hash, active_target) {
            tracing::debug!(%hash, "PoW validation failed");
            break;
        }

        let window_len = zkpow_core::timestamp_count(height);
        let mut sorted = timestamps;
        sorted[..window_len].sort_unstable();
        let mtp = if window_len == 0 {
            0
        } else {
            sorted[window_len / 2]
        };

        if header.inner.timestamp.into_inner() <= mtp {
            tracing::debug!(%hash, timestamp = header.inner.timestamp.into_inner(), mtp, "timestamp not greater than MTP");
            break;
        }

        cumulative_work = cumulative_work + active_work;
        height = candidate_height;

        connected.push(ConnectedHeader::new(
            header.clone(),
            height,
            cumulative_work,
            mtp,
        ));
        prev_hash = hash;

        timestamps[height as usize % WINDOW_SIZE] = header.inner.timestamp.into_inner();
    }

    connected
}

pub fn tip_from_connected(anchor: &Tip, connected: &[ConnectedHeader]) -> Tip {
    let last = connected.last().expect("non-empty slice");

    // Only the last (up to) WINDOW_SIZE connected headers can still be in the
    // window — anything earlier has already been overwritten by a later
    // header landing in the same ring-buffer slot, so there's no need to walk
    // the whole (possibly long) `connected` slice to reconstruct it.
    let mut timestamps = anchor.timestamps;
    let tail_start = connected.len().saturating_sub(WINDOW_SIZE);
    for ch in &connected[tail_start..] {
        timestamps[ch.height as usize % WINDOW_SIZE] = ch.header.inner.timestamp.into_inner();
    }

    let (current_nbits, current_target, current_work) =
        current_difficulty(last.header.inner.compact_target);

    Tip::new(
        last.block_hash(),
        last.height,
        last.chainwork,
        timestamps,
        current_nbits,
        current_target,
        current_work,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zkpow_core::{
        target_from_bits, BlockHash, BlockTimestamp, CompactTarget, Header as CoreHeader, Target,
    };

    // The compact form's mantissa is masked to 23 bits (0x007fffff), so the
    // loosest target it can represent has only its single most-significant
    // byte set (0xFF, everything below zero) — about a 255/256 chance any
    // given hash satisfies it. Distinct mantissas (A vs B) give two distinct
    // `compact_target` values to exercise retarget cache invalidation.
    const TRIVIAL_NBITS_A: u32 = 0x227FFFFF;
    const TRIVIAL_NBITS_B: u32 = 0x227FFFFE;

    fn make_header(prev: BlockHash, nbits: u32, timestamp: u32, nonce: u32) -> Header {
        Header::new(CoreHeader {
            version: 1,
            prev_blockhash: prev,
            merkle_root: [0u8; 32],
            timestamp: BlockTimestamp::new(timestamp),
            compact_target: CompactTarget::new(nbits),
            nonce,
        })
    }

    /// Finds a nonce whose hash satisfies `validate_target`, for a header
    /// that *claims* `claimed_nbits` — these can differ, to test that
    /// validation uses the cached target rather than the header's own claim.
    /// At the (very loose, ~99.6%-of-hashes-pass) test targets this takes
    /// ~1 attempt, never more than a handful.
    fn mine_header_against(
        prev: BlockHash,
        claimed_nbits: u32,
        validate_target: Target,
        timestamp: u32,
    ) -> Header {
        (0u32..)
            .map(|nonce| make_header(prev, claimed_nbits, timestamp, nonce))
            .find(|header| check_proof_of_work(header.block_hash(), validate_target))
            .expect("a passing nonce exists well within u32 range at this difficulty")
    }

    fn mine_header(prev: BlockHash, nbits: u32, timestamp: u32) -> Header {
        mine_header_against(
            prev,
            nbits,
            target_from_bits(CompactTarget::new(nbits)),
            timestamp,
        )
    }

    fn build_chain(anchor: &Tip, count: u32, nbits_at: impl Fn(u32) -> u32) -> Vec<Header> {
        let mut prev = anchor.hash;
        let mut headers = Vec::with_capacity(count as usize);
        // Strictly after every timestamp already in the anchor's window, so
        // the first header always clears the median-time-past check.
        let base_time = anchor.timestamps.iter().copied().max().unwrap_or(0) + 600;
        for i in 0..count {
            let header = mine_header(prev, nbits_at(i), base_time + i * 600);
            prev = header.block_hash();
            headers.push(header);
        }
        headers
    }

    /// A synthetic anchor at `height` with `current_target`/`current_work`
    /// already cached from `nbits` — standing in for whatever a real `Tip`
    /// would carry after validating its way there, without having to mine
    /// `height` real headers to construct one.
    fn synthetic_anchor(height: u64, nbits: u32) -> Tip {
        let (current_nbits, current_target, current_work) =
            current_difficulty(CompactTarget::new(nbits));
        Tip::new(
            BlockHash::new([0xAB; 32]),
            height,
            current_work,
            [2_000_000_000u32; WINDOW_SIZE],
            current_nbits,
            current_target,
            current_work,
        )
    }

    #[test]
    fn connect_headers_wraps_the_timestamp_window_across_batches() {
        let anchor = synthetic_anchor(100, TRIVIAL_NBITS_A);
        // Longer than 2*WINDOW_SIZE so the ring buffer wraps more than once.
        let total = (WINDOW_SIZE as u32) * 2 + 3;
        let headers = build_chain(&anchor, total, |_| TRIVIAL_NBITS_A);

        let connected = connect_headers(&anchor, &headers);
        assert_eq!(
            connected.len(),
            total as usize,
            "every header should connect"
        );
        for (i, ch) in connected.iter().enumerate() {
            assert_eq!(ch.height, anchor.height + 1 + i as u64);
        }

        // Splitting the same chain into two batches through `tip_from_connected`
        // must produce the same final state as one single batch — this is what
        // actually exercises the ring-buffer hand-off between calls.
        let split = 7usize;
        let first_batch = connect_headers(&anchor, &headers[..split]);
        let mid_tip = tip_from_connected(&anchor, &first_batch);
        let second_batch = connect_headers(&mid_tip, &headers[split..]);
        assert_eq!(first_batch.len() + second_batch.len(), connected.len());

        let final_tip = tip_from_connected(&mid_tip, &second_batch);
        let one_shot_tip = tip_from_connected(&anchor, &connected);
        assert_eq!(final_tip.hash, one_shot_tip.hash);
        assert_eq!(final_tip.height, one_shot_tip.height);
        assert_eq!(final_tip.chainwork, one_shot_tip.chainwork);
        assert_eq!(final_tip.timestamps, one_shot_tip.timestamps);
    }

    #[test]
    fn connect_headers_only_re_derives_target_at_epoch_boundary() {
        // One header below a retarget boundary, cached on nbits A.
        let anchor = synthetic_anchor(EPOCH_LENGTH as u64 - 1, TRIVIAL_NBITS_A);
        assert_ne!(TRIVIAL_NBITS_A, TRIVIAL_NBITS_B);

        // The boundary header (candidate_height == EPOCH_LENGTH) claims a
        // *different* difficulty, B — this must be picked up, since this is
        // exactly the `height % EPOCH_LENGTH == 0` case.
        let boundary_header = mine_header(anchor.hash, TRIVIAL_NBITS_B, 2_000_010_000);
        let connected = connect_headers(&anchor, std::slice::from_ref(&boundary_header));
        assert_eq!(
            connected.len(),
            1,
            "boundary header should validate under its own (new) bits"
        );

        let (_, target_b, work_b) = current_difficulty(CompactTarget::new(TRIVIAL_NBITS_B));
        assert_eq!(
            connected[0].chainwork,
            anchor.chainwork + work_b,
            "chainwork must accumulate the *new* epoch's work, not the cached (stale) one"
        );

        let tip = tip_from_connected(&anchor, &connected);
        assert_eq!(tip.current_nbits, CompactTarget::new(TRIVIAL_NBITS_B));

        // One header *past* the boundary (mid-epoch again): even though it
        // claims yet another difficulty (A), that must be ignored — only the
        // cached value (now B, from the boundary header) governs validation
        // until the *next* boundary. Mined against B's target (what should
        // actually govern), not A's (what it dishonestly claims), so the
        // assertion below can only pass if the cache — not the header's own
        // bits — is what's actually checked.
        let mid_epoch_header =
            mine_header_against(tip.hash, TRIVIAL_NBITS_A, target_b, 2_000_020_000);
        let connected_mid = connect_headers(&tip, std::slice::from_ref(&mid_epoch_header));
        assert_eq!(connected_mid.len(), 1);
        assert_eq!(
            connected_mid[0].chainwork,
            tip.chainwork + work_b,
            "mid-epoch header must accumulate the cached epoch's work regardless of its own claimed bits"
        );
    }

    /// Hand-derivable ground truth, independent of `zkpow_core::timestamp_count`
    /// or any of this module's own logic: an anchor at height 0 with exactly
    /// one known timestamp (`base`, its own), followed by headers timestamped
    /// `base+1, base+2, ...` — strictly increasing by 1, so the window is
    /// always already sorted and `median = base + (height_before_candidate / 2)`
    /// up through height 11, after which slot 0 (which held `base`) gets
    /// overwritten and the window becomes `{base+1 ..= base+11}` for height 12.
    #[test]
    fn median_time_past_matches_hand_computed_values_through_a_wrap() {
        let base = 2_000_000_000u32;
        let (current_nbits, current_target, current_work) =
            current_difficulty(CompactTarget::new(TRIVIAL_NBITS_A));
        let mut timestamps = [0u32; WINDOW_SIZE];
        timestamps[0] = base;
        let anchor = Tip::new(
            BlockHash::new([0xCD; 32]),
            0,
            current_work,
            timestamps,
            current_nbits,
            current_target,
            current_work,
        );

        // At height 0 there is exactly 1 valid timestamp (the anchor's own) —
        // not 0, since the anchor *is* a recorded block with a known timestamp.
        assert_eq!(zkpow_core::timestamp_count(anchor.height), 1);

        // Increments of exactly 1 second (not `build_chain`'s 600s) so the
        // expected medians below are trivial to verify by hand.
        let mut prev = anchor.hash;
        let mut headers = Vec::with_capacity(15);
        for i in 1..=15u32 {
            let header = mine_header(prev, TRIVIAL_NBITS_A, base + i);
            prev = header.block_hash();
            headers.push(header);
        }
        let connected = connect_headers(&anchor, &headers);
        assert_eq!(connected.len(), 15);

        let expected_mtp = |height: u64| -> u32 {
            let h = height as u32;
            if height <= WINDOW_SIZE as u64 {
                // Window = {base, base+1, ..., base+(h-1)}, ascending, h
                // values; `sorted[window_len / 2]` picks index h/2.
                base + h / 2
            } else {
                // Past the first wrap: window = the *previous* WINDOW_SIZE
                // timestamps, {base+(h-WINDOW_SIZE) ..= base+(h-1)}, ascending;
                // median is at index WINDOW_SIZE/2 within that.
                base + (h - WINDOW_SIZE as u32) + (WINDOW_SIZE as u32 / 2)
            }
        };

        for (i, ch) in connected.iter().enumerate() {
            let height = i as u64 + 1;
            assert_eq!(
                ch.median_time_past,
                expected_mtp(height),
                "height {height}: median_time_past"
            );
        }

        // Spot-check the exact wrap boundary by hand, not just via the closed
        // form above: height 12 is the first candidate validated *after*
        // slot 0 (holding `base`) was overwritten by height 11's timestamp.
        // Its window is {base+1, base+2, ..., base+11} — `base` has fully
        // aged out — so the median is base+6, not base+5.
        assert_eq!(connected[10].height, 11);
        assert_eq!(connected[10].median_time_past, base + 5);
        assert_eq!(connected[11].height, 12);
        assert_eq!(connected[11].median_time_past, base + 6);
    }
}
