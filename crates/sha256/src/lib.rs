//! SHA-256 helpers for zkpow host and guest code.
#![no_std]
#![allow(clippy::needless_range_loop)]
//!
//! Specialized guest functions for the exact sizes we need:
//! - `sha256_32bytes`: hashes exactly 32 bytes (intermediate hash output)
//! - `of_minimal_public_values`: hashes minimal public values from their fields
//! - `sha256_52bytes`: hashes exactly 52 bytes (continuation data)
//! - `of_header`: hashes a full Bitcoin header from disjoint fields
//!
//! No loops, no branching on block count. The padding and block layout
//! are hardcoded for each size.
use core::mem::MaybeUninit;

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

/// Double-SHA256 (`SHA-256(SHA-256(data))`), computed on the host via the `sha2` crate.
#[cfg(feature = "host")]
#[must_use]
pub fn double(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let first: [u8; 32] = Sha256::digest(data).into();
    Sha256::digest(first).into()
}

#[inline]
const fn word_from_le_u32(value: u32) -> u64 {
    value.swap_bytes() as u64
}

#[inline]
const fn word_from_bytes(bytes: &[u8], word_index: usize) -> u64 {
    let i = word_index * 4;
    ((bytes[i] as u64) << 24) | ((bytes[i + 1] as u64) << 16) | ((bytes[i + 2] as u64) << 8) | (bytes[i + 3] as u64)
}

/// Run one SHA-256 block: extend the 16 message words into the 64-word
/// schedule and compress it into `state`.
///
/// Only the 16 low words carry data; the message schedule fills w[16..64],
/// so the 48-word tail is never zero-initialized — the single largest source
/// of dead work in the per-header hash path.
#[inline]
fn of_block(low16: &[u64; 16], state: &mut [u64; 8]) {
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

/// Second pass of a SHA-256d: the first pass leaves its digest in `prev_state`
/// as 8 words (each a 32-bit value in its low bits). Those are exactly the
/// first 8 message words of the second hash, so we load them straight in
/// instead of serializing to a `[u8; 32]` and parsing it back — the byte
/// buffer would carry the identical information in a different shape.
///
/// Single block: 32 bytes of digest + 0x80 + zeros + 8-byte length (256 bits).
fn of_digest(prev_state: &[u64; 8]) -> [u8; 32] {
    let mut state = SHA256_IV;

    let mut w = [0u64; 16];
    for i in 0..8 {
        // Keep the low 32 bits, exactly what the byte round-trip extracted.
        w[i] = prev_state[i] & 0xFFFF_FFFF;
    }
    // End of data; "1" bit then "0" bits, and the length (256 bits = 0x100).
    w[8] = 0x80000000;
    w[15] = 0x100;

    of_block(&w, &mut state);
    state_to_hash(&state)
}

/// Compute SHA-256 of minimal public values from their individual fields.
///
/// Equivalent to serializing the fields to 165 bytes and hashing them, but avoids
/// materializing the intermediate byte array. The field order matches
/// [`MinimalPublicValues::to_bytes`]:
/// - Block 1: chain_work (32) + verifier_key (32)
/// - Block 2: continuation_digest (32) + genesis_hash (32)
/// - Block 3: tip_hash (32) + height (4) + error_code (1) + padding + length
pub fn of_minimal_public_values(
    chain_work: &[u8; 32],
    verifier_key: &[u8; 32],
    continuation_digest: &[u8; 32],
    genesis_hash: &[u8; 32],
    tip_hash: &[u8; 32],
    height: u32,
    error_code: u8,
) -> [u8; 32] {
    let mut state = SHA256_IV;

    // Block 1: chain_work + verifier_key
    let mut w = [0u64; 16];
    for i in 0..8 {
        w[i] = be_u64_from_slice(chain_work, i);
    }
    for i in 0..8 {
        w[8 + i] = be_u64_from_slice(verifier_key, i);
    }
    of_block(&w, &mut state);

    // Block 2: continuation_digest + genesis_hash
    let mut w = [0u64; 16];
    for i in 0..8 {
        w[i] = be_u64_from_slice(continuation_digest, i);
    }
    for i in 0..8 {
        w[8 + i] = be_u64_from_slice(genesis_hash, i);
    }
    of_block(&w, &mut state);

    // Block 3: tip_hash + height + error_code + padding + length
    let mut w = [0u64; 16];
    for i in 0..8 {
        w[i] = be_u64_from_slice(tip_hash, i);
    }
    let height_bytes = height.to_le_bytes();
    w[8] = be_u64_from_slice(&height_bytes, 0);
    w[9] = ((error_code as u64) << 24) | 0x0080_0000;
    w[15] = 0x528;
    of_block(&w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 52 bytes.
///
/// Produces one block:
/// bytes 0-51 (all data) + 0x80 + 3 zeros + 8-byte length (416 bits = 0x1A0)
pub fn of_52bytes(data: &[u8; 52]) -> [u8; 32] {
    let mut state = SHA256_IV;

    let mut w = [0u64; 16];
    for i in 0..13 {
        w[i] = be_u64_from_slice(data, i);
    }
    w[13] = 0x80000000;
    w[15] = 0x1A0;
    of_block(&w, &mut state);

    state_to_hash(&state)
}

//
// Header helpers
//

/// Build the first SHA-256 block words for a Bitcoin header represented by
/// disjoint prover/header-state fields.
#[inline]
fn header_first_block(version: u32, prev_blockhash: [u8; 32], merkle_root: [u8; 32]) -> [u64; 16] {
    // Only the 16 low message words: the SHA-256 message schedule derives
    // w[16..64] from these, so the caller never materializes the tail.
    let mut words = [0u64; 16];
    words[0] = word_from_le_u32(version);
    for i in 0..8 {
        words[1 + i] = word_from_bytes(&prev_blockhash, i);
    }
    for i in 0..7 {
        words[9 + i] = word_from_bytes(&merkle_root, i);
    }
    words
}

/// Build the second SHA-256 block words for a Bitcoin header represented by
/// disjoint prover/header-state fields.
#[inline]
fn header_second_block(merkle_root: [u8; 32], timestamp: u32, compact_target: u32, nonce: u32) -> [u64; 16] {
    // Live words only (data + padding + length); w[5..15] stay zero as SHA
    // padding, w[16..64] are filled by the message schedule.
    let mut words = [0u64; 16];
    words[0] = word_from_bytes(&merkle_root, 7);
    words[1] = word_from_le_u32(timestamp);
    words[2] = word_from_le_u32(compact_target);
    words[3] = word_from_le_u32(nonce);
    words[4] = 0x80000000;
    words[15] = 0x280;
    words
}

/// Compute SHA256d over a Bitcoin header represented by disjoint fields.
///
/// This is byte-for-byte equivalent to hashing:
/// version || prev_blockhash || merkle_root || timestamp || compact_target || nonce,
/// but avoids materializing either a full [`zkpow_core::Header`] or `[u8; 80]`.
pub fn of_header(
    version: u32,
    prev_blockhash: [u8; 32],
    merkle_root: [u8; 32],
    timestamp: u32,
    compact_target: u32,
    nonce: u32,
) -> [u8; 32] {
    let mut state = SHA256_IV;

    let block1 = header_first_block(version, prev_blockhash, merkle_root);
    of_block(&block1, &mut state);

    let block2 = header_second_block(merkle_root, timestamp, compact_target, nonce);
    of_block(&block2, &mut state);

    of_digest(&state)
}
