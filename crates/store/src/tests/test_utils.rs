use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sync_headers::{ConnectedHeader, Header as SyncHeader};
use zkpow_core::{block_work_from_target, target_from_bits, BlockHash, BlockTimestamp, ChainWork};
use zkpow_params::{Consensus, Mainnet};

use crate::{db::SQLITE_MEMORY_DATABASE, HeaderStore};

const TEMP_DB_FILE_PREFIX: &str = "zkpow-store-";
const TEMP_DB_FILE_EXTENSION: &str = ".sqlite";
const SQLITE_SHARED_MEMORY_EXTENSION: &str = "sqlite-shm";
const SQLITE_WRITE_AHEAD_LOG_EXTENSION: &str = "sqlite-wal";
const SQL_ATTACH_SOURCE_DATABASE: &str = "ATTACH DATABASE ?1 AS src";
const SQL_COPY_SOURCE_HEADERS: &str = "INSERT INTO headers SELECT * FROM src.headers WHERE height <= ?1";
const SQL_DETACH_SOURCE_DATABASE: &str = "DETACH DATABASE src";
const TEST_FIXTURE_HEADERS_SQL: &str = include_str!("test_fixture_headers.sql");

pub fn temp_db_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!(
        "{TEMP_DB_FILE_PREFIX}{test_name}-{}-{nonce}{TEMP_DB_FILE_EXTENSION}",
        std::process::id()
    ))
}

pub fn remove_db_files(path: &Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(path.with_extension(SQLITE_SHARED_MEMORY_EXTENSION));
    let _ = fs::remove_file(path.with_extension(SQLITE_WRITE_AHEAD_LOG_EXTENSION));
}

/// A genesis-seeded store with `count` more headers appended at `path`.
pub fn seeded_store_at(path: &Path, count: u32) -> (HeaderStore, BlockHash) {
    remove_db_files(path);
    let mut store = HeaderStore::open(path).unwrap();

    let genesis_header = SyncHeader::new(Mainnet::PARAMS.genesis);
    let genesis_hash = genesis_header.block_hash();
    let genesis_chain_work = ChainWork::new(*Mainnet::PARAMS.genesis_work.as_inner());
    let genesis = ConnectedHeader::new(genesis_header, genesis_hash, 0, genesis_chain_work, 0, 0);
    store.insert_genesis(&genesis).unwrap();

    let tip = store.get_best_tip::<Mainnet>().unwrap().unwrap();
    let mut headers = Vec::new();
    let mut prev_hash = genesis_hash;
    let mut cumulative_work = genesis.chainwork;
    let mut window: Vec<u32> = tip.timestamps[..zkpow_core::timestamp_count(tip.height)].to_vec();
    let base_time = genesis.header.inner.timestamp.into_inner();

    for i in 1u32..=count {
        let mut inner = genesis.header.inner;
        inner.prev_blockhash = prev_hash;
        inner.timestamp = BlockTimestamp::new(base_time + i * 600);

        let mut tmp_win = window.clone();
        tmp_win.sort_unstable();
        let mtp = tmp_win[tmp_win.len() / 2];

        let header = SyncHeader::new(inner);
        let target = target_from_bits(inner.compact_target);
        let work = block_work_from_target(&target);
        cumulative_work += work;
        let block_hash = header.block_hash();
        let ch = ConnectedHeader::new(header, block_hash, i, cumulative_work, mtp, u64::from(i));
        headers.push(ch);

        window.push(inner.timestamp.into_inner());
        if window.len() > 11 {
            window.remove(0);
        }
        prev_hash = headers.last().unwrap().block_hash();
    }
    if !headers.is_empty() {
        store.bulk_insert(&headers).unwrap();
    }

    (store, genesis_hash)
}

/// A genesis-seeded store with 20 more headers appended in a temporary database file.
pub fn seeded_store(test_name: &str) -> (HeaderStore, PathBuf, BlockHash) {
    let path = temp_db_path(test_name);
    let (store, genesis_hash) = seeded_store_at(&path, 20);
    (store, path, genesis_hash)
}

/// Creates an isolated store at `path` populated with real mined headers up to `max_height`
/// copied from the operational database at `db_path()`.
pub fn create_real_seeded_store(path: &Path, max_height: u32) -> HeaderStore {
    remove_db_files(path);
    let store = HeaderStore::open(path).unwrap();
    let src_path = crate::db_path();
    if src_path.exists() {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute(SQL_ATTACH_SOURCE_DATABASE, [src_path.to_str().unwrap()]).unwrap();
        conn.execute(SQL_COPY_SOURCE_HEADERS, rusqlite::params![max_height]).unwrap();
        conn.execute(SQL_DETACH_SOURCE_DATABASE, []).unwrap();
    }
    store
}

/// Creates an in-memory HeaderStore populated with standard test fixture headers:
/// heights 0..=128, height 38304, and heights 40260..=40320.
pub fn create_test_fixture_store() -> HeaderStore {
    let store = HeaderStore::open(SQLITE_MEMORY_DATABASE).expect("failed to open in-memory store");
    store.conn.execute_batch(TEST_FIXTURE_HEADERS_SQL).expect("failed to insert test fixture headers");
    store
}
