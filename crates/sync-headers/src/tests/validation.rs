use zkpow_core::{
    check_proof_of_work, target_from_bits, BlockHash, BlockTimestamp, CompactTarget, Header as CoreHeader, Target,
    EPOCH_LENGTH, WINDOW_SIZE,
};

use super::*;

// The 23-bit mantissa caps the expressible target just under 2^256, so a
// random hash almost always passes. A and B differ only in the mantissa to
// exercise retarget cache invalidation.
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

/// Finds a nonce whose hash satisfies `validate_target` while the header
/// claims `claimed_nbits`; the two may differ to test that validation uses
/// the cached target rather than the header's own claim. At these test
/// targets a passing nonce is found in a few attempts.
fn mine_header_against(prev: BlockHash, claimed_nbits: u32, validate_target: Target, timestamp: u32) -> Header {
    (0u32..)
        .map(|nonce| make_header(prev, claimed_nbits, timestamp, nonce))
        .find(|header| check_proof_of_work(header.block_hash(), &validate_target))
        .expect("a passing nonce exists well within u32 range at this difficulty")
}

fn mine_header(prev: BlockHash, nbits: u32, timestamp: u32) -> Header {
    mine_header_against(prev, nbits, target_from_bits(CompactTarget::new(nbits)), timestamp)
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

/// A synthetic anchor at `height` with `current_target`/`current_block_work`
/// already cached from `nbits` — standing in for whatever a real `Tip`
/// would carry after validating its way there, without having to mine
/// `height` real headers to construct one.
fn synthetic_anchor(height: u64, nbits: u32) -> Tip {
    let (current_nbits, current_target, current_block_work) = expand_target(CompactTarget::new(nbits));
    let chainwork = current_block_work * ((height + 1) as u32);
    Tip::new(
        BlockHash::new([0xAB; 32]),
        height,
        chainwork,
        [2_000_000_000u32; WINDOW_SIZE],
        height,
        current_nbits,
        current_target,
        current_block_work,
    )
}

#[test]
fn connect_headers_wraps_the_timestamp_window_across_batches() {
    let anchor = synthetic_anchor(100, TRIVIAL_NBITS_A);
    // Longer than 2*WINDOW_SIZE so the ring buffer wraps more than once.
    let total = (WINDOW_SIZE as u32) * 2 + 3;
    let headers = build_chain(&anchor, total, |_| TRIVIAL_NBITS_A);

    let connected = connect_headers(&anchor, &headers);
    assert_eq!(connected.len(), total as usize, "every header should connect");
    for (i, ch) in connected.iter().enumerate() {
        assert_eq!(ch.height, anchor.height + 1 + i as u64);
    }

    // Splitting the same chain into two batches through `tip_from_connected`
    // must produce the same final state as one single batch. This is what
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
    // *different* difficulty, B. This must be picked up, since this is
    // exactly the `height % EPOCH_LENGTH == 0` case.
    let boundary_header = mine_header(anchor.hash, TRIVIAL_NBITS_B, 2_000_010_000);
    let connected = connect_headers(&anchor, std::slice::from_ref(&boundary_header));
    assert_eq!(connected.len(), 1, "boundary header should validate under its own (new) bits");

    let (_, target_b, work_b) = expand_target(CompactTarget::new(TRIVIAL_NBITS_B));
    assert_eq!(
        connected[0].chainwork,
        anchor.chainwork + work_b,
        "chainwork must accumulate the *new* epoch's work, not the cached (stale) one"
    );

    let tip = tip_from_connected(&anchor, &connected);
    assert_eq!(tip.current_nbits, CompactTarget::new(TRIVIAL_NBITS_B));

    // Mid-epoch again: the header claims A but the cached value (B, from
    // the boundary header) must govern until the next boundary. The header
    // is mined against B's target, so the assertion below passes only if
    // validation reads the cache rather than the header's own bits.
    let mid_epoch_header = mine_header_against(tip.hash, TRIVIAL_NBITS_A, target_b, 2_000_020_000);
    let connected_mid = connect_headers(&tip, std::slice::from_ref(&mid_epoch_header));
    assert_eq!(connected_mid.len(), 1);
    assert_eq!(
        connected_mid[0].chainwork,
        tip.chainwork + work_b,
        "mid-epoch header must accumulate the cached epoch's work regardless of its own claimed bits"
    );
}

/// An anchor at height 0 with one known timestamp (`base`), followed by
/// headers timestamped `base+1, base+2, ...`, strictly increasing so the
/// window is always sorted and `median = base + height/2` through height
/// 11. At height 12 slot 0 (holding `base`) has been overwritten and the
/// window is `{base+1 ..= base+11}`.
#[test]
fn median_time_past_matches_hand_computed_values_through_a_wrap() {
    let base = 2_000_000_000u32;
    let (current_nbits, current_target, current_block_work) = expand_target(CompactTarget::new(TRIVIAL_NBITS_A));
    let chainwork = current_block_work * 1u32;
    let mut timestamps = [0u32; WINDOW_SIZE];
    timestamps[0] = base;
    let anchor = Tip::new(
        BlockHash::new([0xCD; 32]),
        0,
        chainwork,
        timestamps,
        0,
        current_nbits,
        current_target,
        current_block_work,
    );

    // At height 0 the count is 1, not 0: the anchor itself is a recorded
    // block with a known timestamp.
    assert_eq!(zkpow_core::timestamp_count(anchor.height), 1);

    // 1-second increments so the expected medians are easy to check by hand.
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
        assert_eq!(ch.median_time_past, expected_mtp(height), "height {height}: median_time_past");
        assert_eq!(ch.ordered_timestamp_run, height, "height {height}: ordered timestamp run");
    }

    // Height 12 is the first candidate after slot 0 was overwritten by
    // height 11's timestamp: `base` has aged out, so the median is base+6,
    // not base+5.
    assert_eq!(connected[10].height, 11);
    assert_eq!(connected[10].median_time_past, base + 5);
    assert_eq!(connected[11].height, 12);
    assert_eq!(connected[11].median_time_past, base + 6);
}

