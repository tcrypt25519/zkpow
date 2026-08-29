use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use zkpow_params::Mainnet;

use super::*;
use crate::flat::{write_snapshot, HeaderBinReader, HEADER_RECORD_SIZE};

fn temp_bin_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("zkpow-flat-{test_name}-{}-{nonce}.bin", std::process::id()))
}

#[test]
fn snapshot_round_trips_header_records() {
    let (store, db_path, _genesis_hash) = seeded_store("round-trip");
    let bin_path = temp_bin_path("round-trip");

    let count = store.header_count().unwrap();
    let records = store.load_header_records(0, count);
    write_snapshot(&bin_path, &records).unwrap();

    let meta = fs::metadata(&bin_path).unwrap();
    assert_eq!(meta.len(), count * HEADER_RECORD_SIZE as u64);

    let reader = HeaderBinReader::open(&bin_path).unwrap();
    assert_eq!(reader.header_count(), count);

    for height in 0..count {
        let from_db = store.load_header_record(height);
        let from_bin = reader.load_header_record(height);
        assert_eq!(from_db.height, from_bin.height, "height mismatch at {height}");
        assert_eq!(from_db.header.version, from_bin.header.version, "version mismatch at {height}");
        assert_eq!(
            from_db.header.prev_blockhash, from_bin.header.prev_blockhash,
            "prev_blockhash mismatch at {height}"
        );
        assert_eq!(
            from_db.header.merkle_root, from_bin.header.merkle_root,
            "merkle_root mismatch at {height}"
        );
        assert_eq!(
            from_db.header.timestamp, from_bin.header.timestamp,
            "timestamp mismatch at {height}"
        );
        assert_eq!(
            from_db.header.compact_target, from_bin.header.compact_target,
            "compact_target mismatch at {height}"
        );
        assert_eq!(from_db.header.nonce, from_bin.header.nonce, "nonce mismatch at {height}");
        assert_eq!(from_db.chain_work, from_bin.chain_work, "chain_work mismatch at {height}");
        assert_eq!(
            from_db.median_time_past, from_bin.median_time_past,
            "median_time_past mismatch at {height}"
        );
        assert_eq!(
            from_db.ordered_timestamp_run, from_bin.ordered_timestamp_run,
            "ordered_timestamp_run mismatch at {height}"
        );
    }

    remove_db_files(&db_path);
    fs::remove_file(&bin_path).ok();
}

#[test]
fn batch_witness_matches_between_db_and_flat_file() {
    let (store, db_path, _genesis_hash) = seeded_store("witness");
    let bin_path = temp_bin_path("witness");
    let count = store.header_count().unwrap();
    write_snapshot(&bin_path, &store.load_header_records(0, count)).unwrap();
    let reader = HeaderBinReader::open(&bin_path).unwrap();

    for (start, len) in [(1u64, 5u64), (6, 10), (1, 20)] {
        let from_db = store.load_header_batch_witness(start, len);
        let from_bin = reader.load_header_batch_witness(start, len);
        assert_eq!(from_db.headers, from_bin.headers, "headers mismatch for ({start}, {len})");
        assert_eq!(
            from_db.median_time_past_hints, from_bin.median_time_past_hints,
            "median hints mismatch for ({start}, {len})"
        );
    }

    remove_db_files(&db_path);
    fs::remove_file(&bin_path).ok();
}

#[test]
fn state_at_height_matches_between_db_and_flat_file() {
    let (store, db_path, genesis_hash) = seeded_store("state");
    let bin_path = temp_bin_path("state");
    let count = store.header_count().unwrap();
    write_snapshot(&bin_path, &store.load_header_records(0, count)).unwrap();
    let reader = HeaderBinReader::open(&bin_path).unwrap();

    for height in [0u32, 1, 10, 20] {
        let from_db = store.state_at_height::<Mainnet>(height, genesis_hash);
        let from_bin = reader.state_at_height::<Mainnet>(height, genesis_hash);
        assert_eq!(from_db, from_bin, "state mismatch at height {height}");
    }

    remove_db_files(&db_path);
    fs::remove_file(&bin_path).ok();
}

#[test]
fn load_header_records_clamps_to_available_range() {
    let (store, db_path, _genesis_hash) = seeded_store("clamp");
    let bin_path = temp_bin_path("clamp");
    let count = store.header_count().unwrap();
    write_snapshot(&bin_path, &store.load_header_records(0, count)).unwrap();
    let reader = HeaderBinReader::open(&bin_path).unwrap();

    let records = reader.load_header_records(count - 2, 100);
    assert_eq!(records.len(), 2, "should clamp to what's on disk, not error or pad");

    let records = reader.load_header_records(count + 5, 10);
    assert!(records.is_empty(), "requesting past the end should return nothing");

    remove_db_files(&db_path);
    fs::remove_file(&bin_path).ok();
}

#[test]
#[should_panic(expected = "not a multiple of the")]
fn open_rejects_truncated_file() {
    let bin_path = temp_bin_path("truncated");
    fs::write(&bin_path, vec![0u8; HEADER_RECORD_SIZE + 3]).unwrap();
    let _ = HeaderBinReader::open(&bin_path);
}
