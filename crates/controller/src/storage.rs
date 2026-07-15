//! Controller-owned artifact storage.
//!
//! The controller decides where proof artifacts live and hands the host
//! opaque [`Target`]s to write to (and read from, when resuming). The host
//! never holds storage credentials: an S3-backed store mints pre-signed
//! URLs, a local store mints filesystem paths. The controller can also
//! `fetch` an artifact back to verify an uploaded proof itself.
//!
//! Objects are laid out per run: `<prefix>/<run_id>/<object>`.

use std::path::PathBuf;

use s3::creds::Credentials;
use s3::{Bucket, Region};

use crate::protocol::Target;
use crate::BoxError;

/// How long a minted pre-signed URL stays valid. Generous: a URL is used
/// within one batch cycle, but proving a large batch can take a while.
const PRESIGN_EXPIRY_SECS: u32 = 6 * 60 * 60;

pub trait ArtifactStore: Send + Sync {
    /// Target the host should upload `object` to.
    fn put_target(&self, run_id: &str, object: &str) -> Result<Target, BoxError>;
    /// Target the host should download `object` from.
    fn get_target(&self, run_id: &str, object: &str) -> Result<Target, BoxError>;
    /// Read `object` back on the controller side (proof verification).
    fn fetch(&self, run_id: &str, object: &str) -> Result<Vec<u8>, BoxError>;
}

/// Filesystem-backed store. Only correct when host and controller share a
/// filesystem (in-process local mode, or a single-machine deployment).
pub struct LocalArtifactStore {
    root: PathBuf,
}

impl LocalArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn object_path(&self, run_id: &str, object: &str) -> PathBuf {
        self.root.join(run_id).join(object)
    }
}

impl ArtifactStore for LocalArtifactStore {
    fn put_target(&self, run_id: &str, object: &str) -> Result<Target, BoxError> {
        let dir = self.root.join(run_id);
        std::fs::create_dir_all(&dir)?;
        Ok(Target::Path(self.object_path(run_id, object).to_string_lossy().into_owned()))
    }

    fn get_target(&self, run_id: &str, object: &str) -> Result<Target, BoxError> {
        Ok(Target::Path(self.object_path(run_id, object).to_string_lossy().into_owned()))
    }

    fn fetch(&self, run_id: &str, object: &str) -> Result<Vec<u8>, BoxError> {
        Ok(std::fs::read(self.object_path(run_id, object))?)
    }
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub prefix: String,
}

/// S3-compatible store (e.g. DigitalOcean Spaces). Mints pre-signed PUT/GET
/// URLs for the host and fetches objects directly for verification.
pub struct S3ArtifactStore {
    bucket: Box<Bucket>,
    key_prefix: String,
}

impl S3ArtifactStore {
    pub fn new(config: &S3Config) -> Result<Self, BoxError> {
        let region = Region::Custom {
            region: config.region.clone(),
            endpoint: config.endpoint.clone(),
        };
        let credentials = Credentials::new(
            Some(&config.access_key),
            Some(&config.secret_key),
            None,
            None,
            None,
        )
        .map_err(|err| format!("invalid S3 credentials: {err}"))?;
        let bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|err| format!("failed to construct S3 bucket client: {err}"))?
            .with_path_style();

        Ok(Self { bucket, key_prefix: config.prefix.trim_matches('/').to_string() })
    }

    fn key(&self, run_id: &str, object: &str) -> String {
        if self.key_prefix.is_empty() {
            format!("{run_id}/{object}")
        } else {
            format!("{}/{run_id}/{object}", self.key_prefix)
        }
    }
}

impl ArtifactStore for S3ArtifactStore {
    fn put_target(&self, run_id: &str, object: &str) -> Result<Target, BoxError> {
        let key = self.key(run_id, object);
        let url = self
            .bucket
            .presign_put(format!("/{key}"), PRESIGN_EXPIRY_SECS, None, None)
            .map_err(|err| format!("failed to presign PUT for {key}: {err}"))?;
        Ok(Target::Url(url))
    }

    fn get_target(&self, run_id: &str, object: &str) -> Result<Target, BoxError> {
        let key = self.key(run_id, object);
        let url = self
            .bucket
            .presign_get(format!("/{key}"), PRESIGN_EXPIRY_SECS, None)
            .map_err(|err| format!("failed to presign GET for {key}: {err}"))?;
        Ok(Target::Url(url))
    }

    fn fetch(&self, run_id: &str, object: &str) -> Result<Vec<u8>, BoxError> {
        let key = self.key(run_id, object);
        let response = self
            .bucket
            .get_object(&key)
            .map_err(|err| format!("failed to fetch artifact {key}: {err}"))?;
        if response.status_code() != 200 {
            return Err(format!(
                "unexpected status {} fetching artifact {key}",
                response.status_code()
            )
            .into());
        }
        Ok(response.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(test_name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zkpow-artifact-store-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn local_store_round_trips_artifacts() {
        let root = temp_root("round-trip");
        let store = LocalArtifactStore::new(&root);

        let put = store.put_target("run-a", "proof.bin").unwrap();
        let Target::Path(path) = &put else { panic!("expected path target") };
        std::fs::write(path, b"proof bytes").unwrap();

        assert_eq!(store.fetch("run-a", "proof.bin").unwrap(), b"proof bytes");
        assert_eq!(store.get_target("run-a", "proof.bin").unwrap(), put);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn local_store_segments_by_run_id() {
        let root = temp_root("segments");
        let store = LocalArtifactStore::new(&root);

        let Target::Path(path) = store.put_target("run-b", "proof.bin").unwrap() else {
            panic!("expected path target")
        };
        assert!(path.contains("run-b"), "path should be segmented by run id: {path}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn s3_store_mints_url_targets_with_run_scoped_keys() {
        let store = S3ArtifactStore::new(&S3Config {
            bucket: "test-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: "http://127.0.0.1:9000".to_string(),
            access_key: "ak".to_string(),
            secret_key: "sk".to_string(),
            prefix: "proofs".to_string(),
        })
        .unwrap();

        let Target::Url(url) = store.put_target("run-c", "proof.bin").unwrap() else {
            panic!("expected url target")
        };
        assert!(url.contains("test-bucket"), "presigned url should reference the bucket: {url}");
        assert!(
            url.contains("proofs/run-c/proof.bin"),
            "presigned url should carry the run-scoped key: {url}"
        );
        assert!(url.contains("X-Amz-Signature"), "presigned url should be signed: {url}");
    }
}
