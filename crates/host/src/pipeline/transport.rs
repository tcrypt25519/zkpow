//! Host-side controller transports.
//!
//! * [`HttpController`] POSTs encoded frames to a remote `zkpow-controller-serve` (e.g. over a Tailscale-private
//!   address).
//! * [`LocalController`] calls the typed runtime API directly.

use std::future::Future;

use zkpow_controller::{protocol::Batch, runtime::ControllerRuntime};

use crate::pipeline::BoxError;

pub trait ControllerTransport: Send + Sync + 'static {
    fn pull_batch(&self, run_id: &str) -> impl Future<Output = Result<Batch, BoxError>> + Send;

    fn complete(&self, run_id: &str) -> impl Future<Output = Result<(), BoxError>> + Send;
}

/// In-process controller: the "local" mode. The controller owns the header
/// database, ledger, and storage exactly as it would behind HTTP; the host
/// calls its typed API directly.
pub struct LocalController {
    runtime: ControllerRuntime,
}

impl LocalController {
    pub fn new(runtime: ControllerRuntime) -> Self {
        Self { runtime }
    }
}

impl ControllerTransport for LocalController {
    async fn pull_batch(&self, run_id: &str) -> Result<Batch, BoxError> {
        Ok(self.runtime.pull_batch(run_id).await?)
    }

    async fn complete(&self, run_id: &str) -> Result<(), BoxError> {
        Ok(self.runtime.complete(run_id).await?)
    }
}

/// Remote controller over postcard-encoded HTTP. Pulls may long-poll, so the
/// client has no request timeout.
pub struct HttpController {
    base_url: String,
    client:   reqwest::Client,
}

impl HttpController {
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut base_url = base_url.into();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    async fn post_bytes(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, BoxError> {
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .body(body)
            .send()
            .await
            .map_err(|err| format!("controller request {path} failed: {err}"))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|err| format!("controller response {path} body read failed: {err}"))?;
        if !status.is_success() {
            return Err(String::from_utf8_lossy(&bytes).into_owned().into());
        }
        Ok(bytes.to_vec())
    }
}

impl ControllerTransport for HttpController {
    async fn pull_batch(&self, run_id: &str) -> Result<Batch, BoxError> {
        let response = self.post_bytes("/v1/batch", run_id.as_bytes().to_vec()).await?;
        let batch: Batch =
            rkyv::from_bytes::<Batch, rkyv::rancor::Error>(&response).map_err(|err| format!("invalid batch: {err}"))?;
        batch.validate().map_err(|err| format!("invalid batch: {err}"))?;
        Ok(batch)
    }

    async fn complete(&self, run_id: &str) -> Result<(), BoxError> {
        self.post_bytes("/v1/complete", run_id.as_bytes().to_vec()).await?;
        Ok(())
    }
}
