//! Binary HTTP transport for a [`ControllerRuntime`].
//!
//! A deliberately thin layer: each route decodes nothing itself — request
//! bodies are handed as-is to the runtime's byte-level handlers (the same
//! functions the host's in-process local transport calls), and the encoded
//! response is returned as `application/octet-stream`. Frame layouts live in
//! [`crate::protocol`]. Error bodies are plain UTF-8 text.
//!
//! Routes:
//!   POST /v1/batch     pull-batch request frame  -> batch frame
//!   POST /v1/complete  complete request frame    -> complete ack frame

use axum::body::Bytes;
use axum::extract::State as AxumState;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;

use crate::protocol::{ControllerError, ErrorKind};
use crate::runtime::ControllerRuntime;

pub fn build_router(runtime: ControllerRuntime) -> Router {
    Router::new()
        .route("/v1/batch", post(pull_batch_handler))
        .route("/v1/complete", post(complete_handler))
        .with_state(runtime)
}

fn into_response(err: ControllerError) -> (StatusCode, String) {
    let status = match err.kind {
        ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
        ErrorKind::Exhausted => StatusCode::NOT_FOUND,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::Timeout => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, err.message)
}

async fn pull_batch_handler(
    AxumState(runtime): AxumState<ControllerRuntime>,
    request: Bytes,
) -> Result<Vec<u8>, (StatusCode, String)> {
    runtime.handle_pull_batch(&request).await.map_err(into_response)
}

async fn complete_handler(
    AxumState(runtime): AxumState<ControllerRuntime>,
    request: Bytes,
) -> Result<Vec<u8>, (StatusCode, String)> {
    runtime.handle_complete(&request).await.map_err(into_response)
}
