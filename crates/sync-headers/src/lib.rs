//! Bitcoin block-header synchronization.
//!
//! [`p2p::sync`] streams headers from the Bitcoin P2P network via DNS-seed
//! peer discovery and the `getheaders`/`headers` cycle, handing each batch to
//! a caller-provided callback (the caller owns validation and persistence).

#[cfg(test)]
mod tests;

mod connection;
mod error;
mod peers;
mod types;
mod validation;

mod p2p_sync;

/// P2P-network header sync backend.
pub mod p2p {
    pub use crate::p2p_sync::sync;
}

pub use connection::PeerConnection;
pub use error::SyncHeadersError;
pub use peers::{PeerEntry, PeerStatus, PeerStore};
pub use types::{expand_target, ConnectedHeader, Header, Network, Tip};
pub use validation::{connect_headers, tip_from_connected};
