use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use sync_headers::{ConnectedHeader, Header as SyncHeader};
use zkpow_core::{block_work_from_target, target_from_bits, BlockHash, BlockTimestamp};
use zkpow_params::{Consensus, Mainnet};

use super::*;

mod db;

pub(crate) fn temp_db_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("zkpow-store-{test_name}-{}-{nonce}.sqlite", std::process::id()))
}

pub(crate) fn remove_db_files(path: &std::path::Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(path.with_extension("sqlite-shm"));
    let _ = fs::remove_file(path.with_extension("sqlite-wal"));
}

/// A genesis-seeded store with 20 more headers appended.
pub(crate) fn seeded_store(test_name: &str) -> (HeaderStore, PathBuf, BlockHash) {
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
