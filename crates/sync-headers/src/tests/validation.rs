use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use bitcoin::consensus::{encode, Encodable};
use zkpow_core::{calculate_next_work_required, check_proof_of_work, target_from_bits};
use zkpow_mtp::{WINDOW_MAX_INDEX, WINDOW_SIZE};
use zkpow_params::{Consensus, ConsensusParams, Mainnet};
use zkpow_types::{BlockHash, BlockTimestamp, BlockWork, CompactTarget, Header as CoreHeader, MerkleRoot, Target};

use super::*;
use crate::{
    connection::{create_checksum, VersionMessage},
    msg::MAX_HEADERS_PER_MSG,
};

const TRIVIAL_NBITS_A: u32 = 0x2007FFFF;
const TRIVIAL_NBITS_B: u32 = 0x2003FFFF;

struct TrivialConsensus;

impl Consensus for TrivialConsensus {
    const PARAMS: ConsensusParams = ConsensusParams {
        genesis:             CoreHeader {
            version:        1,
            prev_blockhash: BlockHash::new([0u8; 32]),
            merkle_root:    MerkleRoot::new([0u8; 32]),
            timestamp:      BlockTimestamp::new(2_000_000_000),
            compact_target: CompactTarget::new(TRIVIAL_NBITS_A),
            nonce:          0,
        },
        genesis_target:      Target::from_limbs([0, 0, 0, 0x07FF_FF00_0000_0000]),
        genesis_work:        BlockWork::from_limbs([32, 0, 0, 0]),
        block_time:          1,
        epoch_length:        4,
        max_epoch_over_mul:  4,
        min_epoch_under_div: 4,
    };
}

