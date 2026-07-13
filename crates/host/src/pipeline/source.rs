use zkpow_controller::{Controller, FetchedBatch, SessionStart};

use crate::pipeline::BoxError;

/// Controller transport used by the host. The controller owns the session
/// cursor; the host only receives work and reports guest/prover output.
pub trait ControllerClient {
    fn begin_session(&mut self, start: SessionStart) -> Result<(), BoxError>;
    fn next_work(&mut self, num_headers: u32) -> Result<FetchedBatch, BoxError>;
    fn complete_work(&mut self, public_values: &[u8]) -> Result<(), BoxError>;
}

pub struct LocalControllerSource {
    controller: Controller,
    session_id: Option<u64>,
}

impl LocalControllerSource {
    pub fn new(controller: Controller) -> Self {
        Self {
            controller,
            session_id: None,
        }
    }
}

impl ControllerClient for LocalControllerSource {
    fn begin_session(&mut self, start: SessionStart) -> Result<(), BoxError> {
        self.session_id = Some(self.controller.begin_session(start)?);
        Ok(())
    }

    fn next_work(&mut self, num_headers: u32) -> Result<FetchedBatch, BoxError> {
        let session_id = self
            .session_id
            .ok_or("controller session is not initialized")?;
        self.controller.next_work(session_id, num_headers)
    }

    fn complete_work(&mut self, public_values: &[u8]) -> Result<(), BoxError> {
        let session_id = self
            .session_id
            .ok_or("controller session is not initialized")?;
        self.controller.complete_work(session_id, public_values)
    }
}

/// `BatchSource` backed by a remote `zkpow-controller-serve` HTTP server,
/// for running the host on a different machine from the headers database.
/// The controller transport is raw fixed-width bytes; it has no JSON or hex
/// encoding layer.
pub struct HttpControllerSource {
    base_url: String,
    client: reqwest::blocking::Client,
    session_id: Option<u64>,
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
            session_id: None,
        }
    }

    fn post_bytes(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, BoxError> {
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .body(body)
            .send()
            .map_err(|err| format!("controller request {path} failed: {err}"))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .map_err(|err| format!("controller response {path} body read failed: {err}"))?;
        if !status.is_success() {
            return Err(String::from_utf8_lossy(&bytes).into_owned().into());
        }
        Ok(bytes.to_vec())
    }
}

impl ControllerClient for HttpControllerSource {
    fn begin_session(&mut self, start: SessionStart) -> Result<(), BoxError> {
        let bytes = self.post_bytes(
            "/session",
            zkpow_controller::http::encode_session_start(&start),
        )?;
        let session_id: [u8; 8] = bytes
            .try_into()
            .map_err(|_| "controller session response was not 8 bytes")?;
        self.session_id = Some(u64::from_le_bytes(session_id));
        Ok(())
    }

    fn next_work(&mut self, num_headers: u32) -> Result<FetchedBatch, BoxError> {
        let session_id = self
            .session_id
            .ok_or("controller session is not initialized")?;
        let bytes = self.post_bytes(
            &format!("/session/{session_id}/work/{num_headers}"),
            Vec::new(),
        )?;
        zkpow_controller::http::decode_batch_response(&bytes)
            .map_err(|err| format!("invalid binary batch response: {err}").into())
    }

    fn complete_work(&mut self, public_values: &[u8]) -> Result<(), BoxError> {
        let session_id = self
            .session_id
            .ok_or("controller session is not initialized")?;
        self.post_bytes(
            &format!("/session/{session_id}/complete"),
            public_values.to_vec(),
        )?;
        Ok(())
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
    /// `ControllerClient` can be compared directly.
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
    fn http_controller_session_matches_local() {
        let (mut local_source, mut http_source) = local_and_http_sources();
        local_source.begin_session(SessionStart::Genesis).unwrap();
        http_source.begin_session(SessionStart::Genesis).unwrap();

        let local_batch = local_source.next_work(13).unwrap();
        let http_batch = http_source.next_work(13).unwrap();

        assert_eq!(local_batch.current_state, http_batch.current_state);
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
    fn controller_session_rejects_zero_header_work_consistently() {
        let (mut local_source, mut http_source) = local_and_http_sources();
        local_source.begin_session(SessionStart::Genesis).unwrap();
        http_source.begin_session(SessionStart::Genesis).unwrap();

        let local_err = local_source.next_work(0).unwrap_err();
        let http_err = http_source.next_work(0).unwrap_err();

        assert_eq!(local_err.to_string(), http_err.to_string());
    }
}
