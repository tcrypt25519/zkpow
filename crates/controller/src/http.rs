//! Axum router exposing a `Controller` over HTTP. Shared between the
//! `zkpow-controller-serve` binary and integration tests that spin up an
//! in-process server.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State as AxumState};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::{
    serialize_median_hints, serialize_new_headers, Controller, State, NO_HEADERS_REMAINING_PREFIX,
};

pub type SharedController = Arc<Mutex<Controller>>;

#[derive(serde::Deserialize)]
struct BatchQuery {
    num_headers: u32,
    #[serde(default)]
    simulate: bool,
}

pub fn build_router(controller: SharedController) -> Router {
    Router::new()
        .route("/genesis_hash", get(genesis_hash_handler))
        .route("/state/{height}", get(state_handler))
        .route("/batch", post(batch_handler))
        .with_state(controller)
}

/// A request that panics while holding this lock (e.g. an out-of-range
/// height hitting a `.expect()` deep in `HeaderStore`) poisons a plain
/// `Mutex::lock().unwrap()`, which would otherwise make every later request
/// panic too and take the whole server down permanently. The panic happens
/// entirely within the read-only query call, never while mutating
/// `Controller`'s own fields, so recovering the guard from a poisoned lock
/// is sound — the data behind it was never left inconsistent.
fn lock_controller(controller: &SharedController) -> std::sync::MutexGuard<'_, Controller> {
    controller
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

async fn genesis_hash_handler(AxumState(controller): AxumState<SharedController>) -> Json<Value> {
    let genesis_hash = lock_controller(&controller).genesis_hash();
    Json(json!({ "genesis_hash_hex": hex::encode(genesis_hash.into_inner()) }))
}

async fn state_handler(
    AxumState(controller): AxumState<SharedController>,
    Path(height): Path<u32>,
) -> Json<Value> {
    let state = lock_controller(&controller).state_at(height);
    Json(json!({ "state_hex": hex::encode(state.to_bytes()) }))
}

async fn batch_handler(
    AxumState(controller): AxumState<SharedController>,
    Query(query): Query<BatchQuery>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let current_state_hex = match body.get("current_state_hex").and_then(Value::as_str) {
        Some(hex_str) => hex_str,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing current_state_hex in request body" })),
            )
        }
    };
    let current_state_bytes = match hex::decode(current_state_hex) {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid current_state_hex: {err}") })),
            )
        }
    };
    let current_state = match State::parse(&current_state_bytes) {
        Ok(state) => state,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("failed to parse current state: {err}") })),
            )
        }
    };

    let fetched =
        lock_controller(&controller).fetch_batch(&current_state, query.num_headers, query.simulate);

    match fetched {
        Ok(batch) => {
            let headers_hex = hex::encode(serialize_new_headers(&batch.headers));
            let median_hints_hex = hex::encode(serialize_median_hints(&batch.median_hints));
            let expected_state_hex = batch
                .expected_state
                .as_ref()
                .map(|s| hex::encode(s.to_bytes()));
            let expected_continuation_digest_hex =
                batch.expected_continuation_digest.map(hex::encode);

            (
                StatusCode::OK,
                Json(json!({
                    "headers_hex": headers_hex,
                    "median_hints_hex": median_hints_hex,
                    "expected_state_hex": expected_state_hex,
                    "expected_continuation_digest_hex": expected_continuation_digest_hex,
                    "first_new_height": batch.first_new_height,
                    "end_height": batch.end_height,
                })),
            )
        }
        Err(err) => {
            let message = err.to_string();
            let status = if message.starts_with(NO_HEADERS_REMAINING_PREFIX) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": message })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db_path, sha256d, BlockHash, HeaderStore};

    fn test_controller() -> SharedController {
        let db = HeaderStore::open(db_path()).expect("failed to open test database");
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        Arc::new(Mutex::new(Controller::new(db, genesis_hash)))
    }

    #[tokio::test]
    async fn panicking_request_does_not_poison_lock_for_later_requests() {
        let controller = test_controller();

        let panicked = tokio::spawn(state_handler(AxumState(controller.clone()), Path(u32::MAX)))
            .await
            .is_err();
        assert!(panicked, "expected an out-of-range height to panic");

        // The lock must not be poisoned by the panic above: a later, valid
        // request on the same shared `Controller` must still succeed.
        let Json(value) = genesis_hash_handler(AxumState(controller)).await;
        assert!(value.get("genesis_hash_hex").is_some());
    }
}
