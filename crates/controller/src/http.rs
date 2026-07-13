//! Binary HTTP transport for a [`Controller`].
//!
//! Every successful response is `application/octet-stream`. `POST /session`
//! starts a controller-owned session, `POST /session/{id}/work/{num_headers}`
//! returns the next batch, and `POST /session/{id}/complete` accepts guest or
//! prover public values. Work responses contain:
//!
//! ```text
//! first_new_height: u32 LE | end_height: u32 LE | median_hint_count: u32 LE
//! headers: (end_height - first_new_height + 1) * NEW_HEADER_SIZE bytes
//! median_hints: median_hint_count * 4 bytes
//! current_state: STATE_SIZE bytes
//! expected_state: STATE_SIZE bytes
//! expected_continuation_digest: 32 bytes
//! ```
//!
//! Error bodies are plain UTF-8 text. There is deliberately no JSON, hex, or
//! serde layer in this protocol.

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, State as AxumState};
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;

use crate::{BlockTimestamp, FetchedBatch, NewHeader, SessionStart, NO_HEADERS_REMAINING_PREFIX};

const BATCH_PREFIX_SIZE: usize = 12;

pub type SharedController = Arc<Mutex<crate::Controller>>;

pub fn build_router(controller: SharedController) -> Router {
    Router::new()
        .route("/session", post(begin_session_handler))
        .route(
            "/session/{session_id}/work/{num_headers}",
            post(next_work_handler),
        )
        .route(
            "/session/{session_id}/complete",
            post(complete_work_handler),
        )
        .with_state(controller)
}

/// A request that panics while holding this lock (e.g. an out-of-range
/// height hitting a `.expect()` deep in `HeaderStore`) poisons a plain
/// `Mutex::lock().unwrap()`, which would otherwise make every later request
/// panic too and take the whole server down permanently. The panic happens
/// entirely within the read-only query call, never while mutating
/// `Controller`'s own fields, so recovering the guard from a poisoned lock
/// is sound — the data behind it was never left inconsistent.
fn lock_controller(controller: &SharedController) -> std::sync::MutexGuard<'_, crate::Controller> {
    controller
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

async fn begin_session_handler(
    AxumState(controller): AxumState<SharedController>,
    start_bytes: Bytes,
) -> Result<Vec<u8>, (StatusCode, String)> {
    let start = decode_session_start(&start_bytes).map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    lock_controller(&controller)
        .begin_session(start)
        .map(|session_id| session_id.to_le_bytes().to_vec())
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

async fn next_work_handler(
    AxumState(controller): AxumState<SharedController>,
    Path((session_id, num_headers)): Path<(u64, u32)>,
) -> Result<Vec<u8>, (StatusCode, String)> {
    match lock_controller(&controller).next_work(session_id, num_headers) {
        Ok(batch) => {
            encode_batch_response(&batch).map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))
        }
        Err(err) => {
            let message = err.to_string();
            let status = if message.starts_with(NO_HEADERS_REMAINING_PREFIX) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((status, message))
        }
    }
}

