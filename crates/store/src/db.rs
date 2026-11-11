//! SQLite persistence for block headers. The single implementation of
//! `headers.db` access. Every reader and writer in this workspace (sync
//! pipelines, the prover's `Controller`) goes through `HeaderStore`.

use std::path::Path;

use bytemuck::PodCastError;
use rusqlite::{Connection, OptionalExtension, params};
use sync_headers::{ConnectedHeader, Tip};
use zkpow_core::{HeaderInput, State, VerifierKeyDigest};
use zkpow_mtp::{Cold, WINDOW_SIZE, Window, WindowState};
use zkpow_params::Consensus;
use zkpow_types::{BlockHash, BlockTimestamp, ChainWork, CompactTarget, Header, MerkleRoot};

pub const SQLITE_MEMORY_DATABASE: &str = ":memory:";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("data failed to convert")]
    ConversionError(#[from] std::array::TryFromSliceError),
    #[error("rusqlite error")]
    RusqliteError(#[from] rusqlite::Error),
    #[error("SQL value conversion failed: {0}")]
    FromSqlError(#[from] rusqlite::types::FromSqlError),
    #[error("bytemuck cast failed: {0}")]
    PodCastError(bytemuck::PodCastError),
}

impl From<PodCastError> for Error {
    fn from(err: PodCastError) -> Self {
        Self::PodCastError(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type ResultOpt<T> = Result<Option<T>>;

pub(crate) fn hash_to_bytes(hash: &BlockHash) -> [u8; BlockHash::SIZE] {
    *hash.as_inner()
}

/// Big-endian byte encoding of chainwork as stored in `headers.db`'s `chainwork`
/// column. It predates `ChainWork`'s little-endian in-memory representation.
/// Every reader and writer of that column must agree on this convention.
pub(crate) fn work_to_bytes(work: &ChainWork) -> [u8; ChainWork::SIZE] {
    let mut bytes = work.to_le_bytes();
    bytes.reverse();
    bytes
}

/// Inverse of [`work_to_bytes`]. Returns `None` if `bytes` isn't exactly 32 bytes.
fn work_from_bytes(bytes: &[u8]) -> Result<ChainWork> {
    let mut arr: [u8; ChainWork::SIZE] = bytes.try_into()?;
    arr.reverse();
    Ok(ChainWork::from_le_bytes(arr))
}

struct HeaderFields {
    version:     u32,
    prev:        [u8; BlockHash::SIZE],
    merkle_root: [u8; BlockHash::SIZE],
    timestamp:   u32,
    n_bits:      u32,
    nonce:       u32,
}

fn header_fields(header: &Header) -> HeaderFields {
    HeaderFields {
        version:     header.version,
        prev:        *header.prev_blockhash.as_inner(),
        merkle_root: *header.merkle_root,
        timestamp:   header.timestamp.into_inner(),
        n_bits:      header.compact_target.into_inner(),
        nonce:       header.nonce,
    }
}

/// The SQLite header store.
pub struct HeaderStore {
    pub(crate) conn: Connection,
}

impl HeaderStore {
    /// Open the database, creating the schema when missing.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let conn = if db_path.as_ref().to_str() == Some(SQLITE_MEMORY_DATABASE) {
            Connection::open_in_memory()
        } else {
            Connection::open(db_path)
        }?;

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

            CREATE TABLE IF NOT EXISTS runs (
                run_id          TEXT PRIMARY KEY,
                verifier_key    BLOB NOT NULL,
                verified_height INTEGER NOT NULL,
                proof_object    TEXT,
                updated_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
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

    pub fn get_best_tip<C: Consensus>(&self) -> ResultOpt<Tip> {
        struct BestTipRow {
            hash_bytes:      Vec<u8>,
            height:          u32,
            chainwork_bytes: Vec<u8>,
            n_bits:          i64,
        }

        let row: Option<BestTipRow> = self
            .conn
            .query_row(
                "SELECT block_hash, height, chainwork, n_bits FROM headers ORDER BY height DESC LIMIT 1",
                [],
                |row| {
                    Ok(BestTipRow {
                        hash_bytes:      row.get(0)?,
                        height:          row.get(1)?,
                        chainwork_bytes: row.get(2)?,
                        n_bits:          row.get(3)?,
                    })
                },
            )
            .optional()?;

        match row {
            Some(row) => {
                let hash = *BlockHash::parse(&row.hash_bytes)?;
                let chainwork = work_from_bytes(&row.chainwork_bytes)?;
                let epoch_start_height = (row.height / C::PARAMS.epoch_length) * C::PARAMS.epoch_length;
                let epoch_timestamp: u32 = self.conn.query_row(
                    "SELECT timestamp FROM headers WHERE height = ?1",
                    params![epoch_start_height as i64],
                    |row| row.get(0),
                )?;
                let target_triplet = sync_headers::expand_target(CompactTarget::new(row.n_bits as u32));

                let timestamps = self.load_timestamp_window(row.height)?;

                Ok(Some(Tip::new(
                    hash,
                    row.height,
                    chainwork,
                    timestamps,
                    epoch_timestamp,
                    target_triplet,
                )))
            },
            None => Ok(None),
        }
    }

    pub fn bulk_insert(&mut self, headers: &[ConnectedHeader]) -> Result<()> {
        let tx = self.conn.transaction()?;

        for ch in headers {
            let fields = header_fields(&ch.header.inner);
            tx.execute(
                "INSERT OR IGNORE INTO headers
                    (block_hash, prev, merkle_root, version, timestamp, n_bits, nonce, height, chainwork, median_time_past, ordered_timestamp_run)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
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
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn load_timestamp_window(&self, height: u32) -> Result<WindowState> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT height, timestamp FROM (SELECT height, timestamp FROM headers WHERE height <= ?1 ORDER BY height DESC LIMIT ?2) ORDER BY height ASC",
        )?;
        let rows = stmt
            .query_map(params![height, WINDOW_SIZE as i64], |row| {
                Ok((row.get::<_, u32>(0)?, BlockTimestamp::new(row.get::<_, u32>(1)?)))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let start_height = rows.first().map_or(0, |row| row.0);
        WindowState::from_timestamps(start_height, rows.into_iter().map(|row| row.1))
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn header_count(&self) -> Result<u64> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM headers", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn insert_genesis(&self, genesis: &ConnectedHeader) -> Result<()> {
        let fields = header_fields(&genesis.header.inner);
        self.conn.execute(
            "INSERT OR IGNORE INTO headers
                (block_hash, prev, merkle_root, version, timestamp, n_bits, nonce, height, chainwork, median_time_past, ordered_timestamp_run)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0)",
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
            ],
        )?;
        Ok(())
    }

    pub fn load_compact_target(&self, height: u64) -> ResultOpt<CompactTarget> {
        const SQL: &str = "SELECT n_bits FROM headers WHERE height = ?1";

        let mut stmt = self.conn.prepare_cached(SQL)?;
        let opt = stmt
            .query_row(params![height as i64], |row| {
                let nbits: i64 = row.get(0)?;
                Ok(CompactTarget::new(nbits as u32))
            })
            .optional()?;
        Ok(opt)
    }

    pub fn load_header_records(&self, start_height: u64, count: u64) -> Result<Vec<HeaderRecord>> {
        const SQL: &str = "SELECT height, version, prev, merkle_root, timestamp, n_bits, nonce, chainwork, \
            median_time_past FROM headers \
            WHERE height >= ?1 AND height < ?2 ORDER BY height ASC";

        let mut stmt = self.conn.prepare_cached(SQL)?;
        let rows = stmt.query_and_then(
            params![start_height as i64, (start_height + count) as i64],
            |row| -> Result<_> {
                let height: i64 = row.get(0)?;
                let version: i64 = row.get(1)?;
                // Borrow blob columns in place; the fixed-width consumers
                // below never need an owned Vec.
                let prev = row.get_ref(2)?.as_blob()?;
                let merkle_root = row.get_ref(3)?.as_blob()?;
                let timestamp: i64 = row.get(4)?;
                let nbits: i64 = row.get(5)?;
                let nonce: i64 = row.get(6)?;
                let chainwork = row.get_ref(7)?.as_blob()?;
                let median_time_past: i64 = row.get(8)?;
                let height = height as u64;

                let prev_blockhash = BlockHash::parse(prev)?;
                let merkle_root = MerkleRoot::parse(merkle_root)?;

                Ok(HeaderRecord {
                    height,
                    header: Header {
                        version:        version as u32,
                        prev_blockhash: *prev_blockhash,
                        merkle_root:    *merkle_root,
                        timestamp:      BlockTimestamp::new(timestamp as u32),
                        compact_target: CompactTarget::new(nbits as u32),
                        nonce:          nonce as u32,
                    },
                    chain_work: work_from_bytes(chainwork)?,
                    median_time_past: BlockTimestamp::new(median_time_past as u32),
                })
            },
        )?;

        let mut records = Vec::with_capacity(count as usize);
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn load_header_record(&self, height: u64) -> ResultOpt<HeaderRecord> {
        let mut records = self.load_header_records(height, 1)?;
        Ok(records.pop())
    }

    pub fn load_new_headers(&self, start_height: u64, count: u64) -> Result<Vec<HeaderInput>> {
        Ok(self.load_header_batch_witness(start_height, count)?.headers)
    }

    /// Load `count` headers from `start_height`, plus a sparse median hint for each header whose
    /// window is full but not provably ordered (see [`Window::requires_median_hint`]).
    pub fn load_header_batch_witness(&self, start_height: u64, count: u64) -> Result<HeaderBatchWitness> {
        const SQL: &str = "SELECT h.version, h.merkle_root, h.timestamp, h.nonce, h.median_time_past \
            FROM headers h \
            WHERE h.height >= ?1 AND h.height < ?2 ORDER BY h.height ASC";

        let mut stmt = self.conn.prepare_cached(SQL)?;
        let rows = stmt.query_and_then(
            params![start_height as i64, (start_height + count) as i64],
            |row| -> Result<_> {
                let version: i64 = row.get(0)?;
                // Borrowed blob: copied once into the fixed-width array, no
                // per-row Vec.
                let merkle_root = row.get_ref(1)?.as_blob()?;
                let timestamp: i64 = row.get(2)?;
                let nonce: i64 = row.get(3)?;
                let median_time_past: i64 = row.get(4)?;
                let merkle_root = MerkleRoot::parse(merkle_root)?;
                Ok((
                    HeaderInput {
                        version:     version as u32,
                        merkle_root: *merkle_root,
                        timestamp:   BlockTimestamp::new(timestamp as u32),
                        nonce:       nonce as u32,
                    },
                    BlockTimestamp::new(median_time_past as u32),
                ))
            },
        )?;

        let mut rows = rows.peekable();
        let mut witness = HeaderBatchWitness {
            headers:                Vec::with_capacity(count as usize),
            median_time_past_hints: Vec::with_capacity(count as usize),
        };
        if rows.peek().is_none() {
            return Ok(witness);
        }
        let timestamps = if start_height == 0 {
            let (genesis, _) = rows.next().unwrap()?;
            let cold = Window::<Cold>::new(genesis.timestamp);
            witness.headers.push(genesis);
            WindowState::Cold(cold)
        } else {
            self.load_timestamp_window((start_height - 1) as u32)?
        };
        let mut timestamps = match timestamps {
            WindowState::Cold(mut cold) => {
                for row in cold.take_cold_headers(rows.by_ref()) {
                    let (h, _) = row?;
                    cold.push(h.timestamp);
                    witness.headers.push(h);
                }
                let Some(warm) = cold.warm() else { return Ok(witness) };
                warm
            },
            WindowState::Warm(warm) => warm,
        };
        for row in rows {
            let (h, m) = row?;
            if timestamps.requires_median_hint() {
                witness.median_time_past_hints.push(m);
            }
            timestamps.push(h.timestamp);
            witness.headers.push(h);
        }
        Ok(witness)
    }

    /// Reconstruct full consensus state at an arbitrary height.
    pub fn state_at_height<C: Consensus>(&self, height: u32, genesis_hash: BlockHash) -> ResultOpt<State> {
        if height == 0 {
            let Some(genesis) = self.load_header_record(0)? else {
                return Ok(None);
            };
            return Ok(Some(genesis_state_from_record::<C>(genesis, genesis_hash)));
        }

        let epoch_length = C::PARAMS.epoch_length;
        let epoch_start_height = (height / epoch_length) * epoch_length;
        let Some(epoch_start) = self.load_header_record(epoch_start_height as u64)? else {
            return Ok(None);
        };
        let Some(current) = self.load_header_record(height as u64)? else {
            return Ok(None);
        };
        let timestamps = self.load_timestamp_window(height)?.values();

        Ok(Some(State {
            block_hash: BlockHash::from(&current.header),
            genesis_hash,
            current_nbits: current.header.compact_target,
            height,
            chain_work: current.chain_work,
            epoch_timestamp: epoch_start.header.timestamp,
            timestamps,
        }))
    }

    pub fn load_run(&self, run_id: &str) -> ResultOpt<RunRecord> {
        let opt = self
            .conn
            .query_row(
                "SELECT verifier_key, verified_height, proof_object FROM runs WHERE run_id = ?1",
                [run_id],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        opt.map(|(verifier_key, verified_height, proof_object)| -> Result<_> {
            let verifier_key: [u8; VerifierKeyDigest::SIZE] = verifier_key.as_slice().try_into()?;
            Ok(RunRecord {
                verifier_key: VerifierKeyDigest::from_bytes(verifier_key),
                verified_height,
                proof_object,
            })
        })
        .transpose()
    }

    pub fn create_run(&self, run_id: &str, record: &RunRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (run_id, verifier_key, verified_height, proof_object)
            VALUES (?1, ?2, ?3, ?4)",
            params![
                run_id,
                record.verifier_key.to_bytes(),
                record.verified_height,
                record.proof_object,
            ],
        )?;
        Ok(())
    }

    /// Persist a newly verified height. Callers must only invoke this after
    /// the batch's public values (and proof, when configured) have been
    /// validated.
    pub fn record_run_progress(&self, run_id: &str, verified_height: u32, proof_object: Option<&str>) -> Result<()> {
        let updated = self.conn.execute(
            "UPDATE runs
            SET verified_height = ?2,
                proof_object = ?3,
                updated_at = strftime('%s','now')
            WHERE run_id = ?1",
            params![run_id, verified_height, proof_object],
        )?;
        if updated != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    /// Forget a run entirely (operator reset of a halted or abandoned chain).
    pub fn delete_run(&self, run_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM runs WHERE run_id = ?1", [run_id])?;
        Ok(())
    }
}

/// One IVC chain's verified progress, keyed by `run_id` in the `runs` table.
/// Only verified progress is stored; handed-out-but-unverified work is
/// controller memory and is simply re-issued after a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    /// Verifying-key digest pinned at run creation.
    pub verifier_key:    VerifierKeyDigest,
    pub verified_height: u32,
    /// Storage object name of the proof at `verified_height`, when one was
    /// uploaded and verified.
    pub proof_object:    Option<String>,
}

/// A single `headers` row, decoded into typed fields.
#[derive(Debug, Clone)]
pub struct HeaderRecord {
    pub height:           u64,
    pub header:           Header,
    pub chain_work:       ChainWork,
    pub median_time_past: BlockTimestamp,
}

/// The SP1 stdin witness for a batch: the new headers plus their median-time-past hints.
#[derive(Debug, Clone)]
pub struct HeaderBatchWitness {
    pub headers:                Vec<HeaderInput>,
    pub median_time_past_hints: Vec<BlockTimestamp>,
}

/// The anchor state, after checking that the stored record is the record of that anchor.
pub fn genesis_state_from_record<C: Consensus>(genesis: HeaderRecord, genesis_hash: BlockHash) -> State {
    assert_eq!(genesis.header, C::PARAMS.genesis, "stored genesis is not the network anchor");
    assert_eq!(genesis_hash, C::GENESIS_HASH, "genesis hash is not the network anchor's");

    State::anchor::<C>()
}
