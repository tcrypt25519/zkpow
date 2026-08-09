//! Moving proof artifacts to and from controller-chosen targets.
//!
//! The host never decides where artifacts live: every batch grant carries
//! upload (and, on resume, download) targets minted by the controller —
//! pre-signed URLs for object storage, or filesystem paths when host and
//! controller share a disk. The host just PUTs and GETs.

use std::path::Path;

use zkpow_controller::protocol::FileLocation;

use crate::pipeline::BoxError;

pub async fn put_artifact(client: &reqwest::Client, target: &FileLocation, bytes: Vec<u8>) -> Result<(), BoxError> {
    match target {
        FileLocation::Path(path) => {
            let path = Path::new(path);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(path, &bytes)
                .await
                .map_err(|err| format!("failed to write artifact {}: {err}", path.display()))?;
        },
        FileLocation::Url(url) => {
            let response = client
                .put(url.clone())
                .body(bytes)
                .send()
                .await
                .map_err(|err| format!("artifact upload failed: {err}"))?;
            if !response.status().is_success() {
                return Err(format!("artifact upload returned status {}", response.status()).into());
            }
        },
    }
    Ok(())
}

pub async fn fetch_artifact(client: &reqwest::Client, target: &FileLocation) -> Result<Vec<u8>, BoxError> {
    match target {
        FileLocation::Path(path) => Ok(tokio::fs::read(path)
            .await
            .map_err(|err| format!("failed to read artifact {path}: {err}"))?),
        FileLocation::Url(url) => {
            let response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|err| format!("artifact download failed: {err}"))?;
            if !response.status().is_success() {
                return Err(format!("artifact download returned status {}", response.status()).into());
            }
            Ok(response
                .bytes()
                .await
                .map_err(|err| format!("artifact download body read failed: {err}"))?
                .to_vec())
        },
    }
}
