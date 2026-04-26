//! Inspect a saved Bitcoin header chain proof and display its public inputs.

use sp1_sdk::SP1ProofWithPublicValues;

use zkpow_host::util;
use zkpow_host::util::{HeaderChainPublicValues, PublicValuesParseError, ValidationErrorCode};

const MAINNET_GENESIS_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

fn reverse_hash_display(hash: util::BlockHash) -> String {
    let mut reversed = hash.into_raw();
    reversed.reverse();
    hex::encode(reversed)
}

fn main() {
    let proof_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bitcoin-header-chain-proof.bin".to_string());

    println!("Loading proof from: {}", proof_path);
    let proof = SP1ProofWithPublicValues::load(&proof_path).expect("failed to load proof file");

    let pv = proof.public_values.as_ref();
    println!("\n=== Bitcoin Header Chain Proof ===\n");

    match HeaderChainPublicValues::parse(pv) {
        Ok(HeaderChainPublicValues::Success(state)) => {
            display_state(&state);
        }
        Ok(HeaderChainPublicValues::Failure(failure)) => {
            println!("--- Error Output ---");
            display_state(&failure.last_valid_state);
            println!("\n--- Error ---");
            let error_name = match failure.error_code {
                ValidationErrorCode::HeaderPayloadLengthInvalid => "Header payload length invalid",
                ValidationErrorCode::PowInsufficient => "PoW insufficient",
                ValidationErrorCode::TimestampTooOld => "Timestamp too old",
                ValidationErrorCode::GenesisHashMismatch => "Genesis hash mismatch",
                ValidationErrorCode::MedianTimePastHintInvalid => {
                    "Median time past hint invalid"
                }
            };
            println!(
                "Error Code:        {} ({})",
                failure.error_code as u8, error_name,
            );
            println!("Header Index:      {}", failure.header_index);
        }
        Err(parse_error) => {
            display_parse_error(parse_error, pv);
            std::process::exit(1);
        }
    }

    // Proof metadata
    println!("\n--- Proof Details ---");
    println!("SP1 Version:       {}", proof.sp1_version);
    println!("Proof Type:        {:?}", proof.proof);
    println!("Public Values Size: {} bytes", pv.len());
}

fn display_state(state: &util::State) {
    use std::time::UNIX_EPOCH;

    // Genesis hash
    println!(
        "Genesis Hash:      {}",
        reverse_hash_display(state.genesis_hash)
    );
    let mainnet_genesis_raw: [u8; 32] = {
        let mut g: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX)
            .unwrap()
            .try_into()
            .unwrap();
        g.reverse();
        g
    };
    if state.genesis_hash == util::BlockHash::from_raw(mainnet_genesis_raw) {
        println!("                     ↳ mainnet ✓");
    } else {
        println!("                     ↳ NOT mainnet (different chain)");
    }

    // Chain tip
    println!(
        "\nChain Tip:         {}",
        reverse_hash_display(state.block_hash)
    );
    println!(
        "Tip Prev Hash:     {}",
        reverse_hash_display(state.header.prev_blockhash)
    );

    // Height
    println!("\nHeight:            {}", state.height);

    // Cumulative chain work
    let work_hex: String = state
        .chain_work
        .as_limbs()
        .iter()
        .rev()
        .map(|w| format!("{:016x}", w))
        .collect();
    println!("Cumulative Work:   0x{}", work_hex);

    // Difficulty
    println!(
        "Header Nbits:      0x{:08x}",
        state.header.nbits.to_consensus()
    );
    println!(
        "Next Nbits:        0x{:08x}",
        state.next_nbits.to_consensus()
    );

    // Epoch start timestamp
    let epoch_start_timestamp = state.epoch_start_timestamp.to_consensus();
    let epoch_dt = UNIX_EPOCH + std::time::Duration::from_secs(epoch_start_timestamp as u64);
    println!(
        "Epoch Start:       {} (timestamp: {})",
        humantime::format_rfc3339_seconds(epoch_dt),
        epoch_start_timestamp
    );

    // Timestamp window
    let timestamp_count = state.timestamp_count();
    if timestamp_count > 0 {
        println!("\nTimestamp Window ({} entries):", timestamp_count);
        for i in 0..timestamp_count {
            let ts = state.timestamps[i].to_consensus();
            let dt = UNIX_EPOCH + std::time::Duration::from_secs(ts as u64);
            println!(
                "  [{}] {} ({})",
                i,
                humantime::format_rfc3339_seconds(dt),
                ts
            );
        }
    }

    println!("\nStatus:              ✓ All headers validated");
}

fn display_parse_error(parse_error: PublicValuesParseError, pv: &[u8]) {
    eprintln!("ERROR: {}", parse_error);
    eprintln!("Raw hex: {}", hex::encode(pv));
}
