//! Bitcoin block-header synchronization.
//!
//! Two interchangeable backends, each exposing a single `sync` entry point:
//!
//! - [`p2p::sync`] — streams headers from the Bitcoin P2P network via DNS-seed
//!   peer discovery and the `getheaders`/`headers` cycle, handing each batch to
//!   a caller-provided callback (the caller owns validation and persistence).
//! - [`alchemy::sync`] — fetches detailed headers from the Alchemy Bitcoin RPC
//!   and stores them in a `headers_alchemy` table.

mod connection;
mod error;
mod peers;
mod types;
mod validation;

mod alchemy_rpc;
mod p2p_sync;

/// P2P-network header sync backend.
pub mod p2p {
    pub use crate::p2p_sync::sync;
}

/// Alchemy RPC header sync backend.
pub mod alchemy {
    pub use crate::alchemy_rpc::sync;
}

pub use connection::PeerConnection;
pub use error::SyncHeadersError;
pub use peers::{PeerEntry, PeerStatus, PeerStore};
pub use types::{current_difficulty, ConnectedHeader, Header, Network, Tip};
pub use validation::{connect_headers, tip_from_connected};
