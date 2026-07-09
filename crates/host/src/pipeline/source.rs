use zkpow_controller::{Controller, FetchedBatch};

use crate::pipeline::BoxError;
use crate::util;

/// Where a batch's headers and dry-run expected state come from. The local
/// implementation wraps a `Controller` over SQLite; future implementations
/// (e.g. an HTTP-backed source) can satisfy the same interface.
pub trait BatchSource {
    fn genesis_hash(&self) -> util::BlockHash;
    fn state_at(&self, height: u32) -> util::State;
    fn fetch_batch(
        &self,
        current_state: &util::State,
        num_headers: u32,
        simulate: bool,
    ) -> Result<FetchedBatch, BoxError>;
}

pub struct LocalControllerSource {
    controller: Controller,
}

impl LocalControllerSource {
    pub fn new(controller: Controller) -> Self {
        Self { controller }
    }
}

impl BatchSource for LocalControllerSource {
    fn genesis_hash(&self) -> util::BlockHash {
        self.controller.genesis_hash()
    }

    fn state_at(&self, height: u32) -> util::State {
        self.controller.state_at(height)
    }

    fn fetch_batch(
        &self,
        current_state: &util::State,
        num_headers: u32,
        simulate: bool,
    ) -> Result<FetchedBatch, BoxError> {
        self.controller
            .fetch_batch(current_state, num_headers, simulate)
    }
}

/// `BatchSource` backed by a remote `zkpow-controller-serve` HTTP server,
/// for running the host on a different machine from the headers database.
pub struct HttpControllerSource {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl HttpControllerSource {
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut base_url = base_url.into();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        Self {
            base_url,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn decode_hex_field(value: &serde_json::Value, field: &str) -> Result<Vec<u8>, BoxError> {
        let hex_str = value
            .get(field)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("response missing `{field}`"))?;
        Ok(hex::decode(hex_str).map_err(|err| format!("invalid hex in `{field}`: {err}"))?)
    }
}

impl BatchSource for HttpControllerSource {
    fn genesis_hash(&self) -> util::BlockHash {
        let response: serde_json::Value = self
            .client
            .get(format!("{}/genesis_hash", self.base_url))
            .send()
            .unwrap_or_else(|err| panic!("genesis_hash request failed: {err}"))
            .json()
            .unwrap_or_else(|err| panic!("genesis_hash response was not JSON: {err}"));
        let bytes = Self::decode_hex_field(&response, "genesis_hash_hex")
            .unwrap_or_else(|err| panic!("{err}"));
        let array: [u8; 32] = bytes
            .try_into()
            .unwrap_or_else(|_| panic!("genesis_hash_hex was not 32 bytes"));
        util::BlockHash::new(array)
    }

    fn state_at(&self, height: u32) -> util::State {
        let response: serde_json::Value = self
            .client
            .get(format!("{}/state/{height}", self.base_url))
            .send()
            .unwrap_or_else(|err| panic!("state_at({height}) request failed: {err}"))
            .json()
            .unwrap_or_else(|err| panic!("state_at({height}) response was not JSON: {err}"));
        let bytes =
            Self::decode_hex_field(&response, "state_hex").unwrap_or_else(|err| panic!("{err}"));
        util::State::parse(&bytes)
            .unwrap_or_else(|err| panic!("state_at({height}) response failed to parse: {err}"))
    }

    fn fetch_batch(
        &self,
        current_state: &util::State,
        num_headers: u32,
        simulate: bool,
    ) -> Result<FetchedBatch, BoxError> {
        let body = serde_json::json!({
            "current_state_hex": hex::encode(current_state.to_bytes()),
        });
        let response = self
            .client
            .post(format!("{}/batch", self.base_url))
            .query(&[
                ("num_headers", num_headers.to_string()),
                ("simulate", simulate.to_string()),
            ])
            .json(&body)
            .send()
            .map_err(|err| format!("batch request failed: {err}"))?;

        let status = response.status();
        let value: serde_json::Value = response
            .json()
            .map_err(|err| format!("batch response was not JSON: {err}"))?;

        if !status.is_success() {
            let message = value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown error")
                .to_string();
            return Err(message.into());
        }

        let headers_bytes = Self::decode_hex_field(&value, "headers_hex")?;
        let headers = util::parse_new_headers(&headers_bytes)
            .map_err(|err| format!("failed to parse headers_hex: {err}"))?
            .to_vec();
        let median_hints_bytes = Self::decode_hex_field(&value, "median_hints_hex")?;
        let median_hints = util::parse_median_hints(&median_hints_bytes)
            .map_err(|err| format!("failed to parse median_hints_hex: {err}"))?
            .to_vec();
        let expected_state = match value.get("expected_state_hex") {
            Some(serde_json::Value::String(hex_str)) => {
                let bytes = hex::decode(hex_str)
                    .map_err(|err| format!("invalid hex in `expected_state_hex`: {err}"))?;
                Some(
                    util::State::parse(&bytes)
                        .map_err(|err| format!("failed to parse expected_state_hex: {err}"))?,
                )
            }
            _ => None,
        };
        let expected_continuation_digest = match value.get("expected_continuation_digest_hex") {
            Some(serde_json::Value::String(hex_str)) => {
                let bytes = hex::decode(hex_str).map_err(|err| {
                    format!("invalid hex in `expected_continuation_digest_hex`: {err}")
                })?;
                let array: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| "expected_continuation_digest_hex was not 32 bytes")?;
                Some(array)
            }
            _ => None,
        };
        let first_new_height = value
            .get("first_new_height")
            .and_then(serde_json::Value::as_u64)
            .ok_or("response missing `first_new_height`")? as u32;
        let end_height = value
            .get("end_height")
            .and_then(serde_json::Value::as_u64)
            .ok_or("response missing `end_height`")? as u32;

