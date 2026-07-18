//! Binary wire protocol between the host and the controller.
//!
//! The same encoded frames travel over both transports: the HTTP server
//! decodes request bodies with these functions, and the host's in-process
//! "local" controller round-trips through the identical encode/decode pair,
//! so the two modes cannot drift apart. Every field is fixed-width
//! little-endian; there is deliberately no JSON, hex, or serde layer.
//!
//! Header and median-hint payloads stay as raw witness bytes end to end:
//! the database serializes them once, and the host feeds the same bytes
//! straight into the guest's stdin without decoding — the host has no reason
//! to look inside.
//!
//! ```text
//! pull-batch request:
//!   run_id_len: u8 | run_id: utf-8 bytes
//!   verifier_key: 32 bytes
//!   num_headers: u32 LE
//!   held_tip: optional u32 (presence byte + u32 LE)
//!
//! batch response:
//!   first_new_height: u32 LE | end_height: u32 LE | median_hint_count: u32 LE
//!   header_bytes: (end_height - first_new_height + 1) * NEW_HEADER_SIZE bytes
//!   median_hint_bytes: median_hint_count * 4 bytes
//!   current_state: STATE_SIZE bytes
//!   prev_proof_fetch: optional target
//!   proof_put: optional target
//!
//! complete request:
//!   run_id_len: u8 | run_id: utf-8 bytes
//!   end_height: u32 LE
//!   has_proof: u8
//!   public_values_len: u32 LE | public_values bytes
//!
//! complete ack response:
//!   verified_height: u32 LE
//!
//! target:
//!   kind: u8 (0 = filesystem path, 1 = URL) | len: u16 LE | utf-8 bytes
//! ```
//!
//! Error bodies are plain UTF-8 text; [`ControllerError`] carries a kind so
//! the HTTP layer can pick a status code, but hosts only rely on the message
//! (including the stable `NO_HEADERS_REMAINING_PREFIX` sentinel).

use zkpow_core::{BlockTimestamp, State, VerifierKeyDigest, NEW_HEADER_SIZE, STATE_SIZE};

const BATCH_PREFIX_SIZE: usize = 12;
const MEDIAN_HINT_SIZE: usize = size_of::<BlockTimestamp>();

/// Where the host should read or write an artifact. `Path` targets are only
/// meaningful when host and controller share a filesystem (local mode or a
/// single-machine deployment); `Url` targets are pre-signed object-storage
/// URLs usable from anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Path(String),
    Url(String),
}

/// Host request for the next batch of a run. The first pull for an unknown
/// `run_id` creates the run (pinning `verifier_key`); later pulls must carry
/// the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullBatchRequest {
    pub run_id: String,
    /// Digest of the guest verifying key the host will feed the guest. The
    /// controller pins this per run and rejects mismatched pulls, so a
    /// mixed-guest IVC chain is refused before any proving time is spent.
    pub verifier_key: VerifierKeyDigest,
    pub num_headers: u32,
    /// Height of the compressed proof the host currently holds in memory
    /// (`None` when it holds nothing, e.g. right after boot). Lets the
    /// controller decide whether to include `prev_proof_fetch`.
    pub held_tip: Option<u32>,
}

/// The next batch for a run: everything the host needs to feed the guest,
/// plus where to upload the resulting proof. Validation data (the expected
/// end state and continuation digest) deliberately never leaves the
/// controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub first_new_height: u32,
    pub end_height: u32,
    pub current_state: State,
    /// Serialized `NewHeader`s, exactly as the guest reads them from stdin.
    pub header_bytes: Vec<u8>,
    /// Serialized median-time-past hints, exactly as the guest reads them.
    pub median_hint_bytes: Vec<u8>,
    /// Present when the host reported a `held_tip` behind the run's verified
    /// tip: fetch this to recover the proof witness needed to continue.
    pub prev_proof_fetch: Option<Target>,
    pub proof_put: Option<Target>,
}

/// Host notification that a batch's proof is uploaded (or, for execute-only
/// runs, that execution finished) with the guest-committed public values for
/// the controller to validate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteRequest {
    pub run_id: String,
    pub end_height: u32,
    /// False for execute-only runs; the controller only accepts unproven
    /// completions when explicitly configured to.
    pub has_proof: bool,
    pub public_values: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompleteAck {
    pub verified_height: u32,
}

/// Protocol-level failure. `kind` picks the HTTP status; hosts see only the
/// message text on either transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Malformed or contradictory request (also proof/public-value rejection).
    BadRequest,
    /// No more headers in the database.
    Exhausted,
    /// The run cannot make progress (halted, or another host owns its tip).
    Conflict,
    /// Long-poll gave up waiting for the previous batch to verify.
    Timeout,
    Internal,
}

