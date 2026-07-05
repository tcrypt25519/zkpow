//! SHA-256 via SP1 precompile syscalls.
#![allow(clippy::needless_range_loop)]
//!
//! Specialized functions for the exact sizes we need:
//! - `sha256_80bytes`: hashes exactly 80 bytes (Bitcoin block header)
//! - `sha256_32bytes`: hashes exactly 32 bytes (intermediate hash output)
//! - `sha256_169bytes`: hashes exactly 169 bytes (minimal public values)
//! - `sha256_116bytes`: hashes exactly 116 bytes (continuation data)
//! - `sha256d_80bytes`: hashes exactly 80 bytes with SHA256d
//!
//! No loops, no branching on block count — the padding and block layout
//! are hardcoded for each size.

use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};

const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Extract a 32-byte hash from the SHA-256 state.
/// The state stores 8 × u64; each contributes its low 32 bits in big-endian order.
#[inline]
fn state_to_hash(state: &[u64; 8]) -> [u8; 32] {
    let b0 = state[0].to_be_bytes();
    let b1 = state[1].to_be_bytes();
    let b2 = state[2].to_be_bytes();
    let b3 = state[3].to_be_bytes();
    let b4 = state[4].to_be_bytes();
    let b5 = state[5].to_be_bytes();
    let b6 = state[6].to_be_bytes();
    let b7 = state[7].to_be_bytes();
    [
        b0[4], b0[5], b0[6], b0[7], //
        b1[4], b1[5], b1[6], b1[7], //
        b2[4], b2[5], b2[6], b2[7], //
        b3[4], b3[5], b3[6], b3[7], //
        b4[4], b4[5], b4[6], b4[7], //
        b5[4], b5[5], b5[6], b5[7], //
        b6[4], b6[5], b6[6], b6[7], //
        b7[4], b7[5], b7[6], b7[7], //
    ]
}

#[inline]
const fn be_u64_from_slice(s: &[u8], i: usize) -> u64 {
    let i = i * 4;
    ((s[i] as u64) << 24) | ((s[i + 1] as u64) << 16) | ((s[i + 2] as u64) << 8) | (s[i + 3] as u64)
}

/// Compute SHA-256 of exactly 80 bytes.
///
/// Produces two blocks:
/// - Block 1: bytes 0–63 of the input (all data)
/// - Block 2: bytes 64–79 (16 data bytes) + 0x80 + 47 zeros + 8-byte length (640 bits)
pub fn sha256_80bytes(data: &[u8; 80]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    for i in 0..16 {
        w[i] = be_u64_from_slice(data, i);
    }
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–79 + padding + length ──
    // Layout: [data 16 bytes | 0x80 | 47× 0x00 | 8-byte big-endian length 640 = 0x280]
    let mut w = [0u64; 64];
    for i in 0..4 {
        w[i] = be_u64_from_slice(data, 16 + i);
    }
    // 0x80 byte at word index 4, byte 0 → 0x80000000
    w[4] = 0x80000000;
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
pub fn sha256_32bytes(data: &[u8; 32]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Single block: 32 bytes + padding + length ──
    let mut w = [0u64; 64];
    for i in 0..8 {
        w[i] = be_u64_from_slice(data, i);
    }
    // End of data; add “1” bit followed by “0” bits
    w[8] = 0x80000000;
    // Add length of data in bits; 256 == 0x100
    w[15] = 0x100;

    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);
    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 169 bytes.
///
/// Produces three blocks:
/// - Block 1: bytes 0–63
/// - Block 2: bytes 64–127
/// - Block 3: bytes 128–168 (41 data bytes) + 0x80 + zeros + 8-byte length (1352 bits = 0x548)
pub(crate) fn sha256_169bytes(data: &[u8; 169]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    for i in 0..16 {
        w[i] = be_u64_from_slice(data, i);
    }
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–127 ──
    let mut w = [0u64; 64];
    for i in 0..16 {
        w[i] = be_u64_from_slice(data, 16 + i);
    }
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 3: bytes 128–168 (41 bytes) + padding + length ──
    // 41 data bytes, then 0x80, then zeros, then 8-byte length (1352 bits = 0x548)
    let mut w = [0u64; 64];
    for i in 0..10 {
        w[i] = be_u64_from_slice(data, 32 + i);
    }
    // data[168] in high byte, then 0x80 in next byte
    w[10] = ((data[168] as u64) << 24) | 0x0080_0000;
    // w[15] = length in bits = 169 * 8 = 1352 = 0x548
    w[15] = 0x548;
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA256d of exactly 80 bytes: SHA-256(SHA-256(data)).
pub(crate) fn sha256d_80bytes(data: &[u8; 80]) -> [u8; 32] {
    sha256_32bytes(&sha256_80bytes(data))
}

/// Compute SHA-256 of exactly 116 bytes.
///
/// Produces two blocks:
/// - Block 1: bytes 0–63
/// - Block 2: bytes 64–115 (52 data bytes) + 0x80 + zeros + 8-byte length (928 bits = 0x3A0)
pub(crate) fn sha256_116bytes(data: &[u8; 116]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    for i in 0..16 {
        w[i] = be_u64_from_slice(data, i);
    }
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–115 (52 bytes) + padding + length ──
    let mut w = [0u64; 64];
    for i in 0..13 {
        w[i] = be_u64_from_slice(data, 16 + i);
    }
    // 0x80 at word index 13, byte 0 → 0x80000000
    w[13] = 0x80000000;
    // w[15] = length in bits = 116 * 8 = 928 = 0x3A0
    w[15] = 0x3A0;
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}