async fn complete_work_handler(
    AxumState(controller): AxumState<SharedController>,
    Path(session_id): Path<u64>,
    public_values: Bytes,
) -> Result<(), (StatusCode, String)> {
    lock_controller(&controller)
        .complete_work(session_id, &public_values)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

/// Encode a controller session start request.
pub fn encode_session_start(start: &SessionStart) -> Vec<u8> {
    match start {
        SessionStart::Genesis => vec![0],
        SessionStart::TrustedHeight(height) => {
            let mut bytes = Vec::with_capacity(5);
            bytes.push(1);
            bytes.extend_from_slice(&height.to_le_bytes());
            bytes
        }
        SessionStart::PriorPublicValues(public_values) => {
            let mut bytes = Vec::with_capacity(1 + public_values.len());
            bytes.push(2);
            bytes.extend_from_slice(public_values);
            bytes
        }
    }
}

/// Decode a controller session start request.
pub fn decode_session_start(bytes: &[u8]) -> Result<SessionStart, String> {
    match bytes {
        [0] => Ok(SessionStart::Genesis),
        [1, height @ ..] if height.len() == 4 => Ok(SessionStart::TrustedHeight(
            u32::from_le_bytes(height.try_into().expect("fixed 4-byte height")),
        )),
        [2, public_values @ ..] if !public_values.is_empty() => {
            Ok(SessionStart::PriorPublicValues(public_values.to_vec()))
        }
        _ => Err("invalid controller session start request".to_string()),
    }
}

/// Encode a fetched batch into the fixed-width controller HTTP response.
pub fn encode_batch_response(batch: &FetchedBatch) -> Result<Vec<u8>, String> {
    let header_count = batch
        .end_height
        .checked_sub(batch.first_new_height)
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| "batch end height precedes first new height".to_string())?;
    if header_count as usize != batch.headers.len() {
        return Err(format!(
            "batch heights describe {header_count} headers, got {}",
            batch.headers.len()
        ));
    }

    let mut bytes = Vec::with_capacity(
        BATCH_PREFIX_SIZE
            + batch.headers.len() * zkpow_core::NEW_HEADER_SIZE
            + batch.median_hints.len() * size_of::<BlockTimestamp>()
            + zkpow_core::STATE_SIZE
            + zkpow_core::STATE_SIZE
            + 32,
    );
    bytes.extend_from_slice(&batch.first_new_height.to_le_bytes());
    bytes.extend_from_slice(&batch.end_height.to_le_bytes());
    bytes.extend_from_slice(&(batch.median_hints.len() as u32).to_le_bytes());
    for header in &batch.headers {
        bytes.extend_from_slice(&header.to_bytes());
    }
    for hint in &batch.median_hints {
        bytes.extend_from_slice(&hint.to_le_bytes());
    }
    bytes.extend_from_slice(&batch.current_state.to_bytes());
    bytes.extend_from_slice(&batch.expected_state.to_bytes());
    bytes.extend_from_slice(&batch.expected_continuation_digest);
    Ok(bytes)
}

/// Decode a fixed-width controller HTTP response into the host's batch type.
pub fn decode_batch_response(bytes: &[u8]) -> Result<FetchedBatch, String> {
    if bytes.len() < BATCH_PREFIX_SIZE {
        return Err(format!(
            "batch response shorter than {BATCH_PREFIX_SIZE}-byte prefix"
        ));
    }

    let first_new_height = read_u32(bytes, 0, "first new height")?;
    let end_height = read_u32(bytes, 4, "end height")?;
    let median_hint_count = read_u32(bytes, 8, "median hint count")? as usize;
    let header_count = end_height
        .checked_sub(first_new_height)
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| "batch end height precedes first new height".to_string())?
        as usize;
    let header_bytes_len = header_count
        .checked_mul(zkpow_core::NEW_HEADER_SIZE)
        .ok_or_else(|| "batch header payload length overflows usize".to_string())?;
    let hint_bytes_len = median_hint_count
        .checked_mul(size_of::<BlockTimestamp>())
        .ok_or_else(|| "batch median-hint payload length overflows usize".to_string())?;

    let mut cursor = BATCH_PREFIX_SIZE;
    let header_bytes = take(bytes, &mut cursor, header_bytes_len, "headers")?;
    let hint_bytes = take(bytes, &mut cursor, hint_bytes_len, "median hints")?;
    let current_state = crate::State::parse(take(
        bytes,
        &mut cursor,
        zkpow_core::STATE_SIZE,
        "current state",
    )?)
    .map_err(|err| format!("invalid current state in batch response: {err}"))?;
    let expected_state = crate::State::parse(take(
        bytes,
        &mut cursor,
        zkpow_core::STATE_SIZE,
        "expected state",
    )?)
    .map_err(|err| format!("invalid expected state in batch response: {err}"))?;
    let expected_continuation_digest =
        take(bytes, &mut cursor, 32, "expected continuation digest")?
            .try_into()
            .expect("fixed 32-byte digest slice");
    if cursor != bytes.len() {
        return Err(format!(
            "batch response has {} trailing bytes",
            bytes.len() - cursor
        ));
    }

    Ok(FetchedBatch {
        current_state,
        headers: decode_headers(header_bytes),
        median_hints: decode_median_hints(hint_bytes),
        expected_state,
        expected_continuation_digest,
        first_new_height,
        end_height,
    })
}

