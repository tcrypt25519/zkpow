//! Bitcoin P2P headers, peer selection, and batch validation.

#[cfg(test)]
mod tests;

mod connection;
mod error;
mod msg;
mod peers;
mod types;
mod validation;

pub use connection::{Command, PeerConnection};
pub use error::ConnectionError;
pub use peers::{PeerEntry, PeerStatus, PeerStore};
pub use types::{expand_target, ConnectedHeader, Header, Network, Tip};
pub use validation::connect_headers;
