//! Inspect a saved zkpow proof and display its public inputs.

use crate::util::{HeaderChainPublicValues, ValidationErrorCode};
use hex::encode;
use sp1_sdk::SP1ProofWithPublicValues;
use zkpow_host::util;

const MAINNET_GENESIS_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

fn reverse_hash_display(hash: util::BlockHash) -> String {
    let mut hash_be = hash.into_raw();
    hash_be.reverse();
    encode(hash_be)
}

fn main() {
    let proof_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "zkpow-proof.bin".to_string());

    println!("Loading proof from: {}", proof_path);
    let proof = SP1ProofWithPublicValues::load(&proof_path).expect("failed to load proof file");

    let pv = proof.public_values.as_ref();
    println!("\n=== zkpow Proof ===\n");

    match HeaderChainPublicValues::parse(pv) {
        Ok(HeaderChainPublicValues::Success {
            claim,
            continuation_digest,
        }) => {
            display_claim(&claim);
            println!("\n--- Continuation ---");
            println!("Continuation Digest: {}", hex::encode(continuation_digest));
            println!("\nStatus:              ✓ All headers validated");
        }
        Ok(HeaderChainPublicValues::Failure {
            failure,
            last_valid_claim,
            continuation_digest,
        }) => {
            println!("--- Last Valid State ---");
            display_claim(&last_valid_claim);
            println!("\n--- Error ---");
            let error_name = match failure.error_code {
                ValidationErrorCode::HeaderPayloadLengthInvalid => "Header payload length invalid",
                ValidationErrorCode::PowInsufficient => "PoW insufficient",
                ValidationErrorCode::TimestampTooOld => "Timestamp too old",
                ValidationErrorCode::GenesisHashMismatch => "Genesis hash mismatch",
            };
            println!(
                "Error Code:        {} ({})",
                failure.error_code as u8, error_name,
            );
            println!("Failure Height:    {}", failure.failure_height);
            println!("\n--- Continuation ---");
            println!("Continuation Digest: {}", hex::encode(continuation_digest));
        }
        Err(parse_error) => {
            eprintln!("ERROR: {}", parse_error);
            eprintln!("Raw hex: {}", hex::encode(pv));
            std::process::exit(1);
        }
    }

    // Proof metadata
    println!("\n--- Proof Details ---");
    println!("SP1 Version:       {}", proof.sp1_version);
    println!("Proof Type:        {:?}", proof.proof);
    println!("Public Values Size: {} bytes", pv.len());
}

fn display_claim(claim: &util::PublicChainClaim) {
    let mainnet_genesis_raw: [u8; 32] = {
        let mut g: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX)
            .unwrap()
            .try_into()
            .unwrap();
        g.reverse();
        g
    };

    println!(
        "Genesis Hash:      {}",
        reverse_hash_display(claim.genesis_hash)
    );
    if claim.genesis_hash == util::BlockHash::from_raw(mainnet_genesis_raw) {
        println!("                     ↳ mainnet ✓");
    } else {
        println!("                     ↳ NOT mainnet (different chain)");
    }

    println!(
        "\nChain Tip:         {}",
        reverse_hash_display(claim.tip_hash)
    );

    println!("\nHeight:            {}", claim.height);

    let work_hex: String = claim
        .chain_work
        .as_limbs()
        .iter()
        .rev()
        .map(|w| format!("{:016x}", w))
        .collect();
    println!("Cumulative Work:   0x{}", work_hex);
}
