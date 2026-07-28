//! Execute-only test guest for Bitcoin header hashing vectors.
//!
//! Input: repeated 80-byte disjoint records:
//!   new_header.version      4 bytes LE
//!   new_header.merkle_root 32 bytes
//!   new_header.timestamp    4 bytes LE
//!   new_header.nonce        4 bytes LE
//!   prev_blockhash         32 bytes
//!   compact_target          4 bytes LE
//!
//! Output: repeated 32-byte SHA256d header hashes.

#![no_main]
sp1_zkvm::entrypoint!(main);

use zkpow_sha256;

const RECORD_SIZE: usize = 80;

fn u32_from_le(bytes: &[u8]) -> u32 {
    let four_bytes = bytes.as_array::<4>().expect("correct length");
    u32::from_le_bytes(*four_bytes)
}

fn hash_bytes_from_le(bytes: &[u8]) -> &[u8; 32] {
    let thirty_two_bytes = bytes.as_array::<32>().expect("correct length");
    thirty_two_bytes
}

pub fn main() {
    let input = sp1_zkvm::io::read_vec();
    assert!(
        input.len().is_multiple_of(RECORD_SIZE),
        "hash-vector input length must be a multiple of 80 bytes",
    );

    for record in input.chunks_exact(RECORD_SIZE) {
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(&record[4..36]);

        // let header = HeaderInput {
        //     version: u32_from_le(&record[0..4]),
        //     merkle_root,
        //     timestamp: BlockTimestamp::new(u32_from_le(&record[36..40])),
        //     nonce: u32_from_le(&record[40..44]),
        // };

        let mut prev_blockhash = [0u8; 32];
        prev_blockhash.copy_from_slice(&record[44..76]);

        let hash = zkpow_sha256::of_header(
            u32_from_le(&record[0..4]),
            *hash_bytes_from_le(&record[44..76]),
            *hash_bytes_from_le(&record[4..36]),
            u32_from_le(&record[36..40]),
            u32_from_le(&record[76..80]),
            u32_from_le(&record[40..44]),
        );
        sp1_zkvm::io::commit_slice(&hash);
    }
}
