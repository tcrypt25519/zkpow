//! HTTP server exposing a `ControllerRuntime` over the network, so
//! `zkpow-host` processes on GPU instances can pull batches, upload proof
//! artifacts to controller-chosen storage targets, and report completions.
//!
//! Routes (binary frames, see `zkpow_controller::protocol`):
//!   POST /v1/batch     -> batch grant
//!   POST /v1/complete  -> complete ack
//!
//! Errors are plain-text bodies.
//!
//! Environment:
//!   ZKPOW_DB_PATH            SQLite database (default: headers.db at workspace
//!                            root); holds both headers and run progress
//!   ZKPOW_CONTROLLER_BIND    address to listen on (default: 0.0.0.0; set your
//!                            private Tailscale IP to avoid public exposure)
//!   ZKPOW_CONTROLLER_PORT    TCP port to listen on (default: 7878)
//!   ZKPOW_ALLOW_UNPROVEN     accept execute-only completions (default: off)
//!
//! Artifact storage (S3-compatible when ZKPOW_S3_BUCKET is set, otherwise a
//! local directory — the latter only works when hosts share this filesystem):
//!   ZKPOW_S3_BUCKET / ZKPOW_S3_REGION / ZKPOW_S3_ENDPOINT
//!   ZKPOW_S3_ACCESS_KEY / ZKPOW_S3_SECRET_KEY / ZKPOW_S3_PREFIX
//!   ZKPOW_PROOF_STORE_DIR    local artifact root (default: proof_store)
//!
//! Proof verification (requires building with `--features sp1-verify`):
//!   ZKPOW_GUEST_ELF          path to the guest ELF; when set, uploaded
//!                            proofs are cryptographically verified and new
//!                            runs are pinned to this guest's verifier key.

use std::net::SocketAddr;

use zkpow_controller::runtime::{ControllerConfig, ControllerRuntime};
use zkpow_controller::storage::{ArtifactStore, LocalArtifactStore, S3ArtifactStore, S3Config};
use zkpow_controller::verify::ProofVerifier;
use zkpow_controller::{db_path, http::build_router, BlockHash, HeaderStore};

const DEFAULT_PORT: u16 = 7878;
const DEFAULT_PROOF_STORE_DIR: &str = "proof_store";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_flag(name: &str) -> bool {
    matches!(env(name).as_deref(), Some("1" | "true" | "yes" | "on"))
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| panic!("{name} is required when ZKPOW_S3_BUCKET is set"))
}

fn storage_from_env() -> Box<dyn ArtifactStore> {
    match env("ZKPOW_S3_BUCKET") {
        Some(bucket) => {
            tracing::info!(bucket, "using S3 artifact storage with pre-signed URLs");
            Box::new(
                S3ArtifactStore::new(&S3Config {
                    bucket,
                    region: require("ZKPOW_S3_REGION"),
                    endpoint: require("ZKPOW_S3_ENDPOINT"),
                    access_key: require("ZKPOW_S3_ACCESS_KEY"),
                    secret_key: require("ZKPOW_S3_SECRET_KEY"),
                    prefix: env("ZKPOW_S3_PREFIX").unwrap_or_default(),
                })
                .expect("failed to construct S3 artifact store"),
            )
        }
        None => {
            let root = env("ZKPOW_PROOF_STORE_DIR")
                .unwrap_or_else(|| DEFAULT_PROOF_STORE_DIR.to_string());
            tracing::warn!(
                root,
                "using local artifact storage; hosts must share this filesystem \
                 (set ZKPOW_S3_* for remote hosts)"
            );
            Box::new(LocalArtifactStore::new(root))
        }
    }
}

async fn verifier_from_env() -> Option<Box<dyn ProofVerifier>> {
    let Some(elf_path) = env("ZKPOW_GUEST_ELF") else {
        tracing::warn!(
            "ZKPOW_GUEST_ELF not set; uploaded proofs are validated against expected \
             public values only, not cryptographically verified"
        );
        return None;
    };

    #[cfg(feature = "sp1-verify")]
    {
        let elf = std::fs::read(&elf_path)
            .unwrap_or_else(|err| panic!("failed to read guest ELF {elf_path}: {err}"));
        tracing::info!(elf_path, "setting up SP1 proof verification (this can take a while)");
        let verifier = zkpow_controller::verify::Sp1ElfVerifier::from_elf(elf)
            .await
            .expect("failed to set up SP1 verifier");
        tracing::info!(
            verifier_key = ?verifier.verifier_key_digest(),
            "SP1 proof verification enabled"
        );
        Some(Box::new(verifier) as Box<dyn ProofVerifier>)
    }

    #[cfg(not(feature = "sp1-verify"))]
    {
        panic!(
            "ZKPOW_GUEST_ELF={elf_path} is set, but this binary was built without proof \
             verification; rebuild with `--features sp1-verify`"
        );
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let db = HeaderStore::open(db_path()).expect("failed to open database");
    let genesis_hash = BlockHash::new(zkpow_core::MAINNET_GENESIS_HASH_RAW);

    let mut config = ControllerConfig::new(genesis_hash);
    config.allow_unproven = env_flag("ZKPOW_ALLOW_UNPROVEN");

    let runtime =
        ControllerRuntime::new(db, storage_from_env(), verifier_from_env().await, config);
    let app = build_router(runtime);

    let bind = env("ZKPOW_CONTROLLER_BIND").unwrap_or_else(|| "0.0.0.0".to_string());
    let port = env("ZKPOW_CONTROLLER_PORT")
        .map(|value| value.parse().expect("invalid ZKPOW_CONTROLLER_PORT"))
        .unwrap_or(DEFAULT_PORT);
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .unwrap_or_else(|err| panic!("invalid bind address {bind}:{port}: {err}"));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));
    tracing::info!("zkpow-controller-serve listening on {addr}");
    axum::serve(listener, app).await.expect("server error");
}
