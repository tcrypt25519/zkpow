//! Persistent per-`run_id` progress ledger.
//!
//! Each Incremental Verifiable Computation chain is identified by a
//! `run_id`. The ledger records, per run, the verifying key pinned at run
//! creation, the highest fully verified height, and the storage object name
//! of the proof at that height (absent for execute-only runs). A controller
//! restart rebuilds all in-memory run state from this table plus the header
//! database; unverified handed-out work is deliberately not persisted — it
//! is simply re-issued.

use rusqlite::{Connection, OptionalExtension};
use zkpow_core::VerifierKeyDigest;

/// Resolve the controller ledger path: `ZKPOW_LEDGER_PATH` if set, otherwise
/// `controller.db` at the workspace root.
pub fn ledger_path() -> std::path::PathBuf {
    match std::env::var("ZKPOW_LEDGER_PATH") {
        Ok(path) => std::path::PathBuf::from(path),
        Err(_) => concat!(env!("CARGO_MANIFEST_DIR"), "/../../controller.db").into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub run_id: String,
    pub verifier_key: VerifierKeyDigest,
    pub verified_height: u32,
    /// Storage object name of the proof at `verified_height`, when one was
    /// uploaded and verified.
    pub proof_object: Option<String>,
}

pub struct Ledger {
    conn: Connection,
}

impl Ledger {
    pub fn open(path: impl AsRef<std::path::Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                run_id          TEXT PRIMARY KEY,
                verifier_key    BLOB NOT NULL,
                verified_height INTEGER NOT NULL,
                proof_object    TEXT,
                updated_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn load(&self, run_id: &str) -> rusqlite::Result<Option<RunRecord>> {
        self.conn
            .query_row(
                "SELECT verifier_key, verified_height, proof_object FROM runs WHERE run_id = ?1",
                [run_id],
                |row| {
                    let verifier_key: Vec<u8> = row.get(0)?;
                    let verified_height: u32 = row.get(1)?;
                    let proof_object: Option<String> = row.get(2)?;
                    Ok((verifier_key, verified_height, proof_object))
                },
            )
            .optional()?
            .map(|(verifier_key, verified_height, proof_object)| {
                let verifier_key: [u8; 32] = verifier_key.try_into().map_err(|_| {
                    rusqlite::Error::InvalidColumnType(
                        0,
                        "verifier_key must be 32 bytes".to_string(),
                        rusqlite::types::Type::Blob,
                    )
                })?;
                Ok(RunRecord {
                    run_id: run_id.to_string(),
                    verifier_key: VerifierKeyDigest::from_bytes(verifier_key),
                    verified_height,
                    proof_object,
                })
            })
            .transpose()
    }

    pub fn create(&self, record: &RunRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO runs (run_id, verifier_key, verified_height, proof_object)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                record.run_id,
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
    pub fn record_progress(
        &self,
        run_id: &str,
        verified_height: u32,
        proof_object: Option<&str>,
    ) -> rusqlite::Result<()> {
        let updated = self.conn.execute(
            "UPDATE runs
             SET verified_height = ?2,
                 proof_object = ?3,
                 updated_at = strftime('%s','now')
             WHERE run_id = ?1",
            rusqlite::params![run_id, verified_height, proof_object],
        )?;
        if updated != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ledger(test_name: &str) -> Ledger {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "zkpow-ledger-{test_name}-{}-{nonce}.db",
            std::process::id()
        ));
        Ledger::open(path).unwrap()
    }

    #[test]
    fn missing_run_loads_as_none() {
        let ledger = temp_ledger("missing");
        assert_eq!(ledger.load("nope").unwrap(), None);
    }

    #[test]
    fn create_then_progress_round_trips() {
        let ledger = temp_ledger("round-trip");
        let record = RunRecord {
            run_id: "mainnet-01".to_string(),
            verifier_key: VerifierKeyDigest::from_raw([7; 8]),
            verified_height: 0,
            proof_object: None,
        };
        ledger.create(&record).unwrap();
        assert_eq!(ledger.load("mainnet-01").unwrap(), Some(record.clone()));

        ledger
            .record_progress("mainnet-01", 2016, Some("proof_height_1_to_2016.bin"))
            .unwrap();
        let loaded = ledger.load("mainnet-01").unwrap().unwrap();
        assert_eq!(loaded.verified_height, 2016);
        assert_eq!(loaded.proof_object.as_deref(), Some("proof_height_1_to_2016.bin"));
        assert_eq!(loaded.verifier_key, record.verifier_key);
    }

    #[test]
    fn duplicate_create_is_rejected() {
        let ledger = temp_ledger("duplicate");
        let record = RunRecord {
            run_id: "run".to_string(),
            verifier_key: VerifierKeyDigest::default(),
            verified_height: 0,
            proof_object: None,
        };
        ledger.create(&record).unwrap();
        assert!(ledger.create(&record).is_err());
    }

    #[test]
    fn progress_for_unknown_run_errors() {
        let ledger = temp_ledger("unknown");
        assert!(ledger.record_progress("ghost", 1, None).is_err());
    }
}
