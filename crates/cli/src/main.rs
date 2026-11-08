use std::{net::SocketAddr, thread, time::Duration};

use clap::{Args, Parser, Subcommand};
use sync_headers::{
    ConnectedHeader, ConnectionError, Header, Network, PeerConnection, PeerStore, Tip, connect_headers,
};
use zkpow_params::{Consensus, Mainnet};
use zkpow_store::{DEFAULT_DB_FILE_NAME, HeaderStore};
use zkpow_types::{BlockHash, ChainWork};

#[derive(Parser, Debug)]
#[command(name = "zkpow-cli")]
#[command(about = "Sync and validate Bitcoin block headers")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Sync block headers from the P2P network into the local database.
    Sync(SyncArgs),
}

#[derive(Args, Debug)]
struct SyncArgs {
    #[arg(short, long, default_value = DEFAULT_DB_FILE_NAME)]
    db: String,

    #[arg(short, long, default_value = "0")]
    max_headers: u32,

    /// Connect to this peer address instead of discovering peers via DNS seeds.
    #[arg(long, default_value = None)]
    p2p_peer: Option<String>,

    /// Bitcoin network to sync.
    #[arg(long, default_value = "bitcoin")]
    p2p_network: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "zkpow_cli=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Sync(args) => sync_p2p(&args)?,
    }

    Ok(())
}

fn sync_p2p(args: &SyncArgs) -> Result<(), Box<dyn std::error::Error>> {
    let Ok(network) = args.p2p_network.parse::<Network>() else {
        return Err("network not found".into());
    };
    if network != Network::Mainnet {
        return Err(format!("unsupported network: {}", network).into());
    }
    tracing::info!(?network, db = %args.db, "Starting P2P sync");

    let mut store = HeaderStore::open(&args.db)?;

    let anchor = match store.get_best_tip::<Mainnet>()? {
        Some(tip) => {
            tracing::info!(height = tip.height, hash = %tip.hash, "Resuming from existing tip");
            tip
        },
        None => {
            tracing::info!("Initializing database with genesis block");
            let genesis = genesis_connected_header::<Mainnet>();
            store.insert_genesis(&genesis)?;
            Tip::genesis::<Mainnet>(genesis.block_hash())
        },
    };

    let mut peer_store = if let Some(addr_str) = &args.p2p_peer {
        let addr = addr_str.parse()?;
        let mut ps = PeerStore::default();
        ps.add_peer(addr);
        tracing::info!(peer = %addr, "Using explicit peer address");
        ps
    } else {
        tracing::info!("Resolving DNS seeds to discover peers...");
        PeerStore::from_dns_seeds()?
    };

    let final_tip = sync_from_peers::<Mainnet>(network, anchor, args.max_headers, &mut peer_store, &mut store)?;

    tracing::info!(
        height = final_tip.height,
        hash = %final_tip.hash,
        total_headers = store.header_count()?,
        "Sync complete"
    );

    Ok(())
}

fn sync_from_peers<C: Consensus>(
    network: Network,
    mut tip: Tip,
    max_headers: u32,
    peers: &mut PeerStore,
    store: &mut HeaderStore,
) -> Result<Tip, Box<dyn std::error::Error>> {
    let mut synced = 0u32;
    let mut last_peer_error = None;
    let mut rng = rand::rng();

    loop {
        if max_headers != 0 && synced >= max_headers {
            return Ok(tip);
        }

        let peer = match peers.pick_candidate(&mut rng) {
            Some(peer) => peer,
            None => {
                return Err(last_peer_error
                    .unwrap_or_else(|| "no candidate peers available".to_string())
                    .into());
            },
        };

        let mut connection = match connect_with_retries(peer, network, tip.height as i32) {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(%peer, %error, "Connection failed");
                last_peer_error = Some(error.to_string());
                peers.mark_unreachable(&peer);
                continue;
            },
        };

        loop {
            if max_headers != 0 && synced >= max_headers {
                return Ok(tip);
            }

            let locators = [tip.hash, C::GENESIS_HASH];
            let request = connection.send_getheaders(&locators, BlockHash::new([0; 32]));
            let headers = request.and_then(|()| connection.read_headers());
            let headers = match headers {
                Ok(headers) => headers,
                Err(error) => {
                    tracing::warn!(%peer, %error, "Peer request failed");
                    last_peer_error = Some(error.to_string());
                    break;
                },
            };

            let limit = if max_headers == 0 {
                headers.len()
            } else {
                headers.len().min((max_headers - synced) as usize)
            };
            let headers = &headers[..limit];
            // TODO: Don't assume we're done just because a peer sent us 0 headers
            if headers.is_empty() {
                peers.mark_connected(&peer);
                return Ok(tip);
            }

            let connected = connect_headers::<C>(&mut tip, headers);
            if !connected.is_empty() {
                store.bulk_insert(&connected)?;
                synced += connected.len() as u32;
                tracing::info!(height = tip.height, %synced, "Headers synced");
            }
            if connected.len() != limit {
                tracing::warn!(%peer, accepted = connected.len(), received = limit, "Invalid headers");
                last_peer_error = Some(format!(
                    "peer {peer} sent invalid headers: accepted {} of {limit}",
                    connected.len()
                ));
                break;
            }
        }

        peers.mark_unreachable(&peer);
    }
}

