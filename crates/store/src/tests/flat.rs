use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use sync_headers::{ConnectedHeader, Header as SyncHeader};
use zkpow_core::{block_work_from_target, target_from_bits, BlockHash, BlockTimestamp};
use zkpow_params::{Consensus, Mainnet};

use super::*;
use crate::flat::{write_snapshot, HeaderBinReader, HEADER_RECORD_SIZE};

fn temp_db_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("zkpow-flat-{test_name}-{}-{nonce}.sqlite", std::process::id()))
}

fn temp_bin_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("zkpow-flat-{test_name}-{}-{nonce}.bin", std::process::id()))
}

fn remove_db_files(path: &std::path::Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(path.with_extension("sqlite-shm"));
    let _ = fs::remove_file(path.with_extension("sqlite-wal"));
}

// TODO: This repeats db.rs (bulk_insert_calculates_median_time_past) and probably 5 other places.
/// A genesis-seeded store with 20 more headers appended, mirroring
/// `db::tests::bulk_insert_calculates_median_time_past`'s synthetic chain.
fn seeded_store(test_name: &str) -> (HeaderStore, PathBuf, BlockHash) {
    let path = temp_db_path(test_name);
    remove_db_files(&path);
    let mut store = HeaderStore::open(path.to_str().unwrap()).unwrap();

    let genesis_header = SyncHeader::new(Mainnet::PARAMS.genesis);
    let genesis_hash = genesis_header.block_hash();
    let genesis = ConnectedHeader::new(genesis_header, genesis_hash, 0, Mainnet::PARAMS.genesis_work, 0, 0);
    store.insert_genesis(&genesis).unwrap();

    let tip = store.get_best_tip().unwrap().unwrap();
    let mut headers = Vec::new();
    let mut prev_hash = genesis_hash;
    let mut cumulative_work = genesis.chainwork;
    let mut window: Vec<u32> = tip.timestamps[..zkpow_core::timestamp_count(tip.height)].to_vec();
    let base_time = genesis.header.inner.timestamp.into_inner();

    for i in 1u32..=20 {
        let mut inner = genesis.header.inner;
        inner.prev_blockhash = prev_hash;
        inner.timestamp = BlockTimestamp::new(base_time + i * 600);

        let mut tmp_win = window.clone();
        tmp_win.sort_unstable();
        let mtp = tmp_win[tmp_win.len() / 2];

        let header = SyncHeader::new(inner);
        let work = block_work_from_target(target_from_bits(inner.compact_target));
        cumulative_work = cumulative_work + work;
        let block_hash = header.block_hash();
        let ch = ConnectedHeader::new(header, block_hash, i.into(), cumulative_work, mtp, u64::from(i));
        headers.push(ch);

        window.push(inner.timestamp.into_inner());
        if window.len() > 11 {
            window.remove(0);
        }
        prev_hash = headers.last().unwrap().block_hash();
    }
    store.bulk_insert(&headers).unwrap();

    (store, path, genesis_hash)
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
        assert_eq!(from_db.header.timestamp, from_bin.header.timestamp, "timestamp mismatch at {height}");
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
