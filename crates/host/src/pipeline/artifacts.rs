//! Moving proof artifacts to and from controller-chosen local paths.
//!
//! The host never decides where artifacts live: every batch grant carries
//! upload (and, on resume, download) paths minted by the controller. The
//! host just reads and writes them.

use crate::pipeline::BoxError;

pub async fn put_artifact(target: &str, bytes: Vec<u8>) -> Result<(), BoxError> {
    let path = std::path::Path::new(target);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, &bytes)
        .await
        .map_err(|err| format!("failed to write artifact {}: {err}", path.display()))?;
    Ok(())
}

pub async fn fetch_artifact(target: &str) -> Result<Vec<u8>, BoxError> {
    Ok(tokio::fs::read(target)
        .await
        .map_err(|err| format!("failed to read artifact {target}: {err}"))?)
}
