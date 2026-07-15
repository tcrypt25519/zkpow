//! Host-side controller transports.
//!
//! The host is a dumb pipe: it never validates block data or results, it
//! only moves protocol frames between the controller and the guest. Both
//! transports speak the identical binary protocol from
//! [`zkpow_controller::protocol`]:
//!
//! * [`HttpController`] POSTs encoded frames to a remote
//!   `zkpow-controller-serve` (e.g. over a Tailscale-private address).
//! * [`LocalController`] runs the controller in-process and round-trips the
//!   same encoded frames through the runtime's byte-level handlers — method
//!   calls instead of sockets, but byte-identical requests, responses, and
//!   error messages.

use std::future::Future;

use zkpow_controller::protocol::{
    decode_batch_grant, decode_complete_ack, encode_complete_request, encode_pull_batch_request,
    BatchGrant, CompleteAck, CompleteRequest, PullBatchRequest,
};
use zkpow_controller::runtime::ControllerRuntime;

use crate::pipeline::BoxError;

pub trait ControllerTransport: Send + Sync + 'static {
    fn pull_batch(
        &self,
        request: &PullBatchRequest,
    ) -> impl Future<Output = Result<BatchGrant, BoxError>> + Send;

    fn complete(
        &self,
        request: &CompleteRequest,
    ) -> impl Future<Output = Result<CompleteAck, BoxError>> + Send;
}

/// In-process controller: the "local" mode. The controller owns the header
/// database, ledger, and storage exactly as it would behind HTTP; the host
/// talks to it through the same encoded protocol frames.
pub struct LocalController {
    runtime: ControllerRuntime,
}

impl LocalController {
    pub fn new(runtime: ControllerRuntime) -> Self {
        Self { runtime }
    }
}

impl ControllerTransport for LocalController {
    async fn pull_batch(&self, request: &PullBatchRequest) -> Result<BatchGrant, BoxError> {
        let request = encode_pull_batch_request(request)?;
        let response = self.runtime.handle_pull_batch(&request).await?;
        Ok(decode_batch_grant(&response)
            .map_err(|err| format!("invalid batch grant frame: {err}"))?)
    }

    async fn complete(&self, request: &CompleteRequest) -> Result<CompleteAck, BoxError> {
        let request = encode_complete_request(request)?;
        let response = self.runtime.handle_complete(&request).await?;
        Ok(decode_complete_ack(&response)
            .map_err(|err| format!("invalid complete ack frame: {err}"))?)
    }
}

/// Remote controller over HTTP. Raw fixed-width bytes both ways; no JSON or
/// hex encoding layer. Pulls may long-poll, so the client has no request
/// timeout.
pub struct HttpController {
    base_url: String,
    client: reqwest::Client,
}

impl HttpController {
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut base_url = base_url.into();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        Self { base_url, client: reqwest::Client::new() }
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
    async fn pull_batch(&self, request: &PullBatchRequest) -> Result<BatchGrant, BoxError> {
        let request = encode_pull_batch_request(request)?;
        let response = self.post_bytes("/v1/batch", request).await?;
        Ok(decode_batch_grant(&response)
            .map_err(|err| format!("invalid batch grant frame: {err}"))?)
    }

    async fn complete(&self, request: &CompleteRequest) -> Result<CompleteAck, BoxError> {
        let request = encode_complete_request(request)?;
        let response = self.post_bytes("/v1/complete", request).await?;
        Ok(decode_complete_ack(&response)
            .map_err(|err| format!("invalid complete ack frame: {err}"))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use zkpow_controller::ledger::Ledger;
    use zkpow_controller::runtime::ControllerConfig;
    use zkpow_controller::storage::LocalArtifactStore;
    use zkpow_controller::{db_path, HeaderStore};
    use zkpow_core::VerifierKeyDigest;

    fn temp_path(test_name: &str, suffix: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zkpow-transport-{test_name}-{}-{nonce}{suffix}",
            std::process::id()
        ))
    }

    /// Both transports get their own runtime (own ledger) over the same
    /// header database and the same artifact root, so equal requests must
    /// produce byte-identical grants — including storage targets.
    fn test_runtime(test_name: &str, artifact_root: &std::path::Path) -> ControllerRuntime {
        let store = HeaderStore::open(db_path()).expect("failed to open test database");
        let ledger = Ledger::open(temp_path(test_name, ".db")).expect("failed to open ledger");
        let mut config = ControllerConfig::new(crate::pipeline::input::genesis_hash());
        config.allow_unproven = true;
        config.pull_timeout = Duration::from_millis(200);
        ControllerRuntime::new(
            store,
            ledger,
            Box::new(LocalArtifactStore::new(artifact_root)),
            None,
            config,
        )
    }

    async fn local_and_http_transports(test_name: &str) -> (LocalController, HttpController) {
        let artifact_root = temp_path(test_name, "-artifacts");

        let local = LocalController::new(test_runtime(test_name, &artifact_root));

        let http_runtime = test_runtime(&format!("{test_name}-http"), &artifact_root);
        let app = zkpow_controller::http::build_router(http_runtime);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        (local, HttpController::new(format!("http://127.0.0.1:{port}")))
    }

    fn request(num_headers: u32) -> PullBatchRequest {
        PullBatchRequest {
            run_id: "transport-parity".to_string(),
            verifier_key: VerifierKeyDigest::default(),
            num_headers,
            held_tip: None,
            start_height: None,
        }
    }

    #[tokio::test]
    async fn http_batch_grant_matches_local() {
        let (local, http) = local_and_http_transports("grant-parity").await;

        let local_grant = local.pull_batch(&request(13)).await.unwrap();
        let http_grant = http.pull_batch(&request(13)).await.unwrap();

        assert_eq!(local_grant, http_grant);
        assert_eq!(local_grant.first_new_height, 1);
        assert_eq!(local_grant.end_height, 13);
    }

    #[tokio::test]
    async fn transport_errors_match_between_local_and_http() {
        let (local, http) = local_and_http_transports("error-parity").await;

        let local_err = local.pull_batch(&request(0)).await.unwrap_err();
        let http_err = http.pull_batch(&request(0)).await.unwrap_err();

        assert_eq!(local_err.to_string(), http_err.to_string());
        assert!(local_err
            .to_string()
            .starts_with(zkpow_controller::NO_HEADERS_REMAINING_PREFIX));
    }
}
