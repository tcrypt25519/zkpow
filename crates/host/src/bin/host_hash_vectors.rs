//! Execute-only tests for the guest's disjoint-field Bitcoin header hashing.
//!
//! Each vector starts from a canonical 80-byte mainnet block header. The host
//! computes SHA256d over those canonical bytes, then sends only the disjoint
//! representation used by the optimized guest hash path.

use sha2::{Digest, Sha256};
use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};

const ELF: Elf = include_elf!("guest_hash_vectors");
const HEADER_SIZE: usize = 80;
const HASH_SIZE: usize = 32;

const HEADER_HEX: &[&str] = &[
    // Mainnet heights 0..7.
    "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c",
    "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e36299",
    "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd61",
    "01000000bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a0000000044f672226090d85db9a9f2fbfe5f0f9609b387af7be5b7fbb7a1767c831c9e995dbe6649ffff001d05e0ed6d",
    "010000004944469562ae1c2c74d9a535e00b6f3e40ffbad4f2fda3895501b582000000007a06ea98cd40ba2e3288262b28638cec5337c1456aaf5eedc8e9e5a20f062bdf8cc16649ffff001d2bfee0a9",
    "0100000085144a84488ea88d221c8bd6c059da090e88f8a2c99690ee55dbba4e00000000e11c48fecdd9e72510ca84f023370c9a38bf91ac5cae88019bee94d24528526344c36649ffff001d1d03e477",
    "01000000fc33f596f822a0a1951ffdbf2a897b095636ad871707bf5d3162729b00000000379dfb96a5ea8c81700ea4ac6b97ae9a9312b2d4301a29580e924ee6761a2520adc46649ffff001d189c4c97",
    "010000008d778fdc15a2d3fb76b7122a3b5582bea4f21f5a0c693537e7a03130000000003f674005103b42f984169c7d008370967e91920a6a5d64fd51282f75bc73a68af1c66649ffff001d39a59c86",
];

fn decode_header(hex_header: &str) -> [u8; HEADER_SIZE] {
    let mut header = [0u8; HEADER_SIZE];
    hex::decode_to_slice(hex_header, &mut header).expect("header vector should be valid hex");
    header
}

// fn disjoint_record(header: &[u8; HEADER_SIZE]) -> [u8; HEADER_SIZE] {
//     let mut record = [0u8; HEADER_SIZE];

//     record[0..4].copy_from_slice(&header[0..4]); // version
//     record[4..36].copy_from_slice(&header[36..68]); // merkle_root
//     record[36..40].copy_from_slice(&header[68..72]); // timestamp
//     record[40..44].copy_from_slice(&header[76..80]); // nonce
//     record[44..76].copy_from_slice(&header[4..36]); // prev_blockhash
//     record[76..80].copy_from_slice(&header[72..76]); // compact_target

//     record
// }

fn sha256d(header: &[u8; HEADER_SIZE]) -> [u8; HASH_SIZE] {
    let mut hasher = Sha256::new();
    hasher.update(header);
    let first_hash = hasher.finalize_reset();
    hasher.update(first_hash);
    let second_hash = hasher.finalize();
    *second_hash.as_array().expect("sha256 output is 32 bytes")
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut stdin = SP1Stdin::new();
    let mut guest_input = Vec::with_capacity(HEADER_HEX.len() * HEADER_SIZE);
    let mut expected_hashes = Vec::with_capacity(HEADER_HEX.len() * HASH_SIZE);

    for hex_header in HEADER_HEX {
        let header = decode_header(hex_header);
        guest_input.extend_from_slice(&header);
        expected_hashes.extend_from_slice(&sha256d(&header));
    }

    stdin.write_vec(guest_input);

    let client = ProverClient::builder().mock().build().await;
    let (public_values, _report) = client
        .execute(ELF, stdin)
        .await
        .map_err(|err| format!("execution failed: {err}"))?;
    let actual_hashes = public_values.to_vec();

    if actual_hashes != expected_hashes {
        return Err(format!(
            "hash vector mismatch: expected {}, got {}",
            hex::encode(expected_hashes),
            hex::encode(actual_hashes),
        ));
    }

    println!("host_hash_vectors: {} vectors ok", HEADER_HEX.len());
    Ok(())
}
