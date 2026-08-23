//! Data returned by the controller to a proving host.
//!
//! Both controller operations accept only a run id. HTTP encodes the batch
//! response with postcard; local mode passes the value directly.

use rkyv::{Archive, Deserialize, Serialize};
use zkpow_core::{BlockTimestamp, HeaderInput, State, VerifierKeyDigest};

pub(crate) const MEDIAN_HINT_SIZE: usize = size_of::<BlockTimestamp>();

/// An opaque artifact location selected by the controller.
#[derive(Archive, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileLocation {
    Path(String),
    Url(String),
}

/// Everything required to prove the next controller-selected batch.
#[derive(Archive, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// #[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub first_new_height:  u32,
    pub end_height:        u32,
    pub verifier_key:      VerifierKeyDigest,
    pub current_state:     State,
    pub header_bytes:      Vec<u8>,
    pub median_hint_bytes: Vec<u8>,
    /// Recursive witness for a fresh worker. A worker that just produced this
    /// proof already has it in memory and need not fetch it.
    pub previous_proof: Option<FileLocation>,
    pub proof_upload: FileLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerError {
    pub kind:    ErrorKind,
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
        Self::new(ErrorKind::BadRequest, message)
    }

    pub fn exhausted(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Exhausted, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
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
        if (header_count as usize) * HeaderInput::SIZE != self.header_bytes.len() {
            return Err(format!(
                "batch heights describe {header_count} headers, got {} payload bytes",
                self.header_bytes.len()
            ));
        }
        if !self.median_hint_bytes.len().is_multiple_of(MEDIAN_HINT_SIZE) {
            return Err(format!(
                "median-hint payload of {} bytes is not a whole number of hints",
                self.median_hint_bytes.len()
            ));
        }
        Ok(())
    }
}
