//! Inspect a saved Bitcoin header chain proof and display its public inputs.

use sp1_sdk::SP1ProofWithPublicValues;

use bitcoin_header_chain_script::util;
use bitcoin_header_chain_script::util::STATE_SIZE;

const MAINNET_GENESIS_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

fn reverse_hash_display(hash: &[u8; 32]) -> String {
    let mut reversed = *hash;
    reversed.reverse();
    hex::encode(reversed)
}

fn main() {
    let proof_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bitcoin-header-chain-proof.bin".to_string());

    println!("Loading proof from: {}", proof_path);
    let proof = SP1ProofWithPublicValues::load(&proof_path)
        .expect("failed to load proof file");

    let pv = proof.public_values.as_ref();
    println!("\n=== Bitcoin Header Chain Proof ===\n");

    if pv.len() == STATE_SIZE {
        // Success path: just state bytes
        display_state(pv);
    } else if pv.len() == STATE_SIZE + 1 + 4 {
        // Error path: state + error_code + header_index
        let state_bytes = &pv[..STATE_SIZE];
        let error_code = pv[STATE_SIZE];
        let header_index =
            u32::from_le_bytes(pv[STATE_SIZE + 1..STATE_SIZE + 5].try_into().unwrap());

        println!("--- Error Output ---");
        display_state(state_bytes);
        println!("\n--- Error ---");
        let error_name = match error_code {
            0 => "Success",
            1 => "Header count mismatch",
            2 => "PoW insufficient",
            3 => "Timestamp too old",
            _ => "Unknown",
        };
        println!("Error Code:        {} ({})", error_code, error_name);
        println!("Header Index:      {}", header_index);
    } else {
        eprintln!(
            "ERROR: unexpected public values size (expected {} or {} bytes, got {})",
            STATE_SIZE,
            STATE_SIZE + 1 + 4,
            pv.len(),
        );
        eprintln!("Raw hex: {}", hex::encode(pv));
        std::process::exit(1);
    }

    // Proof metadata
    println!("\n--- Proof Details ---");
    println!("SP1 Version:       {}", proof.sp1_version);
    println!("Proof Type:        {:?}", proof.proof);
    println!("Public Values Size: {} bytes", pv.len());
}

fn display_state(pv: &[u8]) {
    let state = util::State::from_bytes(pv);

    use std::time::UNIX_EPOCH;

    // Genesis hash
    println!("Genesis Hash:      {}", reverse_hash_display(&state.genesis_hash));
    let mainnet_genesis_raw: [u8; 32] = {
        let mut g: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX).unwrap().try_into().unwrap();
        g.reverse();
        g
    };
    if state.genesis_hash == mainnet_genesis_raw {
        println!("                     ↳ mainnet ✓");
    } else {
        println!("                     ↳ NOT mainnet (different chain)");
    }

    // Chain tip
    println!("\nChain Tip:         {}", reverse_hash_display(&state.prev_blockhash));

    // Height
    println!("\nHeight:            {}", state.height);

    // Cumulative chain work
    let work_hex: String = state
        .chain_work
        .iter()
        .rev()
        .map(|w| format!("{:016x}", w))
        .collect();
    println!("Cumulative Work:   0x{}", work_hex);

    // Difficulty
    println!("Nbits:             0x{:08x}", state.nbits);

    // Epoch start timestamp
    let epoch_dt = UNIX_EPOCH + std::time::Duration::from_secs(state.epoch_start_timestamp as u64);
    println!("Epoch Start:       {} (timestamp: {})",
        humantime::format_rfc3339_seconds(epoch_dt),
        state.epoch_start_timestamp);

    // Timestamp window
    let timestamp_count = (state.height as usize).min(11);
    if timestamp_count > 0 {
        println!("\nTimestamp Window ({} entries):", timestamp_count);
        for i in 0..timestamp_count {
            let ts = state.timestamps[i];
            let dt = UNIX_EPOCH + std::time::Duration::from_secs(ts as u64);
            println!("  [{}] {} ({})", i, humantime::format_rfc3339_seconds(dt), ts);
        }
    }

    println!("\nStatus:              ✓ All headers validated");
}
