use std::sync::Once;

use tracing_subscriber::{fmt::format::FmtSpan, prelude::*, EnvFilter};

static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(|| {
        let fmt_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("off"))
            .add_directive("hyper=off".parse().unwrap())
            .add_directive("slop_keccak_air=off".parse().unwrap())
            .add_directive("p3_fri=off".parse().unwrap())
            .add_directive("p3_dft=off".parse().unwrap())
            .add_directive("p3_challenger=off".parse().unwrap());

        tracing_subscriber::registry()
            .with(console_subscriber::spawn())
            .with(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_file(false)
                    .with_target(false)
                    .with_thread_names(false)
                    .with_span_events(FmtSpan::CLOSE)
                    .with_filter(fmt_filter),
            )
            .init();

        let console_bind =
            std::env::var("TOKIO_CONSOLE_BIND").unwrap_or_else(|_| "127.0.0.1:6669".to_string());
        println!("Tokio Console subscriber initialized on {console_bind}");
    });
}