impl ControllerError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::BadRequest, message: message.into() }
    }

    pub fn exhausted(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Exhausted, message: message.into() }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Conflict, message: message.into() }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Timeout, message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Internal, message: message.into() }
    }
}

impl std::fmt::Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ControllerError {}

pub fn encode_pull_batch_request(request: &PullBatchRequest) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(1 + request.run_id.len() + 32 + 4 + 5);
    push_run_id(&mut bytes, &request.run_id)?;
    bytes.extend_from_slice(&request.verifier_key.to_bytes());
    bytes.extend_from_slice(&request.num_headers.to_le_bytes());
    push_optional_u32(&mut bytes, request.held_tip);
    Ok(bytes)
}

pub fn decode_pull_batch_request(bytes: &[u8]) -> Result<PullBatchRequest, String> {
    let mut cursor = 0;
    let run_id = read_run_id(bytes, &mut cursor)?;
    let verifier_key = VerifierKeyDigest::from_bytes(
        take(bytes, &mut cursor, 32, "verifier key")?
            .try_into()
            .expect("fixed 32-byte verifier key slice"),
    );
    let num_headers = read_u32(bytes, &mut cursor, "num headers")?;
    let held_tip = read_optional_u32(bytes, &mut cursor, "held tip")?;
    expect_end(bytes, cursor, "pull-batch request")?;
    Ok(PullBatchRequest { run_id, verifier_key, num_headers, held_tip })
}

pub fn encode_batch(batch: &Batch) -> Result<Vec<u8>, String> {
    let header_count = batch
        .end_height
        .checked_sub(batch.first_new_height)
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| "batch end height precedes first new height".to_string())?;
    if header_count as usize * NEW_HEADER_SIZE != batch.header_bytes.len() {
        return Err(format!(
            "batch heights describe {header_count} headers, got {} payload bytes",
            batch.header_bytes.len()
        ));
    }
    if batch.median_hint_bytes.len() % MEDIAN_HINT_SIZE != 0 {
        return Err(format!(
            "median-hint payload of {} bytes is not a whole number of hints",
            batch.median_hint_bytes.len()
        ));
    }

    let mut bytes = Vec::with_capacity(
        BATCH_PREFIX_SIZE
            + batch.header_bytes.len()
            + batch.median_hint_bytes.len()
            + STATE_SIZE
            + 2 * 64,
    );
    bytes.extend_from_slice(&batch.first_new_height.to_le_bytes());
    bytes.extend_from_slice(&batch.end_height.to_le_bytes());
    bytes.extend_from_slice(
        &((batch.median_hint_bytes.len() / MEDIAN_HINT_SIZE) as u32).to_le_bytes(),
    );
    bytes.extend_from_slice(&batch.header_bytes);
    bytes.extend_from_slice(&batch.median_hint_bytes);
    bytes.extend_from_slice(&batch.current_state.to_bytes());
    push_optional_target(&mut bytes, batch.prev_proof_fetch.as_ref())?;
    push_optional_target(&mut bytes, batch.proof_put.as_ref())?;
    Ok(bytes)
}

pub fn decode_batch(bytes: &[u8]) -> Result<Batch, String> {
    let mut cursor = 0;
    let first_new_height = read_u32(bytes, &mut cursor, "first new height")?;
    let end_height = read_u32(bytes, &mut cursor, "end height")?;
    let median_hint_count = read_u32(bytes, &mut cursor, "median hint count")? as usize;
    let header_count = end_height
        .checked_sub(first_new_height)
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| "batch end height precedes first new height".to_string())?
        as usize;
    let header_bytes_len = header_count
        .checked_mul(NEW_HEADER_SIZE)
        .ok_or_else(|| "batch header payload length overflows usize".to_string())?;
    let hint_bytes_len = median_hint_count
        .checked_mul(MEDIAN_HINT_SIZE)
        .ok_or_else(|| "batch median-hint payload length overflows usize".to_string())?;

    let header_bytes = take(bytes, &mut cursor, header_bytes_len, "headers")?.to_vec();
    let median_hint_bytes = take(bytes, &mut cursor, hint_bytes_len, "median hints")?.to_vec();
    let current_state = State::parse(take(bytes, &mut cursor, STATE_SIZE, "current state")?)
        .map_err(|err| format!("invalid current state in batch: {err}"))?;
    let prev_proof_fetch = read_optional_target(bytes, &mut cursor, "prev proof fetch target")?;
    let proof_put = read_optional_target(bytes, &mut cursor, "proof put target")?;
    expect_end(bytes, cursor, "batch")?;

    Ok(Batch {
        first_new_height,
        end_height,
        current_state,
        header_bytes,
        median_hint_bytes,
        prev_proof_fetch,
        proof_put,
    })
}