        Ok(FetchedBatch {
            headers,
            median_hints,
            expected_state,
            expected_continuation_digest,
            first_new_height,
            end_height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use zkpow_controller::HeaderStore;

    /// Spawns a `zkpow-controller-serve` router on a random port, on its own
    /// dedicated OS thread with its own tokio runtime, and returns an
    /// `HttpControllerSource` pointed at it alongside a `LocalControllerSource`
    /// over a second connection to the same DB, so the two implementations of
    /// `BatchSource` can be compared directly.
    ///
    /// The server deliberately runs outside of the calling test's runtime:
    /// `HttpControllerSource` uses `reqwest::blocking`, which cannot be
    /// constructed or dropped from within an async task context.
    fn local_and_http_sources() -> (LocalControllerSource, HttpControllerSource) {
        let genesis_hash = crate::pipeline::input::genesis_hash();

        let local_db = HeaderStore::open(zkpow_controller::db_path()).unwrap();
        let local_source = LocalControllerSource::new(Controller::new(local_db, genesis_hash));

        let (port_tx, port_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let server_db = HeaderStore::open(zkpow_controller::db_path()).unwrap();
                let server_controller =
                    Arc::new(Mutex::new(Controller::new(server_db, genesis_hash)));
                let app = zkpow_controller::http::build_router(server_controller);

                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                port_tx.send(listener.local_addr().unwrap().port()).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });
        let port = port_rx.recv().unwrap();

        let http_source = HttpControllerSource::new(format!("http://127.0.0.1:{port}"));
        (local_source, http_source)
    }

    #[test]
    fn http_source_genesis_hash_matches_local() {
        let (local_source, http_source) = local_and_http_sources();
        assert_eq!(local_source.genesis_hash(), http_source.genesis_hash());
    }

    #[test]
    fn http_source_state_at_matches_local() {
        let (local_source, http_source) = local_and_http_sources();
        assert_eq!(local_source.state_at(0), http_source.state_at(0));
        assert_eq!(local_source.state_at(10), http_source.state_at(10));
    }

    #[test]
    fn http_source_fetch_batch_matches_local() {
        let (local_source, http_source) = local_and_http_sources();
        let genesis_state = local_source.state_at(0);

        let local_batch = local_source
            .fetch_batch(&genesis_state, 13, true)
            .expect("local fetch_batch should succeed");
        let http_batch = http_source
            .fetch_batch(&genesis_state, 13, true)
            .expect("http fetch_batch should succeed");

        assert_eq!(local_batch.headers, http_batch.headers);
        assert_eq!(local_batch.median_hints, http_batch.median_hints);
        assert_eq!(local_batch.expected_state, http_batch.expected_state);
        assert_eq!(
            local_batch.expected_continuation_digest,
            http_batch.expected_continuation_digest
        );
        assert_eq!(local_batch.first_new_height, http_batch.first_new_height);
        assert_eq!(local_batch.end_height, http_batch.end_height);
    }

    #[test]
    fn http_source_fetch_batch_error_matches_local_prefix() {
        let (local_source, http_source) = local_and_http_sources();
        let genesis_state = local_source.state_at(0);

        let local_err = local_source
            .fetch_batch(&genesis_state, 0, false)
            .expect_err("local fetch_batch with 0 headers should fail");
        let http_err = http_source
            .fetch_batch(&genesis_state, 0, false)
            .expect_err("http fetch_batch with 0 headers should fail");

        assert_eq!(local_err.to_string(), http_err.to_string());
    }
}
