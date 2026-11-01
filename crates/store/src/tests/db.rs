use sync_headers::{ConnectedHeader, Header as SyncHeader};
use zkpow_core::VerifierKeyDigest;
use zkpow_params::{Consensus, Mainnet};
use zkpow_types::ChainWork;

use super::*;
use crate::db::{hash_to_bytes, work_to_bytes};

const RUN_ID: &str = "mainnet-01";
const RUN_PROOF_OBJECT: &str = "proof_height_1_to_2016.bin";

#[test]
fn malformed_merkle_root_preserves_cast_error() {
    let store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
    store.insert_genesis(&genesis_connected_header()).unwrap();
    store.conn.execute("UPDATE headers SET merkle_root = zeroblob(31)", []).unwrap();

    assert!(matches!(store.load_header_records(0, 1), Err(Error::PodCastError(_))));
    assert!(matches!(store.load_header_batch_witness(0, 1), Err(Error::PodCastError(_))));
}

#[test]
fn malformed_hashes_preserve_cast_error() {
    let store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
    store.insert_genesis(&genesis_connected_header()).unwrap();
    store.conn.execute("UPDATE headers SET prev = zeroblob(31)", []).unwrap();
    assert!(matches!(store.load_header_records(0, 1), Err(Error::PodCastError(_))));

    store.conn.execute("UPDATE headers SET block_hash = zeroblob(31)", []).unwrap();
    assert!(matches!(store.get_best_tip::<Mainnet>(), Err(Error::PodCastError(_))));
}

#[test]
fn malformed_chainwork_preserves_conversion_error() {
    let store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
    store.insert_genesis(&genesis_connected_header()).unwrap();
    store.conn.execute("UPDATE headers SET chainwork = zeroblob(31)", []).unwrap();

    assert!(matches!(store.load_header_records(0, 1), Err(Error::ConversionError(_))));
    assert!(matches!(store.get_best_tip::<Mainnet>(), Err(Error::ConversionError(_))));
}

#[test]
fn malformed_verifier_key_preserves_conversion_error() {
    let store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
    let record = RunRecord {
        verifier_key:    VerifierKeyDigest::from_raw([7; 8]),
        verified_height: 0,
        proof_object:    None,
    };
    store.create_run(RUN_ID, &record).unwrap();
    store.conn.execute("UPDATE runs SET verifier_key = zeroblob(31)", []).unwrap();

    assert!(matches!(store.load_run(RUN_ID), Err(Error::ConversionError(_))));
    assert_eq!(store.load_run("missing").unwrap(), None);
}

#[test]
fn sql_failures_are_wrapped_in_store_error() {
    let store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
    store.conn.execute("DROP TABLE headers", []).unwrap();

    assert!(matches!(store.load_header_records(0, 1), Err(Error::RusqliteError(_))));
    assert!(matches!(store.load_header_batch_witness(0, 1), Err(Error::RusqliteError(_))));
}

#[test]
fn run_ledger_round_trips_in_the_header_database() {
    let path = temp_db_path("runs");
    let db = HeaderStore::open(&path).unwrap();

    assert_eq!(db.load_run("missing").unwrap(), None);

    let record = RunRecord {
        verifier_key:    VerifierKeyDigest::from_raw([7; 8]),
        verified_height: 0,
        proof_object:    None,
    };
    db.create_run(RUN_ID, &record).unwrap();
    assert_eq!(db.load_run(RUN_ID).unwrap(), Some(record.clone()));
    assert!(db.create_run(RUN_ID, &record).is_err(), "duplicate create");

    db.record_run_progress(RUN_ID, 2016, Some(RUN_PROOF_OBJECT)).unwrap();
    let loaded = db.load_run(RUN_ID).unwrap().unwrap();
    assert_eq!(loaded.verified_height, 2016);
    assert_eq!(loaded.proof_object.as_deref(), Some(RUN_PROOF_OBJECT));
    assert_eq!(loaded.verifier_key, record.verifier_key);

    assert!(db.record_run_progress("ghost", 1, None).is_err(), "unknown run");

    db.delete_run(RUN_ID).unwrap();
    assert_eq!(db.load_run(RUN_ID).unwrap(), None);

    remove_db_files(&path);
}

