//! Error types for the header sync protocol.

use std::net::SocketAddr;

use thiserror::Error;

use crate::{connection::Command, types::Tip};

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
}

impl From<bitcoin::io::Error> for ConnectionError {
    fn from(source: bitcoin::io::Error) -> Self {
        Self::Codec(source.into())
    }
}

/// The outcome of a sync-headers run: either we reached the tip or we hit
/// an error partway through.
///
/// In both cases the caller gets back the last known good tip so that
/// another attempt can resume from there.
///
/// The `E` type parameter is the error type returned by the caller's header
/// processing callback.
#[derive(Error, Debug)]
pub enum SyncHeadersError<E: std::fmt::Debug> {
    /// Failed to connect to the peer at all.
    #[error("failed to connect to peer {peer}: {source}")]
    Connect {
        peer:       SocketAddr,
        source:     ConnectionError,
        latest_tip: Box<Tip>,
    },

    /// Lost connection or protocol error during the sync.
    #[error("peer error during sync (after {synced} headers): {source}")]
    Peer {
        source:     ConnectionError,
        latest_tip: Box<Tip>,
        synced:     u32,
    },

    /// The caller's header-processing callback returned an error.
    #[error("header processing error (after {synced} headers): {source:?}")]
    Callback {
        source:     Box<E>,
        latest_tip: Box<Tip>,
        synced:     u32,
    },

    /// No candidate peers available to try.
    #[error("no candidate peers available")]
    NoCandidates { latest_tip: Box<Tip> },
}

impl<E: std::fmt::Debug> SyncHeadersError<E> {
    pub fn latest_tip(&self) -> &Tip {
        match self {
            Self::Connect { latest_tip, .. }
            | Self::Peer { latest_tip, .. }
            | Self::Callback { latest_tip, .. }
            | Self::NoCandidates { latest_tip } => latest_tip,
        }
    }

    /// Whether the caller should try another peer and retry.
    ///
    /// `NoCandidates` and `Callback` errors mean there are no more peers to
    /// try (or the problem isn't the peer), so the caller should surface the
    /// error. Connection and peer errors are worth a retry with a different
    /// peer.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::Connect { .. } | Self::Peer { .. })
    }
}
