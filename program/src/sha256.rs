//! SHA-256 via SP1 precompile syscalls.
//!
//! Two specialized functions for the exact sizes we need:
//! - `sha256_80`: hashes exactly 80 bytes (Bitcoin block header)
//! - `sha256_32`: hashes exactly 32 bytes (intermediate hash output)
//!
//! No loops, no branching on block count — the padding and block layout
//! are hardcoded for each size.

use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};

/// SHA-256 IV constants.
const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Extract a 32-byte hash from the SHA-256 state.
/// The state stores 8 × u64; each contributes its low 32 bits in big-endian order.
fn state_to_hash(state: &[u64; 8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    for (i, &val) in state.iter().enumerate() {
        let bytes = val.to_be_bytes();
        hash[i * 4..(i + 1) * 4].copy_from_slice(&bytes[4..8]);
    }
    hash
}

/// Compute SHA-256 of exactly 80 bytes.
///
/// Produces two blocks:
/// - Block 1: bytes 0–63 of the input (all data)
/// - Block 2: bytes 64–79 (16 data bytes) + 0x80 + 47 zeros + 8-byte length (640 bits)
pub fn sha256_80(data: &[u8; 80]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    for (j, chunk) in data[0..64].chunks(4).enumerate() {
        w[j] = u32::from_be_bytes(chunk.try_into().unwrap()) as u64;
    }
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–79 + padding + length ──
    // Layout: [data 16 bytes | 0x80 | 47× 0x00 | 8-byte big-endian length 640 = 0x280]
    let mut w = [0u64; 64];
    for (j, chunk) in data[64..80].chunks(4).enumerate() {
        w[j] = u32::from_be_bytes(chunk.try_into().unwrap()) as u64;
    }
    // 0x80 byte at position 64+16=80, which is word index 4, byte 0
    // w[4] = 0x80000000 (0x80 << 24)
    w[4] = 0x80000000;
    // w[14] = upper 32 bits of length = 0
    // w[15] = lower 32 bits of length = 640 = 0x280
    w[15] = 0x280;

    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 32 bytes.
///
/// Produces one block:
/// bytes 0–31 (all data) + 0x80 + 23 zeros + 8-byte length (256 bits)
pub fn sha256_32(data: &[u8; 32]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Single block: 32 bytes + padding + length ──
    let mut w = [0u64; 64];
    for (j, chunk) in data.chunks(4).enumerate() {
        w[j] = u32::from_be_bytes(chunk.try_into().unwrap()) as u64;
    }
    // 0x80 at byte 32 → word index 8, byte 0 → 0x80000000
    w[8] = 0x80000000;
    // w[14] = 0, w[15] = length in bits = 256 = 0x100
    w[15] = 0x100;

    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute double SHA-256 of exactly 80 bytes: SHA-256(SHA-256(data)).
pub fn double_sha256_80(data: &[u8; 80]) -> [u8; 32] {
    let inner = sha256_80(data);
    sha256_32(&inner)
}