fn make_header(prev: BlockHash, nbits: u32, timestamp: u32, nonce: u32) -> Header {
    Header::new(CoreHeader {
        version: 1,
        prev_blockhash: prev,
        merkle_root: MerkleRoot::new([0u8; 32]),
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

/// A synthetic anchor at `height` with its target triplet already cached
/// from `nbits`, matching what a `Tip` holds after validating
/// that far without mining `height` real headers.
fn synthetic_anchor(height: u32, nbits: u32) -> Tip {
    let target_triplet = expand_target(CompactTarget::new(nbits));
    let chainwork = target_triplet.block_work() * (height + 1);
    let ordered_run = (height as u64).min(WINDOW_MAX_INDEX as u64);
    Tip::new(
        BlockHash::new([0xAB; 32]),
        height,
        chainwork,
        [2_000_000_000u32; WINDOW_SIZE],
        ordered_run,
        2_000_000_000,
        target_triplet,
    )
}

#[test]
fn connect_headers_wraps_the_timestamp_window_across_batches() {
    let anchor = synthetic_anchor(100, TRIVIAL_NBITS_A);
    // Longer than 2*WINDOW_SIZE so the ring buffer wraps more than once.
    let total = (WINDOW_SIZE as u32) * 2 + 3;
    let headers = build_chain(&anchor, total, |_| TRIVIAL_NBITS_A);

    let mut one_shot_tip = anchor.clone();
    let connected = connect_headers::<Mainnet>(&mut one_shot_tip, &headers);
    assert_eq!(connected.len(), total as usize, "every header should connect");
    for (i, ch) in connected.iter().enumerate() {
        assert_eq!(ch.height, anchor.height + 1 + i as u32);
    }

    // Splitting the chain must produce the same tip as one batch.
    let split = 7usize;
    let mut final_tip = anchor.clone();
    let first_batch = connect_headers::<Mainnet>(&mut final_tip, &headers[..split]);
    let second_batch = connect_headers::<Mainnet>(&mut final_tip, &headers[split..]);
    assert_eq!(first_batch.len() + second_batch.len(), connected.len());

    assert_eq!(final_tip.hash, one_shot_tip.hash);
    assert_eq!(final_tip.height, one_shot_tip.height);
    assert_eq!(final_tip.chainwork, one_shot_tip.chainwork);
    assert_eq!(final_tip.timestamps, one_shot_tip.timestamps);
    assert_eq!(final_tip.epoch_timestamp, one_shot_tip.epoch_timestamp);
}

#[test]
fn connect_headers_only_re_derives_target_at_epoch_boundary() {
    let epoch_len = TrivialConsensus::PARAMS.epoch_length;
    let mut anchor = synthetic_anchor(epoch_len - 1, TRIVIAL_NBITS_A);
    anchor.epoch_timestamp = 2_000_000_000 - (TrivialConsensus::PARAMS.expected_epoch_time() / 2);

    let (nbits_b, target_b, work_b) = calculate_next_work_required::<TrivialConsensus>(
        anchor.target_triplet.target(),
        BlockTimestamp::new(anchor.epoch_timestamp),
        BlockTimestamp::new(2_000_000_000),
    )
    .components();
    assert_eq!(nbits_b, CompactTarget::new(TRIVIAL_NBITS_B));

    let wrong_boundary = mine_header_against(anchor.hash, TRIVIAL_NBITS_A, target_b, 2_000_010_000);
    let mut rejected_tip = anchor.clone();
    assert!(
        connect_headers::<TrivialConsensus>(&mut rejected_tip, std::slice::from_ref(&wrong_boundary)).is_empty(),
        "boundary header with un-retargeted nbits must be rejected"
    );
    assert_eq!(rejected_tip.target_triplet, anchor.target_triplet);

    let boundary_header = mine_header(anchor.hash, TRIVIAL_NBITS_B, 2_000_010_000);
    let mut tip = anchor.clone();
    let connected = connect_headers::<TrivialConsensus>(&mut tip, std::slice::from_ref(&boundary_header));
    assert_eq!(connected.len(), 1, "boundary header should validate under retargeted bits");
    assert_eq!(
        connected[0].chainwork,
        anchor.chainwork + work_b,
        "chainwork must accumulate the new epoch's work"
    );

    assert_eq!(tip.target_triplet.compact_target(), CompactTarget::new(TRIVIAL_NBITS_B));
    assert_eq!(tip.epoch_timestamp, 2_000_010_000);

    let mismatched_mid = mine_header_against(tip.hash, TRIVIAL_NBITS_A, target_b, 2_000_020_000);
    let mut rejected_mid = tip.clone();
    assert!(
        connect_headers::<TrivialConsensus>(&mut rejected_mid, std::slice::from_ref(&mismatched_mid)).is_empty(),
        "mid-epoch header claiming wrong nbits must be rejected"
    );

    let valid_mid = mine_header(tip.hash, TRIVIAL_NBITS_B, 2_000_020_000);
    let mut two_step_tip = tip.clone();
    let connected_mid = connect_headers::<TrivialConsensus>(&mut two_step_tip, std::slice::from_ref(&valid_mid));
    assert_eq!(connected_mid.len(), 1);
    assert_eq!(connected_mid[0].chainwork, tip.chainwork + work_b);

    let both = [boundary_header, valid_mid];
    let mut one_shot_tip = anchor;
    connect_headers::<TrivialConsensus>(&mut one_shot_tip, &both);
    assert_eq!(two_step_tip.hash, one_shot_tip.hash);
    assert_eq!(two_step_tip.chainwork, one_shot_tip.chainwork);
    assert_eq!(two_step_tip.epoch_timestamp, one_shot_tip.epoch_timestamp);
    assert_eq!(two_step_tip.target_triplet, one_shot_tip.target_triplet);
}

/// An anchor at height 0 with one known timestamp (`base`), followed by
/// headers timestamped `base+1, base+2, ...`, strictly increasing so the
/// window is always sorted and `median = base + height/2` through height
/// 11. At height 12 slot 0 (holding `base`) has been overwritten and the
/// window is `{base+1 ..= base+11}`.
#[test]
fn median_time_past_matches_hand_computed_values_through_a_wrap() {
    let base = 2_000_000_000u32;
    let target_triplet = expand_target(CompactTarget::new(TRIVIAL_NBITS_A));
    let chainwork = target_triplet.block_work() * 1u32;
    let mut timestamps = [0u32; WINDOW_SIZE];
    timestamps[0] = base;
    let mut anchor = Tip::new(BlockHash::new([0xCD; 32]), 0, chainwork, timestamps, 0, base, target_triplet);

    // 1-second increments so the expected medians are easy to check by hand.
    let mut prev = anchor.hash;
    let mut headers = Vec::with_capacity(15);
    for i in 1..=15u32 {
        let header = mine_header(prev, TRIVIAL_NBITS_A, base + i);
        prev = header.block_hash();
        headers.push(header);
    }
    let connected = connect_headers::<Mainnet>(&mut anchor, &headers);
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
    let target_triplet = expand_target(CompactTarget::new(TRIVIAL_NBITS_A));
    let chainwork = target_triplet.block_work() * 11u32;
    let mut anchor = Tip::new(
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
        base,
        target_triplet,
    );

    let regressing_header = mine_header(anchor.hash, TRIVIAL_NBITS_A, base + 7);
    let connected = connect_headers::<Mainnet>(&mut anchor, std::slice::from_ref(&regressing_header));

    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].ordered_timestamp_run, 0);
    assert_eq!(connected[0].median_time_past, base + 5);
}

#[test]
fn ordered_window_uses_ring_median_without_changing_result() {
    let base = 2_000_000_000u32;
    let target_triplet = expand_target(CompactTarget::new(TRIVIAL_NBITS_A));
    let chainwork = target_triplet.block_work() * 11u32;
    let mut anchor = Tip::new(
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
        base,
        target_triplet,
    );
    let header = mine_header(anchor.hash, TRIVIAL_NBITS_A, base + 11);

    let connected = connect_headers::<Mainnet>(&mut anchor, std::slice::from_ref(&header));
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].median_time_past, base + 5);
}