pub fn encode_complete_request(request: &CompleteRequest) -> Result<Vec<u8>, String> {
    let mut bytes =
        Vec::with_capacity(1 + request.run_id.len() + 4 + 1 + 4 + request.public_values.len());
    push_run_id(&mut bytes, &request.run_id)?;
    bytes.extend_from_slice(&request.end_height.to_le_bytes());
    bytes.push(u8::from(request.has_proof));
    let pv_len = u32::try_from(request.public_values.len())
        .map_err(|_| "public values length overflows u32".to_string())?;
    bytes.extend_from_slice(&pv_len.to_le_bytes());
    bytes.extend_from_slice(&request.public_values);
    Ok(bytes)
}

pub fn decode_complete_request(bytes: &[u8]) -> Result<CompleteRequest, String> {
    let mut cursor = 0;
    let run_id = read_run_id(bytes, &mut cursor)?;
    let end_height = read_u32(bytes, &mut cursor, "end height")?;
    let has_proof = match take(bytes, &mut cursor, 1, "has-proof flag")?[0] {
        0 => false,
        1 => true,
        other => return Err(format!("invalid has-proof flag {other}")),
    };
    let pv_len = read_u32(bytes, &mut cursor, "public values length")? as usize;
    let public_values = take(bytes, &mut cursor, pv_len, "public values")?.to_vec();
    expect_end(bytes, cursor, "complete request")?;
    Ok(CompleteRequest { run_id, end_height, has_proof, public_values })
}

pub fn encode_complete_ack(ack: &CompleteAck) -> Vec<u8> {
    ack.verified_height.to_le_bytes().to_vec()
}

pub fn decode_complete_ack(bytes: &[u8]) -> Result<CompleteAck, String> {
    let mut cursor = 0;
    let verified_height = read_u32(bytes, &mut cursor, "verified height")?;
    expect_end(bytes, cursor, "complete ack")?;
    Ok(CompleteAck { verified_height })
}

fn push_run_id(bytes: &mut Vec<u8>, run_id: &str) -> Result<(), String> {
    if run_id.is_empty() {
        return Err("run id must not be empty".to_string());
    }
    let len = u8::try_from(run_id.len())
        .map_err(|_| format!("run id longer than 255 bytes: {run_id:?}"))?;
    bytes.push(len);
    bytes.extend_from_slice(run_id.as_bytes());
    Ok(())
}

fn read_run_id(bytes: &[u8], cursor: &mut usize) -> Result<String, String> {
    let len = take(bytes, cursor, 1, "run id length")?[0] as usize;
    if len == 0 {
        return Err("run id must not be empty".to_string());
    }
    let raw = take(bytes, cursor, len, "run id")?;
    std::str::from_utf8(raw).map(str::to_owned).map_err(|_| "run id is not utf-8".to_string())
}

fn push_optional_u32(bytes: &mut Vec<u8>, value: Option<u32>) {
    match value {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        None => bytes.push(0),
    }
}

fn read_optional_u32(
    bytes: &[u8],
    cursor: &mut usize,
    field: &str,
) -> Result<Option<u32>, String> {
    match take(bytes, cursor, 1, field)?[0] {
        0 => Ok(None),
        1 => Ok(Some(read_u32(bytes, cursor, field)?)),
        other => Err(format!("invalid presence byte {other} for {field}")),
    }
}

