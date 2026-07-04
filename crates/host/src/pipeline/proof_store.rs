use std::path::{Path, PathBuf};

use s3::creds::Credentials;
use s3::{Bucket, Region};

use crate::pipeline::BoxError;

/// Where to resume a proving session from, as recorded by a `ProofStore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumePoint {
    /// No prior progress recorded; start from genesis.
    Genesis,
    /// Resume by loading and cross-checking this previously saved proof file.
    PreviousProof(PathBuf),
    /// Resume from this height without a proof witness (execute-only runs).
    TrustedHeight(u32),
}

/// Tracks proving progress so a new process can resume where the last one left off.
///
/// Implementations must write the proof artifact to durable storage *before*
/// recording it as the new resume point, so a crash mid-write never leaves
/// the pointer referencing a missing or partial artifact.
pub trait ProofStore {
    fn resume_point(&self) -> Result<ResumePoint, BoxError>;
    fn record_progress(
        &self,
        end_height: u32,
        compressed_path: Option<&Path>,
    ) -> Result<(), BoxError>;
}

const POINTER_FILE_NAME: &str = "latest.json";

/// Filesystem-backed `ProofStore`: a single pointer file (`latest.json`) in a
/// configured directory records the height most recently proved and, when a
/// real proof was produced, the path to its compressed proof file.
pub struct LocalProofStore {
    store_dir: PathBuf,
}

impl LocalProofStore {
    pub fn new(store_dir: impl Into<PathBuf>) -> Self {
        Self {
            store_dir: store_dir.into(),
        }
    }

    fn pointer_path(&self) -> PathBuf {
        self.store_dir.join(POINTER_FILE_NAME)
    }
}

impl ProofStore for LocalProofStore {
    fn resume_point(&self) -> Result<ResumePoint, BoxError> {
        let pointer_path = self.pointer_path();
        let bytes = match std::fs::read(&pointer_path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ResumePoint::Genesis)
            }
            Err(err) => return Err(err.into()),
        };

        let pointer: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|err| format!("malformed pointer file {}: {err}", pointer_path.display()))?;
        let end_height = pointer
            .get("end_height")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("pointer file {} missing end_height", pointer_path.display()))?
            as u32;

        match pointer
            .get("proof_path")
            .and_then(serde_json::Value::as_str)
        {
            Some(path) => Ok(ResumePoint::PreviousProof(PathBuf::from(path))),
            None => Ok(ResumePoint::TrustedHeight(end_height)),
        }
    }

    fn record_progress(
        &self,
        end_height: u32,
        compressed_path: Option<&Path>,
    ) -> Result<(), BoxError> {
        std::fs::create_dir_all(&self.store_dir)?;

        let pointer = serde_json::json!({
            "end_height": end_height,
            "proof_path": compressed_path.map(|path| path.to_string_lossy().into_owned()),
        });
        let bytes = serde_json::to_vec_pretty(&pointer)?;

        // Write-then-rename so a crash never leaves a partially written pointer file.
        let tmp_path = self.store_dir.join(format!("{POINTER_FILE_NAME}.tmp"));
        std::fs::write(&tmp_path, &bytes)?;
        std::fs::rename(&tmp_path, self.pointer_path())?;
        Ok(())
    }
}

/// S3-compatible (e.g. DigitalOcean Spaces) `ProofStore`. Mirrors
/// `LocalProofStore`'s pointer-file design: a JSON pointer object
/// (`{key_prefix}/latest.json`) records the height most recently proved and,
/// when a real proof was produced, the key of its compressed proof object.
/// Proof bytes needed to resume (`ResumePoint::PreviousProof`) are downloaded
/// to `proof_cache_dir` so the rest of the pipeline can load them like any
/// other local proof file.
pub struct S3ProofStore {
    bucket: Box<Bucket>,
    key_prefix: String,
    proof_cache_dir: PathBuf,
}

impl S3ProofStore {
    pub fn new(
        bucket_name: &str,
        region: &str,
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        key_prefix: impl Into<String>,
        proof_cache_dir: impl Into<PathBuf>,
    ) -> Result<Self, BoxError> {
        let region = Region::Custom {
            region: region.to_string(),
            endpoint: endpoint.to_string(),
        };
        let credentials = Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .map_err(|err| format!("invalid S3 credentials: {err}"))?;
        let bucket = Bucket::new(bucket_name, region, credentials)
            .map_err(|err| format!("failed to construct S3 bucket client: {err}"))?
            .with_path_style();

        Ok(Self {
            bucket,
            key_prefix: key_prefix.into().trim_matches('/').to_string(),
            proof_cache_dir: proof_cache_dir.into(),
        })
    }