fn genesis_connected_header() -> ConnectedHeader {
    let header = SyncHeader::new(Mainnet::PARAMS.genesis);
    let block_hash = header.block_hash();

    // Genesis is the only block at its height, so `genesis_work` equals the
    // cumulative `ChainWork` here.
    let chain_work = ChainWork::new(Mainnet::PARAMS.genesis_work.into_inner());
    ConnectedHeader::new(header, block_hash, 0, chain_work, 0, 0)
}

#[test]
fn open_creates_current_headers_schema_only() {
    let path = temp_db_path("schema");
    remove_db_files(&path);

    let store = HeaderStore::open(path.to_str().unwrap()).unwrap();

    let columns: Vec<String> = {
        let mut stmt = store.conn.prepare("PRAGMA table_info(headers)").unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    };
    assert_eq!(columns, vec![
        "block_hash",
        "prev",
        "merkle_root",
        "version",
        "timestamp",
        "n_bits",
        "nonce",
        "height",
        "chainwork",
        "median_time_past",
        "ordered_timestamp_run",
    ]);

    let table_names: Vec<String> = {
        let mut stmt = store
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    };
    assert_eq!(table_names, vec!["headers", "runs"]);

    let indexes: Vec<(String, bool)> = {
        let mut stmt = store.conn.prepare("PRAGMA index_list(headers)").unwrap();
        stmt.query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, bool>(2)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    };
    assert!(indexes.contains(&("idx_headers_height".to_string(), true)));

    drop(store);
    remove_db_files(&path);
}

#[test]
fn insert_genesis_stores_decomposed_header_fields() {
    let path = temp_db_path("insert");
    remove_db_files(&path);
    let store = HeaderStore::open(path.to_str().unwrap()).unwrap();
    let genesis = genesis_connected_header();

    store.insert_genesis(&genesis).unwrap();

    let row = store
        .conn
        .query_row(
            "SELECT block_hash, prev, merkle_root, version, timestamp, n_bits, nonce, height, chainwork, median_time_past, ordered_timestamp_run
             FROM headers",
            [],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, u32>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(row.0, hash_to_bytes(&genesis.block_hash()));
    assert_eq!(row.1, hash_to_bytes(&genesis.prev_blockhash()));
    assert_eq!(row.2, genesis.header.inner.merkle_root.as_bytes());
    assert_eq!(row.3, 1);
    assert_eq!(row.4, 1231006505);
    assert_eq!(row.5, 0x1d00ffff);
    assert_eq!(row.6, 2083236893);
    assert_eq!(row.7, 0);
    assert_eq!(row.8, work_to_bytes(&genesis.chainwork));
    assert_eq!(row.9, 0);
    assert_eq!(row.10, 0);

    assert_eq!(store.header_count().unwrap(), 1);
    assert_eq!(store.get_best_tip::<Mainnet>().unwrap().unwrap().hash, genesis.block_hash());

    drop(store);
    remove_db_files(&path);
}

#[test]
#[should_panic(expected = "invalid ordered_timestamp_run 1 at height 0")]
fn stale_ordered_timestamp_run_fails_before_witness_loading() {
    let path = temp_db_path("invalid-ordered-run");
    remove_db_files(&path);
    let store = HeaderStore::open(path.to_str().unwrap()).unwrap();
    let genesis = genesis_connected_header();

    store.insert_genesis(&genesis).unwrap();
    store.conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
    store.conn.execute("UPDATE headers SET ordered_timestamp_run = 1", []).unwrap();

    let _ = store.load_header_batch_witness(0, 1);
}

#[test]
fn bulk_insert_calculates_median_time_past() {
    let (store, path, _genesis_hash) = seeded_store("bulk-insert-mtp");

    let base_time = Mainnet::PARAMS.genesis.timestamp.into_inner();

    let mtp1: i64 = store
        .conn
        .query_row("SELECT median_time_past FROM headers WHERE height = 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mtp1, base_time as i64);

    let mtp2: i64 = store
        .conn
        .query_row("SELECT median_time_past FROM headers WHERE height = 2", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mtp2, (base_time + 600) as i64);

    let mtp11: i64 = store
        .conn
        .query_row("SELECT median_time_past FROM headers WHERE height = 11", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mtp11, (base_time + 3000) as i64);

    drop(store);
    remove_db_files(&path);
}