#[test]
fn version_message_rejects_oversized_user_agent() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&70015i32.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&0i64.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&[0u8; 18]);
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&[0u8; 18]);
    payload.extend_from_slice(&0u64.to_le_bytes());
    encode::VarInt(257).consensus_encode(&mut payload).unwrap();
    payload.extend(std::iter::repeat_n(b'a', 257));
    payload.extend_from_slice(&0i32.to_le_bytes());

    assert!(matches!(
        VersionMessage::decode(&payload),
        Err(ConnectionError::UserAgentTooLong { length: 257, limit: 256 })
    ));
}

#[test]
fn peer_store_adds_peer_and_exhausts_unreachable_candidates() {
    let addr: std::net::SocketAddr = "127.0.0.1:8333".parse().unwrap();
    let mut store = PeerStore::default();
    store.add_peer(addr);
    store.add_peer(addr);
    assert_eq!(store.len(), 1);

    let mut rng = rand::rng();
    assert_eq!(store.pick_candidate(&mut rng), Some(addr));
    store.mark_unreachable(&addr);
    assert_eq!(store.pick_candidate(&mut rng), None);
}

fn write_p2p_frame(stream: &mut std::net::TcpStream, cmd: &[u8], payload: &[u8]) {
    let mut cmd_buf = [0u8; 12];
    cmd_buf[..cmd.len()].copy_from_slice(cmd);
    let mut frame = Vec::with_capacity(24 + payload.len());
    frame.extend_from_slice(&[0xF9, 0xBE, 0xB4, 0xD9]);
    frame.extend_from_slice(&cmd_buf);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&create_checksum(payload));
    frame.extend_from_slice(payload);
    stream.write_all(&frame).unwrap();
}