    fn key(&self, name: &str) -> String {
        if self.key_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.key_prefix)
        }
    }

    fn pointer_key(&self) -> String {
        self.key(POINTER_FILE_NAME)
    }

    fn download_proof(&self, proof_key: &str) -> Result<PathBuf, BoxError> {
        let response = self
            .bucket
            .get_object(proof_key)
            .map_err(|err| format!("failed to download proof object {proof_key}: {err}"))?;
        if response.status_code() != 200 {
            return Err(format!(
                "unexpected status {} downloading proof object {proof_key}",
                response.status_code()
            )
            .into());
        }

        std::fs::create_dir_all(&self.proof_cache_dir)?;
        let file_name = proof_key.rsplit('/').next().unwrap_or(proof_key);
        let local_path = self.proof_cache_dir.join(file_name);
        std::fs::write(&local_path, response.as_slice())?;
        Ok(local_path)
    }
}

impl ProofStore for S3ProofStore {
    fn resume_point(&self) -> Result<ResumePoint, BoxError> {
        let pointer_key = self.pointer_key();
        let response = self
            .bucket
            .get_object(&pointer_key)
            .map_err(|err| format!("failed to fetch pointer object {pointer_key}: {err}"))?;

        if response.status_code() == 404 {
            return Ok(ResumePoint::Genesis);
        }
        if response.status_code() != 200 {
            return Err(format!(
                "unexpected status {} fetching pointer object {pointer_key}",
                response.status_code()
            )
            .into());
        }

        let pointer: serde_json::Value = serde_json::from_slice(response.as_slice())
            .map_err(|err| format!("malformed pointer object {pointer_key}: {err}"))?;
        let end_height = pointer
            .get("end_height")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("pointer object {pointer_key} missing end_height"))?
            as u32;

