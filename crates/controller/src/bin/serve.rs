//! HTTP server exposing a `Controller` over the network, so a `zkpow-host`
//! process can fetch batches from a remote machine instead of opening the
//! SQLite database directly.
//!
//! Routes:
//!   GET  /genesis_hash                          -> {"genesis_hash_hex": "<64 hex chars>"}
//!   GET  /state/{height}                         -> {"state_hex": "<hex>"}
//!   POST /batch?num_headers=N&simulate=bool      -> body {"current_state_hex": "<hex>"}
//!                                                    -> success/error envelope
//!
//! Environment:
//!   ZKPOW_DB_PATH          SQLite database path (default: headers.db at workspace root)
//!   ZKPOW_CONTROLLER_PORT  TCP port to listen on (default: 7878)

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use zkpow_controller::http::build_router;
use zkpow_controller::{db_path, Controller, HeaderStore};

const DEFAULT_PORT: u16 = 7878;

fn controller_port() -> u16 {
    std::env::var("ZKPOW_CONTROLLER_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let db = HeaderStore::open(db_path()).expect("failed to open database");
    let genesis_hash = zkpow_controller::BlockHash::new(zkpow_core::MAINNET_GENESIS_HASH_RAW);
    let controller = Arc::new(Mutex::new(Controller::new(db, genesis_hash)));
    let app = build_router(controller);

    let port = controller_port();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));
    tracing::info!("zkpow-controller-serve listening on {addr}");
    axum::serve(listener, app).await.expect("server error");
}
