//! Binary HTTP transport for a [`ControllerRuntime`].
//!
//! Request bodies are the UTF-8 run id. Only batch responses need binary
//! encoding. Error bodies are plain UTF-8 text.
//!
//! Routes:
//!   POST /v1/batch     run id -> batch frame
//!   POST /v1/complete  run id -> empty response

use axum::body::Bytes;
use axum::extract::State as AxumState;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;

use crate::protocol::{encode, ControllerError, ErrorKind};
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
    let run_id = std::str::from_utf8(&request)
        .map_err(|err| into_response(ControllerError::bad_request(err.to_string())))?;
    let batch = runtime.pull_batch(run_id).await.map_err(into_response)?;
    batch
        .validate()
        .map_err(ControllerError::internal)
        .map_err(into_response)?;
    encode(&batch)
        .map_err(ControllerError::internal)
        .map_err(into_response)
}

async fn complete_handler(
    AxumState(runtime): AxumState<ControllerRuntime>,
    request: Bytes,
) -> Result<(), (StatusCode, String)> {
    let run_id = std::str::from_utf8(&request)
        .map_err(|err| into_response(ControllerError::bad_request(err.to_string())))?;
    runtime.complete(run_id).await.map_err(into_response)
}
