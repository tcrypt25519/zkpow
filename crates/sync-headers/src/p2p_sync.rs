//! P2P-network header sync orchestration.
//!
//! This is the **producer**: it handles peer discovery, connection, and the
//! `getheaders`/`headers` request-response cycle. The caller provides a
//! callback (`on_headers`) that receives each batch of raw headers and returns
//! the updated chain tip (or an error), keeping persistence and validation in
//! the caller so this crate stays free of any storage dependency.

use std::{net::SocketAddr, thread, time::Duration};

use zkpow_core::{BlockHash, MAINNET_GENESIS_HASH};

use crate::{
    connection::PeerConnection,
    error::SyncHeadersError,
    peers::PeerStore,
    types::{Header, Network, Tip},
};

/// How many times to retry a connection attempt to the same peer.
const CONNECT_RETRIES: u32 = 3;

/// Sync headers from the Bitcoin P2P network, passing each batch to a callback.
///
/// Re-exported as [`crate::p2p::sync`].
///
/// The sync loop:
/// 1. Picks a random candidate peer from `peer_store`
/// 2. Connects and performs the version handshake
/// 3. Sends `getheaders` starting from `anchor`
/// 4. Passes raw headers to `on_headers(headers, current_tip)`
/// 5. Uses the returned tip for the next `getheaders` request
/// 6. Repeats until the peer sends an empty batch (tip reached) or an error occurs
pub fn sync<C, E>(
    network: Network,
    anchor: &Tip,
    max_headers: u64,
    peer_store: &mut PeerStore,
    mut on_headers: C,
) -> Result<Tip, SyncHeadersError<E>>
where
    C: FnMut(&[Header], &Tip) -> Result<Tip, E>,
    E: std::fmt::Debug,
{
    let mut current_tip = anchor.clone();
    let mut total_synced: u64 = 0;
    let mut batch: u64 = 0;
    let mut rng = rand::rng();

    loop {
        if max_headers > 0 && total_synced >= max_headers {
            tracing::info!(total_synced, "Reached max headers limit");
            break;
        }

        let peer_addr = match peer_store.pick_candidate(&mut rng) {
            Some(addr) => addr,
            None => {
                return Err(SyncHeadersError::NoCandidates {
                    latest_tip: Box::new(current_tip),
                });
            },
        };

        tracing::info!(peer = %peer_addr, tip_height = current_tip.height, "Starting sync run with peer");

        match sync_with_peer(
            peer_addr,
            network,
            &mut current_tip,
            max_headers,
            &mut total_synced,
            &mut batch,
            &mut on_headers,
        ) {
            Ok(()) => {
                tracing::info!(height = current_tip.height, "Sync completed");
                break;
            },
            Err(e) => {
                current_tip = e.latest_tip().clone();
                match &e {
                    SyncHeadersError::Connect { peer, source, .. } => {
                        tracing::warn!(peer = %peer, error = %source, "Connection failed, trying different peer");
                    },
                    SyncHeadersError::Peer { source, synced, .. } => {
                        tracing::warn!(error = %source, synced, "Peer error during sync, trying different peer");
                    },
                    SyncHeadersError::Callback { source: _, synced: _, .. } => {
                        return Err(e);
                    },
                    SyncHeadersError::NoCandidates { .. } => {
                        return Err(e);
                    },
                }
                peer_store.mark_unreachable(&peer_addr);
            },
        }
    }

    Ok(current_tip)
}

fn sync_with_peer<C, E>(
    peer_addr: SocketAddr,
    network: Network,
    current_tip: &mut Tip,
    max_headers: u64,
    total_synced: &mut u64,
    batch: &mut u64,
    on_headers: &mut C,
) -> Result<(), SyncHeadersError<E>>
where
    C: FnMut(&[Header], &Tip) -> Result<Tip, E>,
    E: std::fmt::Debug,
{
    let mut p2p_conn = None;
    for attempt in 0..CONNECT_RETRIES {
        match PeerConnection::connect(peer_addr, network, current_tip.height as i32) {
            Ok(c) => {
                p2p_conn = Some(c);
                break;
            },
            Err(e) => {
                if attempt + 1 < CONNECT_RETRIES {
                    let backoff = Duration::from_secs(2u64.pow(attempt + 1));
                    tracing::warn!(
                        peer = %peer_addr, error = %e, attempt = attempt + 1, backoff = ?backoff,
                        "Connection failed, retrying"
                    );
                    thread::sleep(backoff);
                } else {
                    tracing::warn!(peer = %peer_addr, error = %e, "All connection retries exhausted");
                }
            },
        }
    }

    let mut p2p_conn = match p2p_conn {
        Some(c) => c,
        None => {
            return Err(SyncHeadersError::Connect {
                peer:       peer_addr,
                source:     Box::new(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "all connection retries exhausted",
                )),
                latest_tip: Box::new(current_tip.clone()),
            });
        },
    };

    loop {
        if max_headers > 0 && *total_synced >= max_headers {
            break;
        }

        let locators = build_locators(current_tip);
        let stop_hash = BlockHash::new([0u8; 32]);

        if let Err(e) = p2p_conn.send_getheaders(&locators, stop_hash) {
            return Err(SyncHeadersError::Peer {
                source:     Box::new(e),
                latest_tip: Box::new(current_tip.clone()),
                synced:     *total_synced,
            });
        }

        let raw_headers = match p2p_conn.read_headers() {
            Ok(h) => h,
            Err(e) => {
                return Err(SyncHeadersError::Peer {
                    source:     Box::new(e),
                    latest_tip: Box::new(current_tip.clone()),
                    synced:     *total_synced,
                });
            },
        };

        if raw_headers.is_empty() {
            tracing::info!(height = current_tip.height, "Peer has no more headers, we're at the tip");
            break;
        }

        *batch += 1;
        tracing::info!(batch, received = raw_headers.len(), "Received headers from peer");

        let new_tip = match on_headers(&raw_headers, current_tip) {
            Ok(tip) => tip,
            Err(e) => {
                return Err(SyncHeadersError::Callback {
                    source:     Box::new(e),
                    latest_tip: Box::new(current_tip.clone()),
                    synced:     *total_synced,
                });
            },
        };

        let valid_count = new_tip.height - current_tip.height;
        *total_synced += valid_count;

        tracing::info!(
            batch,
            height = new_tip.height,
            hash = %new_tip.hash,
            batch_total = *total_synced,
            "Progress update"
        );

        *current_tip = new_tip;

        if valid_count < 2000 {
            tracing::info!("Received partial batch, peer is caught up");
            break;
        }
    }

    Ok(())
}

fn build_locators(tip: &Tip) -> Vec<BlockHash> {
    vec![tip.hash, MAINNET_GENESIS_HASH]
}
