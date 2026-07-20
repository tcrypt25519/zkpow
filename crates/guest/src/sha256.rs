//! SHA-256 via SP1 precompile syscalls.
#![allow(clippy::needless_range_loop)]
//!
//! Specialized functions for the exact sizes we need:
//! - `sha256_32bytes`: hashes exactly 32 bytes (intermediate hash output)
//! - `sha256_169bytes`: hashes exactly 169 bytes (minimal public values)
//! - `sha256_52bytes`: hashes exactly 52 bytes (continuation data)
//! - `sha256d_new_header`: hashes a full Bitcoin header from disjoint fields
//!
//! No loops, no branching on block count — the padding and block layout
//! are hardcoded for each size.

use core::mem::MaybeUninit;

use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};
use zkpow_core::{
    header_first_sha256_block_words, header_second_sha256_block_words, BlockHash, CompactTarget,
    HeaderInput,
};

const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Run one SHA-256 block: extend the 16 message words into the 64-word
/// schedule and compress it into `state`.
///
/// Only the 16 low words carry data; the message schedule fills w[16..64],
/// so the 48-word tail is never zero-initialized — the single largest source
/// of dead work in the per-header hash path.
#[inline]
fn sha256_block(low16: &[u64; 16], state: &mut [u64; 8]) {
    let mut w = MaybeUninit::<[u64; 64]>::uninit();
    let w = w.as_mut_ptr();
    // SAFETY: `copy_nonoverlapping` initializes w[0..16]. `syscall_sha256_extend`
    // writes every word of w[16..64] before it is read (each W[t] is derived
    // from strictly-lower indices), so the whole buffer is initialized by the
    // time `syscall_sha256_compress` reads it. Both syscalls take raw pointers,
    // so no reference to the uninitialized tail is ever formed.
    unsafe {
        core::ptr::copy_nonoverlapping(low16.as_ptr(), w.cast::<u64>(), 16);
        syscall_sha256_extend(w);
        syscall_sha256_compress(w, state);
    }
}

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

/// Second pass of a SHA-256d: the first pass leaves its digest in `prev_state`
/// as 8 words (each a 32-bit value in its low bits). Those are exactly the
/// first 8 message words of the second hash, so we load them straight in
/// instead of serializing to a `[u8; 32]` and parsing it back — the byte
/// buffer would carry the identical information in a different shape.
///
/// Single block: 32 bytes of digest + 0x80 + zeros + 8-byte length (256 bits).
fn sha256_of_prev_digest(prev_state: &[u64; 8]) -> [u8; 32] {
    let mut state = SHA256_IV;

    let mut w = [0u64; 16];
    for i in 0..8 {
        // Keep the low 32 bits, exactly what the byte round-trip extracted.
        w[i] = prev_state[i] & 0xFFFF_FFFF;
    }
    // End of data; “1” bit then “0” bits, and the length (256 bits = 0x100).
    w[8] = 0x80000000;
    w[15] = 0x100;

    sha256_block(&w, &mut state);
    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 169 bytes.
///
/// Produces three blocks:
/// - Block 1: bytes 0–63
/// - Block 2: bytes 64–127
/// - Block 3: bytes 128–168 (41 data bytes) + 0x80 + zeros + 8-byte length (1352 bits = 0x548)
#[allow(dead_code)]
pub(crate) fn sha256_169bytes(data: &[u8; 169]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 16];
    for i in 0..16 {
        w[i] = be_u64_from_slice(data, i);
    }
    sha256_block(&w, &mut state);

    // ── Block 2: bytes 64–127 ──
    let mut w = [0u64; 16];
    for i in 0..16 {
        w[i] = be_u64_from_slice(data, 16 + i);
    }
    sha256_block(&w, &mut state);

    // ── Block 3: bytes 128–168 (41 bytes) + padding + length ──
    // 41 data bytes, then 0x80, then zeros, then 8-byte length (1352 bits = 0x548)
    let mut w = [0u64; 16];
    for i in 0..10 {
        w[i] = be_u64_from_slice(data, 32 + i);
    }
    // data[168] in high byte, then 0x80 in next byte
    w[10] = ((data[168] as u64) << 24) | 0x0080_0000;
    // w[15] = length in bits = 169 * 8 = 1352 = 0x548
    w[15] = 0x548;
    sha256_block(&w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA256d over a Bitcoin header represented by disjoint fields.
///
/// This is byte-for-byte equivalent to hashing:
/// version || prev_blockhash || merkle_root || timestamp || compact_target || nonce,
/// but avoids materializing either a full [`zkpow_core::Header`] or `[u8; 80]`.
pub(crate) fn sha256d_new_header(
    header: &HeaderInput,
    prev_blockhash: &BlockHash,
    compact_target: CompactTarget,
) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: version + prev_blockhash + merkle_root[0..28] ──
    let block1 = header_first_sha256_block_words(header, prev_blockhash);
    sha256_block(&block1, &mut state);

    // ── Block 2: merkle_root[28..32] + timestamp + nBits + nonce + padding ──
    let block2 = header_second_sha256_block_words(header, compact_target);
    sha256_block(&block2, &mut state);

    // Second SHA-256d pass straight from the digest words — no [u8; 32]
    // serialize/reparse in between.
    sha256_of_prev_digest(&state)
}

/// Compute SHA-256 of exactly 52 bytes.
///
/// Produces one block:
/// bytes 0–51 (all data) + 0x80 + 3 zeros + 8-byte length (416 bits = 0x1A0)
pub(crate) fn sha256_52bytes(data: &[u8; 52]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Single block: bytes 0–51 + padding + length ──
    let mut w = [0u64; 16];
    for i in 0..13 {
        w[i] = be_u64_from_slice(data, i);
    }
    // 0x80 at word index 13, byte 0 → 0x80000000
    w[13] = 0x80000000;
    // w[15] = length in bits = 52 * 8 = 416 = 0x1A0
    w[15] = 0x1A0;
    sha256_block(&w, &mut state);

    state_to_hash(&state)
}