fn push_target(bytes: &mut Vec<u8>, target: &Target) -> Result<(), String> {
    let (kind, value) = match target {
        Target::Path(path) => (0u8, path),
        Target::Url(url) => (1u8, url),
    };
    let len = u16::try_from(value.len())
        .map_err(|_| format!("target longer than {} bytes", u16::MAX))?;
    bytes.push(kind);
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn push_optional_target(bytes: &mut Vec<u8>, target: Option<&Target>) -> Result<(), String> {
    match target {
        Some(target) => {
            bytes.push(1);
            push_target(bytes, target)
        }
        None => {
            bytes.push(0);
            Ok(())
        }
    }
}

fn read_target(bytes: &[u8], cursor: &mut usize, field: &str) -> Result<Target, String> {
    let kind = take(bytes, cursor, 1, field)?[0];
    let len = u16::from_le_bytes(
        take(bytes, cursor, 2, field)?.try_into().expect("fixed 2-byte target length"),
    ) as usize;
    let raw = take(bytes, cursor, len, field)?;
    let value = std::str::from_utf8(raw)
        .map_err(|_| format!("{field} is not utf-8"))?
        .to_owned();
    match kind {
        0 => Ok(Target::Path(value)),
        1 => Ok(Target::Url(value)),
        other => Err(format!("invalid target kind {other} for {field}")),
    }
}

fn read_optional_target(
    bytes: &[u8],
    cursor: &mut usize,
    field: &str,
) -> Result<Option<Target>, String> {
    match take(bytes, cursor, 1, field)?[0] {
        0 => Ok(None),
        1 => Ok(Some(read_target(bytes, cursor, field)?)),
        other => Err(format!("invalid presence byte {other} for {field}")),
    }
}

fn read_u32(bytes: &[u8], cursor: &mut usize, field: &str) -> Result<u32, String> {
    let raw: [u8; 4] =
        take(bytes, cursor, 4, field)?.try_into().expect("fixed 4-byte field slice");
    Ok(u32::from_le_bytes(raw))
}

fn take<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
    field: &str,
) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| format!("frame {field} length overflows usize"))?;
    let field_bytes =
        bytes.get(*cursor..end).ok_or_else(|| format!("frame truncated in {field}"))?;
    *cursor = end;
    Ok(field_bytes)
}

fn expect_end(bytes: &[u8], cursor: usize, frame: &str) -> Result<(), String> {
    if cursor != bytes.len() {
        return Err(format!("{frame} has {} trailing bytes", bytes.len() - cursor));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_batch() -> Batch {
        Batch {
            first_new_height: 7,
            end_height: 8,
            current_state: State::default(),
            header_bytes: vec![0xAB; 2 * NEW_HEADER_SIZE],
            median_hint_bytes: vec![0xCD; MEDIAN_HINT_SIZE],
            prev_proof_fetch: Some(Target::Url("https://example.test/get".to_string())),
            proof_put: Some(Target::Path("/tmp/run/proof.bin".to_string())),
        }
    }

    #[test]
    fn pull_batch_request_round_trips() {
        let request = PullBatchRequest {
            run_id: "mainnet-01".to_string(),
            verifier_key: VerifierKeyDigest::from_raw([0x11223344; 8]),
            num_headers: 2016,
            held_tip: Some(4032),
        };
        let bytes = encode_pull_batch_request(&request).unwrap();
        assert_eq!(decode_pull_batch_request(&bytes).unwrap(), request);
    }

    #[test]
    fn pull_batch_request_round_trips_without_held_tip() {
        let request = PullBatchRequest {
            run_id: "r".to_string(),
            verifier_key: VerifierKeyDigest::default(),
            num_headers: 1,
            held_tip: None,
        };
        let bytes = encode_pull_batch_request(&request).unwrap();
        assert_eq!(decode_pull_batch_request(&bytes).unwrap(), request);
    }

    #[test]
    fn batch_round_trips() {
        let batch = sample_batch();
        let bytes = encode_batch(&batch).unwrap();
        assert_eq!(decode_batch(&bytes).unwrap(), batch);
    }

    #[test]
    fn batch_rejects_trailing_bytes() {
        let mut bytes = encode_batch(&sample_batch()).unwrap();
        bytes.push(0);
        assert!(decode_batch(&bytes).unwrap_err().contains("trailing bytes"));
    }

    #[test]
    fn batch_rejects_header_payload_mismatch() {
        let mut batch = sample_batch();
        batch.header_bytes.pop();
        assert!(encode_batch(&batch).is_err());
    }

    #[test]
    fn complete_request_round_trips() {
        let request = CompleteRequest {
            run_id: "mainnet-01".to_string(),
            end_height: 2016,
            has_proof: true,
            public_values: vec![0xAB; zkpow_core::MINIMAL_PV_SIZE],
        };
        let bytes = encode_complete_request(&request).unwrap();
        assert_eq!(decode_complete_request(&bytes).unwrap(), request);
    }

    #[test]
    fn complete_ack_round_trips() {
        let ack = CompleteAck { verified_height: 123 };
        assert_eq!(decode_complete_ack(&encode_complete_ack(&ack)).unwrap(), ack);
    }

    #[test]
    fn empty_run_id_is_rejected() {
        let request = PullBatchRequest {
            run_id: String::new(),
            verifier_key: VerifierKeyDigest::default(),
            num_headers: 1,
            held_tip: None,
        };
        assert!(encode_pull_batch_request(&request).is_err());
    }
}
