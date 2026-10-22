use std::path::PathBuf;

use crate::BoxError;

pub trait ArtifactStore: Send + Sync {
    /// Target the host should upload `object` to.
    fn put_target(&self, run_id: &str, object: &str) -> Result<String, BoxError>;
    /// Target the host should download `object` from.
    fn get_target(&self, run_id: &str, object: &str) -> Result<String, BoxError>;
    /// Read `object` back on the controller side for completion verification.
    fn fetch(&self, run_id: &str, object: &str) -> Result<Vec<u8>, BoxError>;
}

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
    fn put_target(&self, run_id: &str, object: &str) -> Result<String, BoxError> {
        let dir = self.root.join(run_id);
        std::fs::create_dir_all(&dir)?;
        Ok(self.object_path(run_id, object).to_string_lossy().into_owned())
    }

    fn get_target(&self, run_id: &str, object: &str) -> Result<String, BoxError> {
        let path = self.object_path(run_id, object);
        if !path.exists() {
            return Err(format!("run {run_id} has no {object} under {}", self.root.display()).into());
        }
        Ok(path.to_string_lossy().into_owned())
    }

    fn fetch(&self, run_id: &str, object: &str) -> Result<Vec<u8>, BoxError> {
        Ok(std::fs::read(self.object_path(run_id, object))?)
    }
}
