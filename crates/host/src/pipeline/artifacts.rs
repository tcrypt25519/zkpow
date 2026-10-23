//! Moving proof artifacts to and from controller-chosen local paths.
//!
//! The host never decides where artifacts live: the controller sets the
//! upload (and, on resume, download) paths in each batch grant. The host
//! just reads and writes them.

use crate::pipeline::BoxError;

pub async fn put_artifact(target: &str, bytes: Vec<u8>) -> Result<(), BoxError> {
    let path = std::path::Path::new(target);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Write via a unique temporary file in the same directory and atomically
    // rename so interrupted writes never leave corrupted/partial artifacts.
    let tmp_path = format!("{target}.tmp.{}", std::process::id());
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .map_err(|err| format!("failed to write artifact {tmp_path}: {err}"))?;
    tokio::fs::rename(&tmp_path, path)
        .await
        .map_err(|err| format!("failed to atomically commit artifact {}: {err}", path.display()))?;
    Ok(())
}

pub async fn fetch_artifact(target: &str) -> Result<Vec<u8>, BoxError> {
    Ok(tokio::fs::read(target)
        .await
        .map_err(|err| format!("failed to read artifact {target}: {err}"))?)
}
