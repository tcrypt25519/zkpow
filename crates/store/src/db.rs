//! SQLite persistence for block headers. The single implementation of
//! `headers.db` access — every reader and writer in this workspace (sync
//! pipelines, the prover's `Controller`) goes through `HeaderStore`.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use sync_headers::{ConnectedHeader, Tip};
use zkpow_core::{
    chain_work_from_db_bytes, chain_work_to_db_bytes, sha256d, BlockHash, BlockTimestamp,
    ChainWork, Claim, CompactTarget, Header, NewHeader, State, WINDOW_SIZE,
};

const HASH_LEN: usize = 32;

fn hash_to_bytes(hash: &BlockHash) -> [u8; HASH_LEN] {
    *hash.as_inner()
}

fn hash_from_bytes(bytes: &[u8]) -> rusqlite::Result<BlockHash> {
    let arr: [u8; HASH_LEN] = bytes.try_into().map_err(|_| {
        rusqlite::Error::InvalidColumnType(0, "blob".into(), rusqlite::types::Type::Blob)
    })?;
    Ok(BlockHash::new(arr))
}

fn work_to_bytes(work: &ChainWork) -> [u8; HASH_LEN] {
    chain_work_to_db_bytes(*work)
}

fn work_from_bytes(bytes: &[u8]) -> rusqlite::Result<ChainWork> {
    chain_work_from_db_bytes(bytes).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(0, "blob".into(), rusqlite::types::Type::Blob)
    })
}

fn ordered_timestamp_run_from_db(height: u64, value: i64) -> u64 {
    assert!(
        value >= 0 && value as u64 <= height,
        "invalid ordered_timestamp_run {value} at height {height}; rebuild or explicitly migrate headers.db"
    );
    value as u64
}

struct HeaderFields {
    version: u32,
    prev: [u8; HASH_LEN],
    merkle_root: [u8; HASH_LEN],
    timestamp: u32,
    n_bits: u32,
    nonce: u32,
}

fn header_fields(header: &Header) -> HeaderFields {
    HeaderFields {
        version: header.version,
        prev: *header.prev_blockhash.as_inner(),
        merkle_root: header.merkle_root,
        timestamp: header.timestamp.into_inner(),
        n_bits: header.compact_target.into_inner(),
        nonce: header.nonce,
    }
}

/// The SQLite header store.
pub struct HeaderStore {
    conn: Connection,
}

