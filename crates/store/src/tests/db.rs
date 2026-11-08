use sync_headers::{ConnectedHeader, Header as SyncHeader};
use zkpow_core::VerifierKeyDigest;
use zkpow_params::{Consensus, Mainnet};
use zkpow_types::ChainWork;

use super::{test_utils::in_memory_store, *};
use crate::db::{hash_to_bytes, work_to_bytes};

const RUN_ID: &str = "mainnet-01";
const RUN_PROOF_OBJECT: &str = "proof_height_1_to_2016.bin";

#[test]
fn malformed_merkle_root_preserves_cast_error() {
    let store = in_memory_store();
    store.insert_genesis(&genesis_connected_header()).unwrap();
    store.conn.execute("UPDATE headers SET merkle_root = zeroblob(31)", []).unwrap();

    assert!(matches!(store.load_header_records(0, 1), Err(Error::PodCastError(_))));
    assert!(matches!(store.load_header_batch_witness(0, 1), Err(Error::PodCastError(_))));
}

#[test]
fn malformed_hashes_preserve_cast_error() {
    let store = in_memory_store();
    store.insert_genesis(&genesis_connected_header()).unwrap();
    store.conn.execute("UPDATE headers SET prev = zeroblob(31)", []).unwrap();
    assert!(matches!(store.load_header_records(0, 1), Err(Error::PodCastError(_))));

    store.conn.execute("UPDATE headers SET block_hash = zeroblob(31)", []).unwrap();
    assert!(matches!(store.get_best_tip::<Mainnet>(), Err(Error::PodCastError(_))));
}

#[test]
fn malformed_chainwork_preserves_conversion_error() {
    let store = in_memory_store();
    store.insert_genesis(&genesis_connected_header()).unwrap();
    store.conn.execute("UPDATE headers SET chainwork = zeroblob(31)", []).unwrap();

    assert!(matches!(store.load_header_records(0, 1), Err(Error::ConversionError(_))));
    assert!(matches!(store.get_best_tip::<Mainnet>(), Err(Error::ConversionError(_))));
}

#[test]
fn malformed_verifier_key_preserves_conversion_error() {
    let store = in_memory_store();
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
    let store = in_memory_store();
    store.conn.execute("DROP TABLE headers", []).unwrap();

    assert!(matches!(store.load_header_records(0, 1), Err(Error::RusqliteError(_))));
    assert!(matches!(store.load_header_batch_witness(0, 1), Err(Error::RusqliteError(_))));
}

#[test]
fn run_ledger_round_trips_in_the_header_database() {
    let db = in_memory_store();

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
}

fn genesis_connected_header() -> ConnectedHeader {
    let header = SyncHeader::new(Mainnet::PARAMS.genesis);
    let block_hash = header.block_hash();

    // Genesis is the only block at its height, so `genesis_work` equals the
    // cumulative `ChainWork` here.
    let chain_work = ChainWork::new(Mainnet::PARAMS.genesis_work.into_inner());
    ConnectedHeader::new(header, block_hash, 0, chain_work, 0)
}

#[test]
fn open_creates_current_headers_schema_only() {
    let store = in_memory_store();

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
}

#[test]
fn insert_genesis_stores_decomposed_header_fields() {
    let store = in_memory_store();
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
}

#[test]
fn hint_selection_uses_the_window_instead_of_stored_run_metadata() {
    use zkpow_types::BlockTimestamp;

    let mut store = in_memory_store();
    let genesis = genesis_connected_header();
    store.insert_genesis(&genesis).unwrap();
    let base = genesis.header.inner.timestamp.into_inner();
    let mut history = vec![base];
    let mut previous = genesis.block_hash();
    let mut headers = Vec::new();
    for height in 1..=25 {
        let mut sorted = history[history.len().saturating_sub(zkpow_mtp::WINDOW_SIZE)..].to_vec();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        let mut inner = genesis.header.inner;
        inner.prev_blockhash = previous;
        inner.timestamp = BlockTimestamp::new(base + if height == 11 { 7 } else { height });
        inner.nonce = height;
        history.push(inner.timestamp.into_inner());
        let header = SyncHeader::new(inner);
        previous = header.block_hash();
        headers.push(ConnectedHeader::new(header, previous, height, genesis.chainwork, median));
    }
    store.bulk_insert(&headers).unwrap();
    store
        .conn
        .execute("UPDATE headers SET ordered_timestamp_run = 1000", [])
        .unwrap();
    let expected = [6, 7, 7, 8, 9, 10, 12, 13, 14, 15].map(|value| BlockTimestamp::new(base + value));
    assert_eq!(store.load_header_batch_witness(1, 25).unwrap().median_time_past_hints, expected);
    assert_eq!(
        store.load_header_batch_witness(12, 14).unwrap().median_time_past_hints,
        expected
    );
    assert_eq!(
        store.load_header_batch_witness(17, 9).unwrap().median_time_past_hints,
        expected[5..]
    );
    assert!(
        store
            .load_header_batch_witness(22, 4)
            .unwrap()
            .median_time_past_hints
            .is_empty()
    );
    let tip = store.get_best_tip::<Mainnet>().unwrap().unwrap();
    let state = store.state_at_height::<Mainnet>(25, Mainnet::GENESIS_HASH).unwrap().unwrap();
    assert_eq!(
        tip.timestamps,
        zkpow_mtp::WindowState::from_height(state.timestamps, state.height)
    );
    assert!(!tip.timestamps.requires_median_hint());
}

#[test]
fn empty_witness_ranges_do_not_require_a_timestamp_window() {
    let store = in_memory_store();
    for (start, count) in [(0, 0), (0, 10), (100, 10)] {
        let witness = store.load_header_batch_witness(start, count).unwrap();
        assert!(witness.headers.is_empty());
        assert!(witness.median_time_past_hints.is_empty());
    }
    store.insert_genesis(&genesis_connected_header()).unwrap();
    for (start, count) in [(0, 0), (1, 10), (100, 10)] {
        let witness = store.load_header_batch_witness(start, count).unwrap();
        assert!(witness.headers.is_empty());
        assert!(witness.median_time_past_hints.is_empty());
    }
}

#[test]
fn bulk_insert_stores_median_time_past() {
    let mut store = in_memory_store();
    let genesis = genesis_connected_header();
    store.insert_genesis(&genesis).unwrap();

    let mut inner = genesis.header.inner;
    inner.prev_blockhash = genesis.block_hash();
    let header = SyncHeader::new(inner);
    let block_hash = header.block_hash();
    let ch = ConnectedHeader::new(header, block_hash, 1, genesis.chainwork, 123456);
    store.bulk_insert(&[ch]).unwrap();

    let mtp: i64 = store
        .conn
        .query_row("SELECT median_time_past FROM headers WHERE height = 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mtp, 123456);
}
