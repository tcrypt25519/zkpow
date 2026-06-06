use std::sync::Once;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*, EnvFilter};

use crate::pipeline::input::ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS;

static INIT: Once = Once::new();

/// Holds the non-blocking file writer guard. Must be kept alive for the duration of the process.
static mut LOG_GUARD: Option<WorkerGuard> = None;

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Initialise tracing.
///
/// Two layers are registered:
/// - **stderr** — compact human-readable output, filtered by `RUST_LOG` (default: off).
/// - **`logs/run.jsonl`** — newline-delimited JSON, always written at `info` level and above.
///   Agents can query this file directly; see the Debugging section in AGENTS.md.
pub fn init() {
    INIT.call_once(|| {
        let show_detailed_spans = env_truthy(ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS);
        let span_events = if show_detailed_spans {
            FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        };

        // --- stderr layer (human-readable, respects RUST_LOG) ---
        let fmt_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("off"))
            .add_directive("hyper=off".parse().unwrap())
            .add_directive("slop_keccak_air=off".parse().unwrap())
            .add_directive("sp1_sdk::mock=warn".parse().unwrap())
            .add_directive("sp1_prover::worker::prover::execute=warn".parse().unwrap())
            .add_directive("p3_fri=off".parse().unwrap())
            .add_directive("p3_dft=off".parse().unwrap())
            .add_directive("p3_challenger=off".parse().unwrap());

        let stderr_layer = tracing_subscriber::fmt::layer()
            .compact()
            .with_file(false)
            .with_target(false)
            .with_thread_names(false)
            .with_span_events(span_events.clone())
            .with_filter(fmt_filter);

        // --- JSON file layer (always on at info+, written to logs/run.jsonl) ---
        std::fs::create_dir_all("logs").expect("failed to create logs/ directory");
        let file_appender = tracing_appender::rolling::never("logs", "run.jsonl");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // SAFETY: written once inside Once::call_once, never read until process exit.
        unsafe { LOG_GUARD = Some(guard) };

        let json_level = if show_detailed_spans { "debug" } else { "info" };

        let json_filter = EnvFilter::new(json_level)
            .add_directive("hyper=off".parse().unwrap())
            .add_directive("slop_keccak_air=off".parse().unwrap())
            .add_directive("sp1_sdk::mock=warn".parse().unwrap())
            .add_directive("sp1_prover::worker::prover::execute=warn".parse().unwrap())
            .add_directive("p3_fri=off".parse().unwrap())
            .add_directive("p3_dft=off".parse().unwrap())
            .add_directive("p3_challenger=off".parse().unwrap());

        let json_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_span_events(span_events)
            .with_filter(json_filter);

        let enable_console = env_truthy("SP1_ENABLE_TOKIO_CONSOLE");
        tracing_subscriber::registry()
            .with(enable_console.then(console_subscriber::spawn))
            .with(stderr_layer)
            .with(json_layer)
            .init();

        if enable_console {
            let console_bind = std::env::var("TOKIO_CONSOLE_BIND")
                .unwrap_or_else(|_| "127.0.0.1:6669".to_string());
            println!("Tokio Console subscriber initialized on {console_bind}");
        }
        println!("Structured JSON logs → logs/run.jsonl");
    });
}
