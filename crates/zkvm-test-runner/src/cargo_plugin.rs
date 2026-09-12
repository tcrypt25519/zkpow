//! `cargo-zkvm` binary entrypoint.

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = zkvm_test_runner::plugin::run_cargo_zkvm(args).await;
    if code != 0 {
        std::process::exit(code);
    }
}
