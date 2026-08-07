use clap::{Args, Parser, Subcommand};
use sync_headers::{connect_headers, tip_from_connected, ConnectedHeader, Header, Network, PeerStore, Tip};
use zkpow_store::HeaderStore;

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
    #[arg(short, long, default_value = "zkpow.sqlite")]
    db: String,

    #[arg(short, long, default_value = "0")]
    max_headers: u64,

    /// Connect to this peer address instead of discovering peers via DNS seeds.
    #[arg(long, default_value = None)]
    p2p_peer: Option<String>,

    /// Bitcoin network to sync.
    #[arg(long, default_value = "bitcoin")]
    p2p_network: String,

    /// Delete the database before syncing.
    #[arg(long)]
    p2p_reset: bool,
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

/// Wire the store-backed callback and run the P2P sync backend.
///
/// `sync_headers::p2p::sync` is callback-based by design: `zkpow-store` depends
/// on `zkpow-sync-headers`, so the producer cannot call back into the store —
/// the CLI owns the store and supplies persistence through the callback here.
fn sync_p2p(args: &SyncArgs) -> Result<(), Box<dyn std::error::Error>> {
    let network = parse_network(&args.p2p_network)?;
    tracing::info!(?network, db = %args.db, "Starting P2P sync");

    if args.p2p_reset {
        tracing::warn!("Resetting database");
        std::fs::remove_file(&args.db).ok();
    }

    let mut store = HeaderStore::open(&args.db)?;

    let anchor = match store.get_best_tip()? {
        Some(tip) => {
            tracing::info!(height = tip.height, hash = %tip.hash, "Resuming from existing tip");
            tip
        },
        None => {
            tracing::info!("Initializing database with genesis block");
            let genesis = genesis_connected_header();
            store.insert_genesis(&genesis)?;
            Tip::genesis(genesis.block_hash())
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

    let final_tip = sync_headers::p2p::sync(
        network,
        &anchor,
        args.max_headers,
        &mut peer_store,
        |raw_headers, tip| -> Result<Tip, String> {
            let connected = connect_headers(tip, raw_headers);
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

fn genesis_connected_header() -> ConnectedHeader {
    let header = Header::new(zkpow_core::MAINNET_GENESIS_HEADER);
    let block_hash = header.block_hash();
    ConnectedHeader::new(header, block_hash, 0, zkpow_core::GENESIS_WORK, 0, 0)
}

fn parse_network(s: &str) -> Result<Network, Box<dyn std::error::Error>> {
    match s.to_lowercase().as_str() {
        "bitcoin" | "mainnet" | "main" => Ok(Network::Bitcoin),
        "testnet" | "test" => Ok(Network::Testnet),
        "signet" => Ok(Network::Signet),
        "regtest" | "reg" => Ok(Network::Regtest),
        other => Err(format!("unknown network '{other}'; use: bitcoin, testnet, signet, regtest").into()),
    }
}
