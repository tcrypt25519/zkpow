use std::sync::Once;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*, EnvFilter};

use crate::pipeline::diagnostics::ShardAreas;

const QUIET_DIRECTIVES: [&str; 7] = [
    "hyper=off",
    "slop_keccak_air=off",
    "sp1_sdk::mock=warn",
    "sp1_prover::worker::prover::execute=warn",
    "p3_fri=off",
    "p3_dft=off",
    "p3_challenger=off",
];

static INIT: Once = Once::new();

/// Holds the non-blocking file writer guard. Must be kept alive for the duration of the process.
static mut LOG_GUARD: Option<WorkerGuard> = None;

/// Observability configuration for tracing, console, and log levels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityConfig {
    pub show_compressed_proof_spans: bool,
    pub enable_tokio_console:        bool,
    pub tokio_console_bind:          String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            show_compressed_proof_spans: false,
            enable_tokio_console:        false,
            tokio_console_bind:          "127.0.0.1:6669".to_string(),
        }
    }
}

fn quiet(filter: EnvFilter) -> EnvFilter {
    QUIET_DIRECTIVES
        .iter()
        .fold(filter, |filter, directive| filter.add_directive(directive.parse().unwrap()))
}

/// Initialize tracing with the provided configuration.
///
/// Two layers are registered:
/// - **stderr**: compact human-readable output, filtered by `RUST_LOG` (default: off).
/// - **`logs/run.jsonl`**: newline-delimited JSON, always written at `info` level and above.
pub fn init(config: &ObservabilityConfig) {
    INIT.call_once(|| {
        let span_events = if config.show_compressed_proof_spans {
            FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        };

        // stderr layer
        let fmt_filter = quiet(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off")));

        let stderr_layer = tracing_subscriber::fmt::layer()
            .compact()
            .with_file(false)
            .with_target(false)
            .with_thread_names(false)
            .with_span_events(span_events.clone())
            .with_filter(fmt_filter);

        // JSON file layer
        std::fs::create_dir_all("logs").expect("failed to create logs/ directory");
        let file_appender = tracing_appender::rolling::never("logs", "run.jsonl");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // SAFETY: written once inside Once::call_once, never read until process exit.
        unsafe { LOG_GUARD = Some(guard) };

        let json_level = if config.show_compressed_proof_spans {
            "debug"
        } else {
            "info"
        };

        let json_filter = quiet(EnvFilter::new(json_level));

        let json_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_span_events(span_events)
            .with_filter(json_filter);

        // shard area layer
        let shard_layer = ShardAreas
            .with_filter(EnvFilter::new("off").add_directive("sp1_hypercube::prover::shard=debug".parse().unwrap()));

        tracing_subscriber::registry()
            .with(config.enable_tokio_console.then(console_subscriber::spawn))
            .with(stderr_layer)
            .with(json_layer)
            .with(shard_layer)
            .init();

        if config.enable_tokio_console {
            println!("Tokio Console subscriber initialized on {}", config.tokio_console_bind);
        }
        println!("Structured JSON logs → logs/run.jsonl");
    });
}

/// Initialize tracing from host environment configuration.
pub fn init_from_env() {
    let config = crate::pipeline::input::observability_config_from_env();
    init(&config);
}
