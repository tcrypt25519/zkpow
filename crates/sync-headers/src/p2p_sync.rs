//! P2P-network header sync orchestration.
//!
//! This is the **producer**: it handles peer discovery, connection, and the
//! `getheaders`/`headers` request-response cycle. The caller provides a
//! callback (`on_headers`) that receives each batch of raw headers and returns
//! the updated chain tip (or an error), keeping persistence and validation in
//! the caller so this crate stays free of any storage dependency.

use std::{net::SocketAddr, thread, time::Duration};

use zkpow_params::Consensus;
use zkpow_types::BlockHash;

use crate::{
    connection::PeerConnection,
    error::{ConnectionError, SyncHeadersError},
    msg::MAX_HEADERS_PER_MSG,
    peers::PeerStore,
    types::{Header, Network, Tip},
};

/// How many times to retry a connection attempt to the same peer.
const CONNECT_RETRIES: u32 = 3;

#[derive(Debug, thiserror::Error)]
enum PeerSyncError<E: std::fmt::Debug> {
    #[error("connection failed: {0}")]
    Connect(#[source] ConnectionError),
    #[error("peer error: {0}")]
    Peer(#[source] ConnectionError),
    #[error("header processing error: {0:?}")]
    Callback(E),
}

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
pub fn sync<P, C, E>(
    network: Network,
    anchor: &Tip,
    max_headers: u32,
    peer_store: &mut PeerStore,
    mut on_headers: C,
) -> Result<Tip, SyncHeadersError<E>>
where
    P: Consensus,
    C: FnMut(&[Header], &Tip) -> Result<Tip, E>,
    E: std::fmt::Debug,
{
    let mut current_tip = anchor.clone();
    let mut total_synced: u32 = 0;
    let mut batch: u32 = 0;
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

        match sync_with_peer::<P, _, _>(
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
            Err(error) => {
                match error {
                    PeerSyncError::Connect(source) => {
                        tracing::warn!(peer = %peer_addr, error = %source, "Connection failed, trying different peer");
                    },
                    PeerSyncError::Peer(source) => {
                        tracing::warn!(error = %source, synced = total_synced, "Peer error during sync, trying different peer");
                    },
                    PeerSyncError::Callback(source) => {
                        return Err(SyncHeadersError::Callback {
                            source:     Box::new(source),
                            latest_tip: Box::new(current_tip),
                            synced:     total_synced,
                        });
                    },
                }
                peer_store.mark_unreachable(&peer_addr);
            },
        }
    }

    Ok(current_tip)
}

fn sync_with_peer<P, C, E>(
    peer_addr: SocketAddr,
    network: Network,
    current_tip: &mut Tip,
    max_headers: u32,
    total_synced: &mut u32,
    batch: &mut u32,
    on_headers: &mut C,
) -> Result<(), PeerSyncError<E>>
where
    P: Consensus,
    C: FnMut(&[Header], &Tip) -> Result<Tip, E>,
    E: std::fmt::Debug,
{
    let connect = || PeerConnection::connect(peer_addr, network, current_tip.height as i32);
    let mut connection = connect();
    for attempt in 1..CONNECT_RETRIES {
        let Err(error) = &connection else { break };
        let backoff = Duration::from_secs(2u64.pow(attempt));
        tracing::warn!(
            peer = %peer_addr, error = %error, attempt, backoff = ?backoff,
            "Connection failed, retrying"
        );
        thread::sleep(backoff);
        connection = connect();
    }

    let mut p2p_conn = connection.map_err(|source| {
        tracing::warn!(peer = %peer_addr, error = %source, "All connection retries exhausted");
        PeerSyncError::Connect(source)
    })?;

    loop {
        if max_headers > 0 && *total_synced >= max_headers {
            break;
        }

        let locators = build_locators::<P>(current_tip);
        let stop_hash = BlockHash::new([0u8; 32]);

        p2p_conn.send_getheaders(&locators, stop_hash).map_err(PeerSyncError::Peer)?;

        let raw_headers = p2p_conn.read_headers().map_err(PeerSyncError::Peer)?;

        if raw_headers.is_empty() {
            tracing::info!(height = current_tip.height, "Peer has no more headers, we're at the tip");
            break;
        }

        *batch += 1;
        tracing::info!(batch, received = raw_headers.len(), "Received headers from peer");

        let batch_headers = if max_headers > 0 {
            let remaining = (max_headers - *total_synced) as usize;
            &raw_headers[..raw_headers.len().min(remaining)]
        } else {
            &raw_headers[..]
        };

        let new_tip = on_headers(batch_headers, current_tip).map_err(PeerSyncError::Callback)?;

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

        if valid_count < batch_headers.len() as u32 {
            return Err(PeerSyncError::Peer(ConnectionError::InvalidHeaders {
                valid: valid_count as usize,
                total: batch_headers.len(),
            }));
        }

        if raw_headers.len() < MAX_HEADERS_PER_MSG {
            tracing::info!("Received partial batch, peer is caught up");
            break;
        }
    }

    Ok(())
}

fn build_locators<P: Consensus>(tip: &Tip) -> Vec<BlockHash> {
    vec![tip.hash, P::GENESIS_HASH]
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        io::{Read, Write},
        net::TcpListener,
    };

    use zkpow_params::Mainnet;

    use super::*;

    #[test]
    fn exhausted_retries_preserve_the_last_handshake_error_and_tip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for attempt in 0..CONNECT_RETRIES {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut frame = [0; 24];
                stream.read_exact(&mut frame).unwrap();
                let length = u32::from_le_bytes(frame[16..20].try_into().unwrap());
                stream.read_exact(&mut vec![0; length as usize]).unwrap();
                stream.write_all(&[attempt as u8; 4]).unwrap();
            }
        });

        let mut tip = Tip::genesis::<Mainnet>(Mainnet::GENESIS_HASH);
        let mut total_synced = 0;
        let mut batch = 0;
        let mut on_headers = |_: &[Header], _: &Tip| -> Result<Tip, std::convert::Infallible> {
            panic!("failed handshakes must not invoke the callback")
        };
        let error = sync_with_peer::<Mainnet, _, _>(
            peer,
            Network::Mainnet,
            &mut tip,
            0,
            &mut total_synced,
            &mut batch,
            &mut on_headers,
        )
        .unwrap_err();

        assert!(matches!(
            &error,
            PeerSyncError::Connect(ConnectionError::InvalidMagic { received, .. })
                if *received == [(CONNECT_RETRIES - 1) as u8; 4]
        ));
        assert_eq!(tip.hash, Mainnet::GENESIS_HASH);
        assert_eq!(tip.height, 0);
        assert!(error.source().unwrap().downcast_ref::<ConnectionError>().is_some());
        server.join().unwrap();
    }
}