fn connect_with_retries(peer: SocketAddr, network: Network, height: i32) -> Result<PeerConnection, ConnectionError> {
    let mut connection = PeerConnection::connect(peer, network, height);
    for attempt in 1..3 {
        if connection.is_ok() {
            break;
        }
        thread::sleep(Duration::from_secs(2u64.pow(attempt)));
        connection = PeerConnection::connect(peer, network, height);
    }
    connection
}

fn genesis_connected_header<C: Consensus>() -> ConnectedHeader {
    let header = Header::new(C::PARAMS.genesis);
    let block_hash = header.block_hash();
    let chainwork = ChainWork::from_limbs(C::PARAMS.genesis_work.into_inner().into_limbs());
    ConnectedHeader::new(header, block_hash, 0, chainwork, 0)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        thread,
    };

    use sha2::{Digest, Sha256};
    use zkpow_core::{check_proof_of_work, target_from_bits};
    use zkpow_params::ConsensusParams;
    use zkpow_store::SQLITE_MEMORY_DATABASE;
    use zkpow_types::{BlockTimestamp, BlockWork, CompactTarget, Header as CoreHeader, MerkleRoot, Target};

    use super::*;

    const EASY_BITS: u32 = 0x2007_ffff;

    struct TestConsensus;

    impl Consensus for TestConsensus {
        const PARAMS: ConsensusParams = ConsensusParams {
            genesis:             CoreHeader {
                version:        1,
                prev_blockhash: BlockHash::new([0; 32]),
                merkle_root:    MerkleRoot::new([0; 32]),
                timestamp:      BlockTimestamp::new(2_000_000_000),
                compact_target: CompactTarget::new(EASY_BITS),
                nonce:          0,
            },
            genesis_target:      Target::from_limbs([0, 0, 0, 0x07ff_ff00_0000_0000]),
            genesis_work:        BlockWork::from_limbs([32, 0, 0, 0]),
            block_time:          600,
            epoch_length:        2016,
            max_epoch_over_mul:  4,
            min_epoch_under_div: 4,
        };
    }

    fn mine_header(prev: BlockHash, timestamp: u32) -> Header {
        (0..)
            .map(|nonce| {
                Header::new(CoreHeader {
                    version: 1,
                    prev_blockhash: prev,
                    merkle_root: MerkleRoot::new([0; 32]),
                    timestamp: BlockTimestamp::new(timestamp),
                    compact_target: CompactTarget::new(EASY_BITS),
                    nonce,
                })
            })
            .find(|header| check_proof_of_work(header.block_hash(), &target_from_bits(CompactTarget::new(EASY_BITS))))
            .expect("easy target has a solution")
    }

    fn read_frame(stream: &mut std::net::TcpStream) {
        let mut frame = [0u8; 24];
        stream.read_exact(&mut frame).unwrap();
        let len = u32::from_le_bytes(frame[16..20].try_into().unwrap()) as usize;
        let mut payload = vec![0; len];
        stream.read_exact(&mut payload).unwrap();
    }

    fn write_frame(stream: &mut std::net::TcpStream, command: &[u8], payload: &[u8]) {
        let mut frame = Vec::with_capacity(24 + payload.len());
        frame.extend_from_slice(&[0xf9, 0xbe, 0xb4, 0xd9]);
        let mut name = [0u8; 12];
        name[..command.len()].copy_from_slice(command);
        frame.extend_from_slice(&name);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&Sha256::digest(Sha256::digest(payload))[..4]);
        frame.extend_from_slice(payload);
        stream.write_all(&frame).unwrap();
    }

    fn serve_headers(batches: Vec<Vec<Header>>) -> (SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_frame(&mut stream);
            let mut version = Vec::new();
            version.extend_from_slice(&70015i32.to_le_bytes());
            version.extend_from_slice(&0u64.to_le_bytes());
            version.extend_from_slice(&0i64.to_le_bytes());
            version.extend_from_slice(&0u64.to_le_bytes());
            version.extend_from_slice(&[0u8; 18]);
            version.extend_from_slice(&0u64.to_le_bytes());
            version.extend_from_slice(&[0u8; 18]);
            version.extend_from_slice(&0u64.to_le_bytes());
            version.push(0);
            version.extend_from_slice(&0i32.to_le_bytes());
            version.push(0);
            write_frame(&mut stream, b"version", &version);
            read_frame(&mut stream);
            write_frame(&mut stream, b"verack", &[]);
            read_frame(&mut stream);
            for headers in batches {
                read_frame(&mut stream);
                let mut payload = vec![headers.len() as u8];
                for header in headers {
                    payload.extend_from_slice(&header.inner.to_bytes_const());
                    payload.push(0);
                }
                write_frame(&mut stream, b"headers", &payload);
            }
        });
        (address, server)
    }

    #[test]
    fn rejects_unknown_and_unsupported_networks() {
        for network in ["invalid", "testnet", "regtest"] {
            let args = SyncArgs {
                db:          SQLITE_MEMORY_DATABASE.to_string(),
                max_headers: 1,
                p2p_peer:    None,
                p2p_network: network.to_string(),
            };
            assert!(sync_p2p(&args).is_err(), "{network} must fail");
        }
    }

    #[test]
    fn sync_honors_unlimited_and_capped_header_counts() {
        for limit in [0, 1] {
            let mut store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
            store.insert_genesis(&genesis_connected_header::<TestConsensus>()).unwrap();
            let anchor = Tip::genesis::<TestConsensus>(TestConsensus::GENESIS_HASH);
            let first = mine_header(anchor.hash, 2_000_000_001);
            let second = mine_header(first.block_hash(), 2_000_000_002);
            let mut batches = vec![vec![first, second]];
            if limit == 0 {
                batches.push(vec![]);
            }
            let (address, server) = serve_headers(batches);
            let mut peers = PeerStore::default();
            peers.add_peer(address);
            let tip =
                sync_from_peers::<TestConsensus>(Network::Mainnet, anchor, limit, &mut peers, &mut store).unwrap();
            server.join().unwrap();
            let expected = if limit == 0 { 2 } else { 1 };
            assert_eq!(tip.height, expected);
            assert_eq!(store.header_count().unwrap(), u64::from(expected) + 1);
            assert_eq!(
                store.get_best_tip::<TestConsensus>().unwrap().unwrap().timestamps,
                tip.timestamps
            );
        }
    }

    #[test]
    fn sync_persists_valid_prefix_and_resumes_from_its_window() {
        let mut store = HeaderStore::open(SQLITE_MEMORY_DATABASE).unwrap();
        store.insert_genesis(&genesis_connected_header::<TestConsensus>()).unwrap();
        let anchor = Tip::genesis::<TestConsensus>(TestConsensus::GENESIS_HASH);
        let first = mine_header(anchor.hash, 2_000_000_001);
        let second = mine_header(first.block_hash(), 2_000_000_002);
        let third = mine_header(second.block_hash(), 2_000_000_003);
        let mut invalid = second.clone();
        invalid.inner.prev_blockhash = BlockHash::new([0xff; 32]);

        let (bad_addr, bad_server) = serve_headers(vec![vec![first.clone(), invalid]]);
        let mut peers = PeerStore::default();
        peers.add_peer(bad_addr);
        assert!(sync_from_peers::<TestConsensus>(Network::Mainnet, anchor, 0, &mut peers, &mut store).is_err());
        bad_server.join().unwrap();
        let saved = store.get_best_tip::<TestConsensus>().unwrap().unwrap();
        assert_eq!(saved.height, 1);
        assert_eq!(saved.hash, first.block_hash());
        assert_eq!(saved.timestamps.newest(), first.inner.timestamp);

        let (good_addr, good_server) = serve_headers(vec![vec![second], vec![third.clone()]]);
        peers.add_peer(good_addr);
        let tip = sync_from_peers::<TestConsensus>(Network::Mainnet, saved, 2, &mut peers, &mut store).unwrap();
        good_server.join().unwrap();
        assert_eq!(tip.height, 3);
        assert_eq!(tip.hash, third.block_hash());
        assert_eq!(tip.timestamps.newest(), third.inner.timestamp);
        assert_eq!(store.header_count().unwrap(), 4);
    }
}
