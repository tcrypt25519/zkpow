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

#[allow(dead_code)]
#[path = "../sha256.rs"]
mod sha256;

use sha256::sha256d_new_header;
use zkpow_core::{BlockHash, BlockTimestamp, CompactTarget, HeaderInput};

const RECORD_SIZE: usize = 80;

fn u32_from_le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("slice should be four bytes"))
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

        let header = HeaderInput {
            version: u32_from_le(&record[0..4]),
            merkle_root,
            timestamp: BlockTimestamp::new(u32_from_le(&record[36..40])),
            nonce: u32_from_le(&record[40..44]),
        };

        let mut prev_blockhash = [0u8; 32];
        prev_blockhash.copy_from_slice(&record[44..76]);
        let compact_target = CompactTarget::new(u32_from_le(&record[76..80]));

        let hash = sha256d_new_header(&header, &BlockHash::new(prev_blockhash), compact_target);
        sp1_zkvm::io::commit_slice(&hash);
    }
}
