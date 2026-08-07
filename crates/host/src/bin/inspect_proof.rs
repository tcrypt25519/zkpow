//! Inspect a saved zkpow proof and display its public inputs.

use hex::encode;
use sp1_sdk::SP1ProofWithPublicValues;
use zkpow_host::util;

use crate::util::MinimalPublicValues;

const MAINNET_GENESIS_HEX: &str = "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

fn reverse_hash_display(hash: util::BlockHash) -> String {
    let mut hash_be = hash.into_inner();
    hash_be.reverse();
    encode(hash_be)
}

fn main() {
    let proof_path = std::env::args().nth(1).unwrap_or_else(|| "zkpow-proof.bin".to_string());

    println!("Loading proof from: {}", proof_path);
    let proof = SP1ProofWithPublicValues::load(&proof_path).expect("failed to load proof file");

    let pv = proof.public_values.as_ref();
    println!("\n=== zkpow Proof ===\n");

    match MinimalPublicValues::parse(pv) {
        Ok(pv) if pv.is_success() => {
            let claim = pv.claim();
            display_claim(&claim);
            display_verifier_key(pv.verifier_key);
            println!("\n--- Continuation ---");
            println!("Continuation Digest: {}", hex::encode(pv.continuation_digest));
            println!("\nStatus:              ✓ All headers validated");
        },
        Ok(pv) => {
            let last_valid_claim = pv.claim();
            let error_code = pv.validation_error_code().expect("failure has an error code");
            println!("--- Last Valid State ---");
            display_claim(&last_valid_claim);
            display_verifier_key(pv.verifier_key);
            println!("\n--- Error ---");
            println!("Error Code:        {} ({})", error_code.get(), error_code);
            println!("Failure Height:    {}", last_valid_claim.height + 1);
            println!("\n--- Continuation ---");
            println!("Continuation Digest: {}", hex::encode(pv.continuation_digest));
        },
        Err(parse_error) => {
            eprintln!("ERROR: {}", parse_error);
            eprintln!("Raw hex: {}", hex::encode(pv));
            std::process::exit(1);
        },
    }

    // Proof metadata
    println!("\n--- Proof Details ---");
    println!("SP1 Version:       {}", proof.sp1_version);
    println!("Proof Type:        {:?}", proof.proof);
    println!("Public Values Size: {} bytes", pv.len());
}

fn display_verifier_key(verifier_key: util::VerifierKeyDigest) {
    println!("Verifier Key:     {}", hex::encode(verifier_key.to_bytes()));
}

fn display_claim(claim: &util::Claim) {
    let mainnet_genesis_raw: [u8; 32] = {
        let mut g: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX).unwrap().try_into().unwrap();
        g.reverse();
        g
    };

    println!("Genesis Hash:      {}", reverse_hash_display(claim.genesis_hash));
    if claim.genesis_hash == util::BlockHash::new(mainnet_genesis_raw) {
        println!("                     ↳ mainnet ✓");
    } else {
        println!("                     ↳ NOT mainnet (different chain)");
    }

    println!("\nChain Tip:         {}", reverse_hash_display(claim.tip_hash));

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
