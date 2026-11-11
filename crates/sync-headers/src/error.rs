//! Error types for the header sync protocol.

use thiserror::Error;

use crate::connection::Command;

/// Transport, wire-format, and protocol failures reported by a peer connection.
#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("connection I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("Bitcoin message encoding or decoding failed: {0}")]
    Codec(#[from] bitcoin::consensus::encode::Error),

    #[error("system clock is before the Unix epoch: {0}")]
    Clock(#[from] std::time::SystemTimeError),

    #[error("invalid network magic {received:02x?}, expected {expected:02x?}")]
    InvalidMagic { received: [u8; 4], expected: [u8; 4] },

    #[error("message payload length {length} exceeds {limit}")]
    MessageTooLarge { length: usize, limit: usize },

    #[error("headers count {count} exceeds {limit}")]
    TooManyHeaders { count: u64, limit: usize },

    #[error("user agent length {length} exceeds {limit}")]
    UserAgentTooLong { length: u64, limit: u64 },

    #[error("version message has {remaining} trailing bytes")]
    TrailingVersionData { remaining: usize },

    #[error("checksum mismatch for {command:?}: received {received:02x?}, expected {expected:02x?}")]
    ChecksumMismatch {
        command:  Command,
        received: [u8; 4],
        expected: [u8; 4],
    },

    #[error("too many unsolicited messages while waiting for {expected:?} (limit {limit})")]
    TooManyUnsolicitedMessages { expected: Command, limit: usize },

    #[error("peer sent invalid headers: accepted {valid} of {total}")]
    InvalidHeaders { valid: usize, total: usize },

    #[error("unknown command")]
    UnknownCommand,

    #[error("utf8 error in command")]
    InvalidCommandUtf8(#[from] std::str::Utf8Error),
}

impl From<bitcoin::io::Error> for ConnectionError {
    fn from(source: bitcoin::io::Error) -> Self {
        Self::Codec(source.into())
    }
}
