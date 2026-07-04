//! Error types for the header sync protocol.

use std::net::SocketAddr;
use thiserror::Error;

use crate::types::Tip;

/// The outcome of a sync-headers run — either we reached the tip or we hit
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
        peer: SocketAddr,
        source: Box<std::io::Error>,
        latest_tip: Box<Tip>,
    },

    /// Lost connection or protocol error during the sync.
    #[error("peer error during sync (after {synced} headers): {source}")]
    Peer {
        source: Box<std::io::Error>,
        latest_tip: Box<Tip>,
        synced: u64,
    },

    /// The caller's header-processing callback returned an error.
    #[error("header processing error (after {synced} headers): {source:?}")]
    Callback {
        source: Box<E>,
        latest_tip: Box<Tip>,
        synced: u64,
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