impl HeaderStore {
    pub fn open(db_path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS headers (
                block_hash      BLOB PRIMARY KEY,
                prev            BLOB NOT NULL,
                merkle_root     BLOB NOT NULL,
                version         INTEGER NOT NULL,
                timestamp       INTEGER NOT NULL,
                n_bits          INTEGER NOT NULL,
                nonce           INTEGER NOT NULL,
                height          INTEGER NOT NULL,
                chainwork       BLOB NOT NULL,
                median_time_past INTEGER NOT NULL DEFAULT 0,
                ordered_timestamp_run INTEGER NOT NULL CHECK (ordered_timestamp_run >= 0)
            );
            ",
        )?;

        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_headers_prev ON headers(prev);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_headers_height ON headers(height);
            ",
        )?;
        Ok(Self { conn })
    }

    pub fn get_best_tip(&self) -> rusqlite::Result<Option<Tip>> {
        struct BestTipRow {
            hash_bytes: Vec<u8>,
            height: i64,
            chainwork_bytes: Vec<u8>,
            n_bits: i64,
            ordered_timestamp_run: i64,
        }

        let row: Option<BestTipRow> = self
            .conn
            .query_row(
                "SELECT block_hash, height, chainwork, n_bits, ordered_timestamp_run FROM headers ORDER BY height DESC LIMIT 1",
                [],
                |row| {
                    Ok(BestTipRow {
                        hash_bytes: row.get(0)?,
                        height: row.get(1)?,
                        chainwork_bytes: row.get(2)?,
                        n_bits: row.get(3)?,
                        ordered_timestamp_run: row.get(4)?,
                    })
                },
            )
            .optional()?;

        match row {
            Some(row) => {
                let hash = hash_from_bytes(&row.hash_bytes)?;
                let chainwork = work_from_bytes(&row.chainwork_bytes)?;
                let ordered_timestamp_run =
                    ordered_timestamp_run_from_db(row.height as u64, row.ordered_timestamp_run);
                let (current_nbits, current_target, current_work) =
                    sync_headers::current_difficulty(CompactTarget::new(row.n_bits as u32));

                let mut stmt = self.conn.prepare(
                    "SELECT height, timestamp FROM headers ORDER BY height DESC LIMIT ?1",
                )?;
                let mut timestamps = [0u32; WINDOW_SIZE];
                let rows = stmt.query_map(params![WINDOW_SIZE as i64], |row| {
                    let row_height: i64 = row.get(0)?;
                    let timestamp: u32 = row.get(1)?;
                    Ok((row_height, timestamp))
                })?;
                for row in rows {
                    let (row_height, timestamp) = row?;
                    timestamps[row_height as usize % WINDOW_SIZE] = timestamp;
                }

                Ok(Some(Tip::new(
                    hash,
                    row.height as u64,
                    chainwork,
                    timestamps,
                    ordered_timestamp_run,
                    current_nbits,
                    current_target,
                    current_work,
                )))
            }
            None => Ok(None),
        }
    }

    pub fn bulk_insert(&mut self, headers: &[ConnectedHeader]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;

        for ch in headers {
            let fields = header_fields(&ch.header.inner);
            tx.execute(
                "INSERT OR IGNORE INTO headers
                    (block_hash, prev, merkle_root, version, timestamp, n_bits, nonce, height, chainwork, median_time_past, ordered_timestamp_run)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    hash_to_bytes(&ch.block_hash()),
                    fields.prev,
                    fields.merkle_root,
                    fields.version,
                    fields.timestamp,
                    fields.n_bits,
                    fields.nonce,
                    ch.height as i64,
                    work_to_bytes(&ch.chainwork),
                    ch.median_time_past as i64,
                    ch.ordered_timestamp_run as i64,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn header_count(&self) -> rusqlite::Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM headers", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn insert_genesis(&self, genesis: &ConnectedHeader) -> rusqlite::Result<()> {
        let fields = header_fields(&genesis.header.inner);
        self.conn.execute(
            "INSERT OR IGNORE INTO headers
                (block_hash, prev, merkle_root, version, timestamp, n_bits, nonce, height, chainwork, median_time_past, ordered_timestamp_run)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10)",
            params![
                hash_to_bytes(&genesis.block_hash()),
                fields.prev,
                fields.merkle_root,
                fields.version,
                fields.timestamp,
                fields.n_bits,
                fields.nonce,
                genesis.height as i64,
                work_to_bytes(&genesis.chainwork),
                genesis.ordered_timestamp_run as i64,
            ],
        )?;
        Ok(())
    }

    pub fn load_public_claim(&self, height: u64, genesis_hash: BlockHash) -> Claim {
        const SQL: &str = "SELECT block_hash, chainwork FROM headers WHERE height = ?1";

        let mut stmt = self.conn.prepare_cached(SQL).expect("prepare");
        stmt.query_row(params![height], |row| {
            let block_hash: Vec<u8> = row.get(0)?;
            let chainwork: Vec<u8> = row.get(1)?;
            Ok(Claim {
                genesis_hash,
                tip_hash: hash_from_bytes(&block_hash)?,
                chain_work: work_from_bytes(&chainwork)?,
                height: height as u32,
            })
        })
        .expect("load_public_claim")
    }

    pub fn load_compact_target(&self, height: u64) -> CompactTarget {
        const SQL: &str = "SELECT n_bits FROM headers WHERE height = ?1";

        let mut stmt = self.conn.prepare_cached(SQL).expect("prepare");
        stmt.query_row(params![height], |row| {
            let nbits: i64 = row.get(0)?;
            Ok(CompactTarget::new(nbits as u32))
        })
        .expect("load_compact_target")
    }

    pub fn load_header_records(&self, start_height: u64, count: u64) -> Vec<HeaderRecord> {
        const SQL: &str =
            "SELECT height, version, prev, merkle_root, timestamp, n_bits, nonce, chainwork, \
             median_time_past, ordered_timestamp_run FROM headers \
             WHERE height >= ?1 AND height < ?2 ORDER BY height ASC";

        let mut stmt = self.conn.prepare_cached(SQL).expect("prepare");
        let rows = stmt
            .query_map(params![start_height, start_height + count], |row| {
                let height: i64 = row.get(0)?;
                let version: i64 = row.get(1)?;
                let prev: Vec<u8> = row.get(2)?;
                let merkle_root: Vec<u8> = row.get(3)?;
                let timestamp: i64 = row.get(4)?;
                let nbits: i64 = row.get(5)?;
                let nonce: i64 = row.get(6)?;
                let chainwork: Vec<u8> = row.get(7)?;
                let median_time_past: i64 = row.get(8)?;
                let ordered_timestamp_run: i64 = row.get(9)?;
                let height = height as u64;
                Ok(HeaderRecord {
                    height,
                    header: Header {
                        version: version as u32,
                        prev_blockhash: hash_from_bytes(&prev)?,
                        merkle_root: merkle_root
                            .try_into()
                            .expect("merkle_root must be 32 bytes"),
                        timestamp: BlockTimestamp::new(timestamp as u32),
                        compact_target: CompactTarget::new(nbits as u32),
                        nonce: nonce as u32,
                    },
                    chain_work: work_from_bytes(&chainwork)?,
                    median_time_past: BlockTimestamp::new(median_time_past as u32),
                    ordered_timestamp_run: ordered_timestamp_run_from_db(
                        height,
                        ordered_timestamp_run,
                    ),
                })
            })
            .expect("query");

        let mut records = Vec::with_capacity(count as usize);
        for row in rows {
            records.push(row.expect("row"));
        }
        records
    }

    pub fn load_header_record(&self, height: u64) -> HeaderRecord {
        self.load_header_records(height, 1)
            .into_iter()
            .next()
            .expect("one header record")
    }

    pub fn load_new_headers(&self, start_height: u64, count: u64) -> Vec<NewHeader> {
        self.load_header_batch_witness(start_height, count).headers
    }

    pub fn load_header_batch_witness(&self, start_height: u64, count: u64) -> HeaderBatchWitness {
        const SQL: &str =
            "SELECT h.height, h.version, h.merkle_root, h.timestamp, h.nonce, h.median_time_past, h.ordered_timestamp_run \
             FROM headers h \
             WHERE h.height >= ?1 AND h.height < ?2 ORDER BY h.height ASC";

        let mut stmt = self.conn.prepare_cached(SQL).expect("prepare");
        let rows = stmt
            .query_map(params![start_height, start_height + count], |row| {
                let height: i64 = row.get(0)?;
                let version: i64 = row.get(1)?;
                let merkle_root: Vec<u8> = row.get(2)?;
                let timestamp: i64 = row.get(3)?;
                let nonce: i64 = row.get(4)?;
                let median_time_past: i64 = row.get(5)?;
                let ordered_timestamp_run: i64 = row.get(6)?;
                let height = height as u64;
                Ok((
                    height,
                    NewHeader {
                        version: version as u32,
                        merkle_root: merkle_root
                            .try_into()
                            .expect("merkle_root must be 32 bytes"),
                        timestamp: BlockTimestamp::new(timestamp as u32),
                        nonce: nonce as u32,
                    },
                    BlockTimestamp::new(median_time_past as u32),
                    ordered_timestamp_run_from_db(height, ordered_timestamp_run),
                ))
            })
            .expect("query");

        let mut headers = Vec::with_capacity(count as usize);
        let mut median_time_past_hints = Vec::with_capacity(count as usize);
        let mut previous_ordered_timestamp_run = if start_height == 0 {
            0
        } else {
            self.load_header_record(start_height - 1)
                .ordered_timestamp_run
        };
        for row in rows {
            let (height, h, m, ordered_timestamp_run) = row.expect("row");
            headers.push(h);
            if height >= WINDOW_SIZE as u64
                && previous_ordered_timestamp_run < (WINDOW_SIZE as u64 - 1)
            {
                median_time_past_hints.push(m);
            }
            previous_ordered_timestamp_run = ordered_timestamp_run;
        }
        HeaderBatchWitness {
            headers,
            median_time_past_hints,
        }
    }

    /// Reconstruct full consensus state at an arbitrary height.
    pub fn state_at_height(&self, height: u32, genesis_hash: BlockHash) -> State {
        if height == 0 {
            return genesis_state_from_record(self.load_header_record(0), genesis_hash);
        }

        let current = self.load_header_record(height as u64);
        let epoch_start_height = (height / zkpow_core::EPOCH_LENGTH) * zkpow_core::EPOCH_LENGTH;
        let epoch_start = self.load_header_record(epoch_start_height as u64);
        let window_count = (height as usize + 1).min(zkpow_core::WINDOW_SIZE) as u64;
        let window_start = height as u64 + 1 - window_count;
        let window = self.load_header_records(window_start, window_count);

        let mut timestamps = [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE];
        for r in window {
            timestamps[r.height as usize % zkpow_core::WINDOW_SIZE] = r.header.timestamp;
        }

        State {
            block_hash: BlockHash::new(sha256d(&current.header.to_bytes())),
            genesis_hash,
            current_nbits: current.header.compact_target,
            height,
            chain_work: current.chain_work,
            epoch_start_timestamp: epoch_start.header.timestamp,
            timestamps,
        }
    }
}