#[test]
fn connect_headers_resets_ordered_timestamp_run_on_timestamp_regression() {
    let base = 2_000_000_000u32;
    let (current_nbits, current_target, current_block_work) = expand_target(CompactTarget::new(TRIVIAL_NBITS_A));
    let chainwork = current_block_work * 11u32;
    let anchor = Tip::new(
        BlockHash::new([0xEF; 32]),
        10,
        chainwork,
        [
            base,
            base + 1,
            base + 2,
            base + 3,
            base + 4,
            base + 5,
            base + 6,
            base + 7,
            base + 8,
            base + 9,
            base + 10,
        ],
        10,
        current_nbits,
        current_target,
        current_block_work,
    );

    let regressing_header = mine_header(anchor.hash, TRIVIAL_NBITS_A, base + 7);
    let connected = connect_headers(&anchor, std::slice::from_ref(&regressing_header));

    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].ordered_timestamp_run, 0);
    assert_eq!(connected[0].median_time_past, base + 5);
}

#[test]
fn ordered_window_uses_ring_median_without_changing_result() {
    let base = 2_000_000_000u32;
    let (current_nbits, current_target, current_block_work) = expand_target(CompactTarget::new(TRIVIAL_NBITS_A));
    let chainwork = current_block_work * 11u32;
    let anchor = Tip::new(
        BlockHash::new([0x9A; 32]),
        10,
        chainwork,
        [
            base,
            base + 1,
            base + 2,
            base + 3,
            base + 4,
            base + 5,
            base + 6,
            base + 7,
            base + 8,
            base + 9,
            base + 10,
        ],
        10,
        current_nbits,
        current_target,
        current_block_work,
    );
    let header = mine_header(anchor.hash, TRIVIAL_NBITS_A, base + 11);

    let connected = connect_headers(&anchor, std::slice::from_ref(&header));
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].median_time_past, base + 5);
}