fn read_p2p_frame(stream: &mut std::net::TcpStream) -> ([u8; 12], Vec<u8>) {
    let mut hdr = [0u8; 24];
    stream.read_exact(&mut hdr).unwrap();
    let mut cmd = [0u8; 12];
    cmd.copy_from_slice(&hdr[4..16]);
    let len = u32::from_le_bytes(hdr[16..20].try_into().unwrap()) as usize;
    let mut payload = vec![0u8; len];
    if len > 0 {
        stream.read_exact(&mut payload).unwrap();
    }
    (cmd, payload)
}

fn accept_handshake(listener: &TcpListener) -> std::net::TcpStream {
    let (mut stream, _) = listener.accept().unwrap();
    let _ = read_p2p_frame(&mut stream);
    let mut ver = Vec::new();
    ver.extend_from_slice(&70015i32.to_le_bytes());
    ver.extend_from_slice(&0u64.to_le_bytes());
    ver.extend_from_slice(&0i64.to_le_bytes());
    ver.extend_from_slice(&0u64.to_le_bytes());
    ver.extend_from_slice(&[0u8; 18]);
    ver.extend_from_slice(&0u64.to_le_bytes());
    ver.extend_from_slice(&[0u8; 18]);
    ver.extend_from_slice(&0u64.to_le_bytes());
    ver.push(0);
    ver.extend_from_slice(&0i32.to_le_bytes());
    ver.push(0);
    write_p2p_frame(&mut stream, b"version", &ver);
    let _ = read_p2p_frame(&mut stream);
    write_p2p_frame(&mut stream, b"verack", &[]);
    let _ = read_p2p_frame(&mut stream);
    stream
}

#[test]
fn peer_connection_rejects_oversized_frame_and_header_count() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_handshake(&listener);
        let mut frame = Vec::new();
        frame.extend_from_slice(&[0xF9, 0xBE, 0xB4, 0xD9]);
        frame.extend_from_slice(b"headers\0\0\0\0\0");
        frame.extend_from_slice(&(4_000_001u32).to_le_bytes());
        frame.extend_from_slice(&[0u8; 4]);
        stream.write_all(&frame).unwrap();
    });
    let mut conn = PeerConnection::connect(addr, Network::Mainnet, 0).unwrap();
    let err = conn.read_headers().unwrap_err();
    assert!(matches!(err, ConnectionError::MessageTooLarge {
        length: 4_000_001,
        limit:  4_000_000,
    }));
    server.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_handshake(&listener);
        write_p2p_frame(&mut stream, b"ping", &[1, 2, 3, 4, 5, 6, 7, 8]);
        let (cmd, pong) = read_p2p_frame(&mut stream);
        assert_eq!(&cmd[..4], b"pong");
        assert_eq!(pong, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut bad_headers = Vec::new();
        encode::VarInt((MAX_HEADERS_PER_MSG + 1) as u64)
            .consensus_encode(&mut bad_headers)
            .unwrap();
        write_p2p_frame(&mut stream, b"headers", &bad_headers);
    });
    let mut conn = PeerConnection::connect(addr, Network::Mainnet, 0).unwrap();
    let err = conn.read_headers().unwrap_err();
    assert!(matches!(
        err,
        ConnectionError::TooManyHeaders { count, limit } if count == (MAX_HEADERS_PER_MSG + 1) as u64 && limit == MAX_HEADERS_PER_MSG
    ));
    server.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_handshake(&listener);
        for _ in 0..=64 {
            write_p2p_frame(&mut stream, b"sendaddrv2", &[]);
        }
    });
    let mut conn = PeerConnection::connect(addr, Network::Mainnet, 0).unwrap();
    let err = conn.read_headers().unwrap_err();
    assert!(matches!(err, ConnectionError::TooManyUnsolicitedMessages {
        expected: Command::Headers,
        limit:    64,
    }));
    server.join().unwrap();
}
