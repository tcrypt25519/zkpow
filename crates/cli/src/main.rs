use clap::{Args, Parser, Subcommand};
use sync_headers::{connect_headers, tip_from_connected, ConnectedHeader, Header, Network, PeerStore, Tip};
use zkpow_params::{Consensus, Mainnet};
use zkpow_store::HeaderStore;
use zkpow_types::ChainWork;

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
    #[arg(short, long, default_value = "headers.db")]
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
        Command::Sync(args) => sync_p2p_for_network(&args)?,
    }

    Ok(())
}

fn sync_p2p_for_network(args: &SyncArgs) -> Result<(), Box<dyn std::error::Error>> {
    sync_p2p::<Mainnet>(args)
}

/// Wire the store-backed callback and run the P2P sync backend.
///
/// `sync_headers::p2p::sync` is callback-based by design: `zkpow-store` depends
/// on `zkpow-sync-headers`, so the producer cannot call back into the store.
/// The CLI owns the store and supplies persistence through the callback here.
fn sync_p2p<C: Consensus>(args: &SyncArgs) -> Result<(), Box<dyn std::error::Error>> {
    let Some(network) = Network::parse(&args.p2p_network) else {
        return Err("network not found".into());
    };
    tracing::info!(?network, db = %args.db, "Starting P2P sync");

    let mut store = HeaderStore::open(&args.db)?;

    let anchor = match store.get_best_tip()? {
        Some(tip) => {
            tracing::info!(height = tip.height, hash = %tip.hash, "Resuming from existing tip");
            tip
        },
        None => {
            tracing::info!("Initializing database with genesis block");
            let genesis = genesis_connected_header::<C>();
            store.insert_genesis(&genesis)?;
            Tip::genesis::<C>(genesis.block_hash())
        },
    };

    let mut peer_store = if let Some(addr_str) = &args.p2p_peer {
        let addr = addr_str.parse()?;
        let mut ps = PeerStore::default();
        ps.mark_connected(&addr);
        tracing::info!(peer = %addr, "Using explicit peer address");
        ps
    } else {
        tracing::info!("Resolving DNS seeds to discover peers...");
        PeerStore::from_dns_seeds()?
    };

    let final_tip = sync_headers::p2p::sync::<C, _, _>(
        network,
        &anchor,
        args.max_headers,
        &mut peer_store,
        |raw_headers, tip| -> Result<Tip, String> {
            let connected = connect_headers::<C>(tip, raw_headers);
            if connected.is_empty() {
                return Err("no headers could be connected to the tip".into());
            }
            let new_tip = tip_from_connected(tip, &connected);
            store.bulk_insert(&connected).map_err(|e| e.to_string())?;
            Ok(new_tip)
        },
    )
    .map_err(|e| format!("{e:?}"))?;

    tracing::info!(
        height = final_tip.height,
        hash = %final_tip.hash,
        total_headers = store.header_count()?,
        "Sync complete"
    );

    Ok(())
}

fn genesis_connected_header<C: Consensus>() -> ConnectedHeader {
    let header = Header::new(C::PARAMS.genesis);
    let block_hash = header.block_hash();
    let chainwork = ChainWork::from_limbs(C::PARAMS.genesis_work.into_inner().into_limbs());
    ConnectedHeader::new(header, block_hash, 0, chainwork, 0, 0)
}