fn read_u32(bytes: &[u8], offset: usize, field: &str) -> Result<u32, String> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("batch response missing {field}"))?
        .try_into()
        .expect("fixed 4-byte field slice");
    Ok(u32::from_le_bytes(value))
}

fn take<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
    field: &str,
) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| format!("batch response {field} length overflows usize"))?;
    let field_bytes = bytes
        .get(*cursor..end)
        .ok_or_else(|| format!("batch response truncated in {field}"))?;
    *cursor = end;
    Ok(field_bytes)
}

fn decode_headers(bytes: &[u8]) -> Vec<NewHeader> {
    bytes
        .chunks_exact(zkpow_core::NEW_HEADER_SIZE)
        .map(|bytes| NewHeader {
            version: u32::from_le_bytes(bytes[0..4].try_into().expect("fixed header version")),
            merkle_root: bytes[4..36].try_into().expect("fixed header merkle root"),
            timestamp: BlockTimestamp::from_le_bytes(
                bytes[36..40].try_into().expect("fixed header timestamp"),
            ),
            nonce: u32::from_le_bytes(bytes[40..44].try_into().expect("fixed header nonce")),
        })
        .collect()
}

fn decode_median_hints(bytes: &[u8]) -> Vec<BlockTimestamp> {
    bytes
        .chunks_exact(size_of::<BlockTimestamp>())
        .map(|bytes| BlockTimestamp::from_le_bytes(bytes.try_into().expect("fixed median hint")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::State;

    #[test]
    fn binary_batch_frame_round_trips() {
        let batch = FetchedBatch {
            current_state: State::default(),
            headers: vec![NewHeader {
                version: 2,
                merkle_root: [3; 32],
                timestamp: BlockTimestamp::new(4),
                nonce: 5,
            }],
            median_hints: vec![BlockTimestamp::new(6)],
            expected_state: State::default(),
            expected_continuation_digest: [7; 32],
            first_new_height: 7,
            end_height: 7,
        };

        let bytes = encode_batch_response(&batch).unwrap();
        assert_eq!(
            decode_batch_response(&bytes).unwrap().headers,
            batch.headers
        );
        assert_eq!(
            decode_batch_response(&bytes).unwrap().median_hints,
            batch.median_hints
        );
    }

    #[test]
    fn binary_batch_frame_rejects_trailing_data() {
        let batch = FetchedBatch {
            current_state: State::default(),
            headers: vec![NewHeader {
                version: 0,
                merkle_root: [0; 32],
                timestamp: BlockTimestamp::new(0),
                nonce: 0,
            }],
            median_hints: Vec::new(),
            expected_state: State::default(),
            expected_continuation_digest: [0; 32],
            first_new_height: 1,
            end_height: 1,
        };
        let mut bytes = encode_batch_response(&batch).unwrap();
        bytes.push(0);

        assert!(decode_batch_response(&bytes)
            .unwrap_err()
            .contains("trailing bytes"));
    }

    #[tokio::test]
    async fn panicking_request_does_not_poison_lock_for_later_requests() {
        use crate::{db_path, sha256d, BlockHash, HeaderStore};

        let db = HeaderStore::open(db_path()).expect("failed to open test database");
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        let controller = Arc::new(Mutex::new(crate::Controller::new(db, genesis_hash)));

        let panicked = tokio::spawn(begin_session_handler(
            AxumState(controller.clone()),
            Bytes::from(encode_session_start(&SessionStart::TrustedHeight(u32::MAX))),
        ))
        .await
        .is_err();
        assert!(panicked, "expected an out-of-range height to panic");

        assert!(
            begin_session_handler(AxumState(controller), Bytes::from(vec![0]))
                .await
                .is_ok(),
            "a later request should recover the poisoned lock"
        );
    }
}