/// A single `headers` row, decoded into typed fields.
#[derive(Debug, Clone)]
pub struct HeaderRecord {
    pub height: u64,
    pub header: Header,
    pub chain_work: ChainWork,
    pub median_time_past: BlockTimestamp,
    pub ordered_timestamp_run: u64,
}

impl HeaderRecord {
    pub fn public_claim(&self, genesis_hash: BlockHash) -> Claim {
        Claim {
            genesis_hash,
            tip_hash: BlockHash::new(sha256d(&self.header.to_bytes())),
            chain_work: self.chain_work,
            height: self.height as u32,
        }
    }
}

/// The SP1 stdin witness for a batch: the new headers plus their median-time-past hints.
#[derive(Debug, Clone)]
pub struct HeaderBatchWitness {
    pub headers: Vec<NewHeader>,
    pub median_time_past_hints: Vec<BlockTimestamp>,
}

/// Reconstruct the genesis state from its stored record.
pub fn genesis_state_from_record(genesis: HeaderRecord, genesis_hash: BlockHash) -> State {
    let block_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
    assert_eq!(block_hash, genesis_hash, "genesis hash mismatch");

    let mut timestamps = [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE];
    timestamps[0] = genesis.header.timestamp;

    State {
        block_hash,
        genesis_hash,
        current_nbits: genesis.header.compact_target,
        height: genesis.height as u32,
        chain_work: genesis.chain_work,
        epoch_start_timestamp: genesis.header.timestamp,
        timestamps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sync_headers::Header as SyncHeader;
    use zkpow_core::{target_from_bits, work_from_target, BlockTimestamp};

    fn temp_db_path(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zkpow-{test_name}-{}-{nonce}.sqlite",
            std::process::id()
        ))
    }

    fn remove_db_files(path: &std::path::Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("sqlite-shm"));
        let _ = fs::remove_file(path.with_extension("sqlite-wal"));
    }

    fn genesis_connected_header() -> ConnectedHeader {
        let header = SyncHeader::new(zkpow_core::MAINNET_GENESIS_HEADER);
        let block_hash = header.block_hash();
        ConnectedHeader::new(header, block_hash, 0, zkpow_core::genesis_work(), 0, 0)
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
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(
            columns,
            vec![
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
            ]
        );

        let table_names: Vec<String> = {
            let mut stmt = store
                .conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(table_names, vec!["headers"]);

        let indexes: Vec<(String, bool)> = {
            let mut stmt = store.conn.prepare("PRAGMA index_list(headers)").unwrap();
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, bool>(2)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
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
        assert_eq!(row.2, genesis.header.inner.merkle_root);
        assert_eq!(row.3, 1);
        assert_eq!(row.4, 1231006505);
        assert_eq!(row.5, 0x1d00ffff);
        assert_eq!(row.6, 2083236893);
        assert_eq!(row.7, 0);
        assert_eq!(row.8, work_to_bytes(&genesis.chainwork));
        assert_eq!(row.9, 0);
        assert_eq!(row.10, 0);

        assert_eq!(store.header_count().unwrap(), 1);
        assert_eq!(
            store.get_best_tip().unwrap().unwrap().hash,
            genesis.block_hash()
        );

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
        store
            .conn
            .execute("PRAGMA ignore_check_constraints = ON", [])
            .unwrap();
        store
            .conn
            .execute("UPDATE headers SET ordered_timestamp_run = 1", [])
            .unwrap();

        let _ = store.load_header_batch_witness(0, 1);
    }

    #[test]
    fn bulk_insert_calculates_median_time_past() {
        let path = temp_db_path("bulk-insert-mtp");
        remove_db_files(&path);
        let mut store = HeaderStore::open(path.to_str().unwrap()).unwrap();

        let genesis = genesis_connected_header();
        store.insert_genesis(&genesis).unwrap();

        let tip = store.get_best_tip().unwrap().unwrap();

        let mut headers = Vec::new();
        let mut prev_hash = genesis.block_hash();
        let mut cumulative_work = genesis.chainwork;
        let mut window: Vec<u32> =
            tip.timestamps[..zkpow_core::timestamp_count(tip.height)].to_vec();
        let base_time = genesis.header.inner.timestamp.into_inner();

        for i in 1u32..=12 {
            let mut inner = genesis.header.inner;
            inner.prev_blockhash = prev_hash;
            inner.timestamp = BlockTimestamp::new(base_time + i * 600);

            let mut tmp_win = window.clone();
            tmp_win.sort_unstable();
            let mtp = tmp_win[tmp_win.len() / 2];

            let header = SyncHeader::new(inner);
            let work =
                work_from_target(target_from_bits(inner.compact_target)).expect("valid target");
            cumulative_work = cumulative_work + work;
            let ordered_timestamp_run = u64::from(i);
            let block_hash = header.block_hash();
            let ch = ConnectedHeader::new(
                header,
                block_hash,
                i.into(),
                cumulative_work,
                mtp,
                ordered_timestamp_run,
            );
            headers.push(ch);

            window.push(inner.timestamp.into_inner());
            if window.len() > 11 {
                window.remove(0);
            }

            prev_hash = headers.last().unwrap().block_hash();
        }

        store.bulk_insert(&headers).unwrap();

        let mtp1: i64 = store
            .conn
            .query_row(
                "SELECT median_time_past FROM headers WHERE height = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mtp1, base_time as i64);

        let mtp2: i64 = store
            .conn
            .query_row(
                "SELECT median_time_past FROM headers WHERE height = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mtp2, (base_time + 600) as i64);

        let mtp11: i64 = store
            .conn
            .query_row(
                "SELECT median_time_past FROM headers WHERE height = 11",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mtp11, (base_time + 3000) as i64);

        drop(store);
        remove_db_files(&path);
    }
}