        match pointer.get("proof_key").and_then(serde_json::Value::as_str) {
            Some(proof_key) => Ok(ResumePoint::PreviousProof(self.download_proof(proof_key)?)),
            None => Ok(ResumePoint::TrustedHeight(end_height)),
        }
    }

    fn record_progress(
        &self,
        end_height: u32,
        compressed_path: Option<&Path>,
    ) -> Result<(), BoxError> {
        let proof_key = match compressed_path {
            Some(path) => {
                let file_name = path
                    .file_name()
                    .ok_or("compressed proof path has no file name")?
                    .to_string_lossy()
                    .into_owned();
                let proof_key = self.key(&file_name);
                let bytes = std::fs::read(path)?;
                let response = self
                    .bucket
                    .put_object(&proof_key, &bytes)
                    .map_err(|err| format!("failed to upload proof object {proof_key}: {err}"))?;
                if response.status_code() != 200 {
                    return Err(format!(
                        "unexpected status {} uploading proof object {proof_key}",
                        response.status_code()
                    )
                    .into());
                }
                Some(proof_key)
            }
            None => None,
        };

        let pointer = serde_json::json!({
            "end_height": end_height,
            "proof_key": proof_key,
        });
        let bytes = serde_json::to_vec_pretty(&pointer)?;
        let pointer_key = self.pointer_key();
        let response = self
            .bucket
            .put_object(&pointer_key, &bytes)
            .map_err(|err| format!("failed to upload pointer object {pointer_key}: {err}"))?;
        if response.status_code() != 200 {
            return Err(format!(
                "unexpected status {} uploading pointer object {pointer_key}",
                response.status_code()
            )
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zkpow-proof-store-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn resume_point_is_genesis_when_no_pointer_file_exists() {
        let dir = temp_store_dir("missing");
        let store = LocalProofStore::new(&dir);

        assert_eq!(store.resume_point().unwrap(), ResumePoint::Genesis);
    }

    #[test]
    fn record_progress_then_resume_round_trips_proof_path() {
        let dir = temp_store_dir("proof-path");
        let store = LocalProofStore::new(&dir);
        let proof_path = dir.join("proof_height_1_to_100.bin");

        store.record_progress(100, Some(&proof_path)).unwrap();

        assert_eq!(
            store.resume_point().unwrap(),
            ResumePoint::PreviousProof(proof_path)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn record_progress_without_proof_path_resumes_at_trusted_height() {
        let dir = temp_store_dir("trusted-height");
        let store = LocalProofStore::new(&dir);

        store.record_progress(2016, None).unwrap();

        assert_eq!(
            store.resume_point().unwrap(),
            ResumePoint::TrustedHeight(2016)
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Exercises `S3ProofStore` against a real, in-process S3-compatible HTTP
/// server (a minimal axum router implementing object PUT/GET semantics with
/// path-style routing), so the `rust-s3` client code in `S3ProofStore` makes
/// genuine signed HTTP requests over the network rather than being mocked
/// away. This mirrors the approach used to test `HttpControllerSource`
/// against a real `zkpow-controller-serve` router.
#[cfg(test)]
mod s3_tests {
    use super::*;
    use axum::body::Bytes;
    use axum::extract::{Path as AxumPath, State as AxumState};
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    type ObjectStore = Arc<Mutex<HashMap<String, Vec<u8>>>>;

    async fn get_object_handler(
        AxumState(store): AxumState<ObjectStore>,
        AxumPath((_bucket, key)): AxumPath<(String, String)>,
    ) -> (StatusCode, Vec<u8>) {
        match store.lock().unwrap().get(&key) {
            Some(bytes) => (StatusCode::OK, bytes.clone()),
            None => (StatusCode::NOT_FOUND, Vec::new()),
        }
    }

    async fn put_object_handler(
        AxumState(store): AxumState<ObjectStore>,
        AxumPath((_bucket, key)): AxumPath<(String, String)>,
        body: Bytes,
    ) -> StatusCode {
        store.lock().unwrap().insert(key, body.to_vec());
        StatusCode::OK
    }

    /// Spawns the mock S3 server on its own OS thread with its own tokio
    /// runtime, returning the bound port and a handle to the in-memory
    /// object map (so tests can assert on raw stored keys/bytes directly).
    fn spawn_mock_s3_server() -> (u16, ObjectStore) {
        let store: ObjectStore = Arc::new(Mutex::new(HashMap::new()));
        let store_for_server = store.clone();
        let (port_tx, port_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let app = Router::new()
                    .route(
                        "/{bucket}/{*key}",
                        get(get_object_handler).put(put_object_handler),
                    )
                    .with_state(store_for_server);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                port_tx.send(listener.local_addr().unwrap().port()).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });
        let port = port_rx.recv().unwrap();
        (port, store)
    }

    fn temp_cache_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zkpow-s3-proof-store-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn test_store(port: u16, prefix: &str, cache_dir: &Path) -> S3ProofStore {
        S3ProofStore::new(
            "test-bucket",
            "us-east-1",
            &format!("http://127.0.0.1:{port}"),
            "test-access-key",
            "test-secret-key",
            prefix,
            cache_dir,
        )
        .unwrap()
    }

    #[test]
    fn resume_point_is_genesis_when_no_pointer_object_exists() {
        let (port, _store) = spawn_mock_s3_server();
        let cache_dir = temp_cache_dir("missing");
        let store = test_store(port, "proofs", &cache_dir);

        assert_eq!(store.resume_point().unwrap(), ResumePoint::Genesis);
    }

    #[test]
    fn record_progress_then_resume_round_trips_proof_bytes() {
        let (port, _store) = spawn_mock_s3_server();
        let cache_dir = temp_cache_dir("proof-path");
        let store = test_store(port, "proofs", &cache_dir);

        std::fs::create_dir_all(&cache_dir).unwrap();
        let local_proof_path = cache_dir.join("source_proof_height_1_to_100.bin");
        std::fs::write(&local_proof_path, b"fake compressed proof bytes").unwrap();

        store.record_progress(100, Some(&local_proof_path)).unwrap();

        let downloaded_path = match store.resume_point().unwrap() {
            ResumePoint::PreviousProof(path) => path,
            other => panic!("expected PreviousProof, got {other:?}"),
        };
        assert_eq!(
            std::fs::read(&downloaded_path).unwrap(),
            b"fake compressed proof bytes"
        );

        std::fs::remove_dir_all(&cache_dir).ok();
    }

    #[test]
    fn record_progress_without_proof_path_resumes_at_trusted_height() {
        let (port, _store) = spawn_mock_s3_server();
        let cache_dir = temp_cache_dir("trusted-height");
        let store = test_store(port, "proofs", &cache_dir);

        store.record_progress(2016, None).unwrap();

        assert_eq!(
            store.resume_point().unwrap(),
            ResumePoint::TrustedHeight(2016)
        );

        std::fs::remove_dir_all(&cache_dir).ok();
    }

    #[test]
    fn record_progress_applies_key_prefix() {
        let (port, raw_store) = spawn_mock_s3_server();
        let cache_dir = temp_cache_dir("prefix");
        let store = test_store(port, "env-a", &cache_dir);

        store.record_progress(5, None).unwrap();

        assert!(raw_store.lock().unwrap().contains_key("env-a/latest.json"));

        std::fs::remove_dir_all(&cache_dir).ok();
    }
}
