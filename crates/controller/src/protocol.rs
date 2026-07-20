//! Data returned by the controller to a proving host.
//!
//! Both controller operations accept only a run id. HTTP encodes the batch
//! response with postcard; local mode passes the value directly.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zkpow_core::{
    BlockTimestamp, State, VerifierKeyDigest, CONTINUATION_DATA_SIZE, NEW_HEADER_SIZE,
};

const MEDIAN_HINT_SIZE: usize = size_of::<BlockTimestamp>();

/// An opaque artifact location selected by the controller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileLocation {
    Path(String),
    Url(String),
}

/// Everything required to prove the next controller-selected batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub first_new_height: u32,
    pub end_height: u32,
    pub verifier_key: VerifierKeyDigest,
    pub current_state: State,
    pub header_bytes: Vec<u8>,
    pub median_hint_bytes: Vec<u8>,
    /// Recursive witness for a fresh worker. A worker that just produced this
    /// proof already has it in memory and need not fetch it.
    pub previous_proof: Option<FileLocation>,
    pub proof_upload: FileLocation,
    pub continuation_bytes: Vec<u8>,
    pub continuation_upload: FileLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    Exhausted,
    Conflict,
    Timeout,
    Internal,
}

impl ControllerError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::BadRequest,
            message: message.into(),
        }
    }

    pub fn exhausted(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Exhausted,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Conflict,
            message: message.into(),
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Timeout,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ControllerError {}

impl Batch {
    pub fn validate(&self) -> Result<(), String> {
        let header_count = self
            .end_height
            .checked_sub(self.first_new_height)
            .and_then(|distance| distance.checked_add(1))
            .ok_or_else(|| "batch end height precedes first new height".to_string())?;
        if header_count as usize * NEW_HEADER_SIZE != self.header_bytes.len() {
            return Err(format!(
                "batch heights describe {header_count} headers, got {} payload bytes",
                self.header_bytes.len()
            ));
        }
        if !self
            .median_hint_bytes
            .len()
            .is_multiple_of(MEDIAN_HINT_SIZE)
        {
            return Err(format!(
                "median-hint payload of {} bytes is not a whole number of hints",
                self.median_hint_bytes.len()
            ));
        }
        if self.continuation_bytes.len() != CONTINUATION_DATA_SIZE {
            return Err(format!(
                "continuation data has {} bytes, expected {CONTINUATION_DATA_SIZE}",
                self.continuation_bytes.len()
            ));
        }
        Ok(())
    }
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    postcard::to_allocvec(value).map_err(|err| err.to_string())
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let (value, remaining) = postcard::take_from_bytes(bytes).map_err(|err| err.to_string())?;
    if !remaining.is_empty() {
        return Err(format!("message has {} trailing bytes", remaining.len()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_batch() -> Batch {
        Batch {
            first_new_height: 7,
            end_height: 8,
            verifier_key: VerifierKeyDigest::from_raw([0x11223344; 8]),
            current_state: State::default(),
            header_bytes: vec![0xAB; 2 * NEW_HEADER_SIZE],
            median_hint_bytes: vec![0xCD; MEDIAN_HINT_SIZE],
            previous_proof: Some(FileLocation::Url("https://example.test/get".to_string())),
            proof_upload: FileLocation::Path("/tmp/run/proof.bin".to_string()),
            continuation_bytes: vec![0xEF; CONTINUATION_DATA_SIZE],
            continuation_upload: FileLocation::Path("/tmp/run/continuation.bin".to_string()),
        }
    }

    #[test]
    fn batch_round_trips() {
        let batch = sample_batch();
        assert_eq!(decode::<Batch>(&encode(&batch).unwrap()).unwrap(), batch);
    }

    #[test]
    fn batch_rejects_trailing_bytes() {
        let mut bytes = encode(&sample_batch()).unwrap();
        bytes.push(0);
        assert!(decode::<Batch>(&bytes).is_err());
    }

    #[test]
    fn batch_rejects_header_payload_mismatch() {
        let mut batch = sample_batch();
        batch.header_bytes.pop();
        assert!(batch.validate().is_err());
    }
}
