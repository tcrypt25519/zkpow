//! SHA-256 helpers for zkpow host and guest code.
#![no_std]
#![allow(clippy::needless_range_loop)]
#![cfg_attr(all(test, target_os = "zkvm"), feature(custom_test_frameworks))]
#![cfg_attr(all(test, target_os = "zkvm"), test_runner(zkvm_test::test_runner))]
//!
//! Specialized guest functions for the exact sizes we need:
//! - `of_public_values`: hashes the complete 185 byte public values
//! - `of_header`: hashes a full Bitcoin header from disjoint fields
//!
//! No loops, no branching on block count. The padding and block layout
//! are hardcoded for each size.
use core::mem::MaybeUninit;

#[cfg(target_os = "zkvm")]
use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};

const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Extract a 32-byte hash from the SHA-256 state.
/// The state stores 8 × u64; each contributes its low 32 bits in big-endian order.
#[inline]
pub fn state_to_hash(state: &[u64; 8]) -> [u8; 32] {
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

/// Pack a 32-byte hash into 8 SHA state words.
/// Each word holds a big-endian 32-bit word in its low 32 bits.
#[inline]
pub const fn hash_to_state(hash: &[u8; 32]) -> [u64; 8] {
    [
        word_from_bytes(hash, 0),
        word_from_bytes(hash, 1),
        word_from_bytes(hash, 2),
        word_from_bytes(hash, 3),
        word_from_bytes(hash, 4),
        word_from_bytes(hash, 5),
        word_from_bytes(hash, 6),
        word_from_bytes(hash, 7),
    ]
}

#[inline]
const fn be_u64_from_slice(s: &[u8], i: usize) -> u64 {
    let i = i * 4;
    ((s[i] as u64) << 24) | ((s[i + 1] as u64) << 16) | ((s[i + 2] as u64) << 8) | (s[i + 3] as u64)
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

/// Extend the 16 message words in `w[0..16]` into the full 64-word schedule
/// and compress it into `state`.
#[cfg(target_os = "zkvm")]
#[inline(always)]
unsafe fn compress_schedule(w: *mut u64, state: &mut [u64; 8]) {
    syscall_sha256_extend(w.cast());
    syscall_sha256_compress(w.cast(), state);
}

/// Host fallback using the `sha2` crate's compression function.
#[cfg(not(target_os = "zkvm"))]
#[allow(deprecated)]
#[inline(always)]
unsafe fn compress_schedule(w: *const u64, state: &mut [u64; 8]) {
    use sha2::{
        compress256,
        digest::generic_array::{GenericArray, typenum::U64},
    };

    let mut block = [0u8; 64];
    for i in 0..16 {
        let word = unsafe { *w.add(i) };
        block[i * 4] = ((word >> 24) & 0xff) as u8;
        block[i * 4 + 1] = ((word >> 16) & 0xff) as u8;
        block[i * 4 + 2] = ((word >> 8) & 0xff) as u8;
        block[i * 4 + 3] = (word & 0xff) as u8;
    }

    let mut state_u32 = [0u32; 8];
    for i in 0..8 {
        state_u32[i] = state[i] as u32;
    }
    let block = GenericArray::<u8, U64>::clone_from_slice(&block);
    compress256(&mut state_u32, &[block]);
    for i in 0..8 {
        state[i] = state_u32[i] as u64;
    }
}

/// Run one arbitrary SHA-256 block: copy 16 message words into the schedule
/// buffer, extend into the 64-word schedule, and compress into `state`.
#[inline(always)]
fn of_block(low16: &[u64; 16], state: &mut [u64; 8]) {
    let mut w = MaybeUninit::<[u64; 64]>::uninit();
    let ptr = w.as_mut_ptr().cast::<u64>();
    // SAFETY: copy_nonoverlapping initializes w[0..16], compress_schedule derives
    // w[16..64] and compresses into state.
    unsafe {
        core::ptr::copy_nonoverlapping(low16.as_ptr(), ptr, 16);
        compress_schedule(ptr, state);
    }
}

/// Second pass of a SHA-256d: the first pass leaves its digest in `prev_state`
/// as 8 words (each a 32-bit value in its low bits). Those are exactly the
/// first 8 message words of the second hash, built directly into the 64-word buffer.
///
/// Single block: 32 bytes of digest + 0x80 + zeros + 8-byte length (256 bits).
#[inline(always)]
pub fn of_digest_words(prev_state: &[u64; 8]) -> [u64; 8] {
    let mut state = SHA256_IV;
    let mut w = MaybeUninit::<[u64; 64]>::uninit();
    let ptr = w.as_mut_ptr().cast::<u64>();
    // SAFETY: words 0..16 are all written directly into w before compression.
    unsafe {
        for i in 0..8 {
            ptr.add(i).write(prev_state[i] & 0xFFFF_FFFF);
        }
        ptr.add(8).write(0x80000000);
        for i in 9..15 {
            ptr.add(i).write(0);
        }
        ptr.add(15).write(0x100);
        compress_schedule(ptr, &mut state);
    }
    state
}

/// [`of_digest_words`] unpacked to bytes.
pub fn of_digest(prev_state: &[u64; 8]) -> [u8; 32] {
    let state = of_digest_words(prev_state);
    state_to_hash(&state)
}

/// Compute SHA-256 of the complete 185 byte public values.
///
/// Layout matches `zkpow_core::PublicValues::to_bytes`:
/// - Block 1: state[0..64]
/// - Block 2: state[64..128]
/// - Block 3: state[128..152] || verifier_key (32) || return_code (1) + padding
/// - Block 4: length (1480 bits = 0x5C8)
pub fn of_public_values(state_bytes: &[u8], verifier_key: &[u8], return_code: u8) -> [u8; 32] {
    let mut state = SHA256_IV;

    let mut w = [0u64; 16];
    for i in 0..16 {
        w[i] = be_u64_from_slice(state_bytes, i);
    }
    of_block(&w, &mut state);

    let mut w = [0u64; 16];
    for i in 0..16 {
        w[i] = be_u64_from_slice(state_bytes, 16 + i);
    }
    of_block(&w, &mut state);

    let mut w = [0u64; 16];
    for i in 0..6 {
        w[i] = be_u64_from_slice(state_bytes, 32 + i);
    }
    for i in 0..8 {
        w[6 + i] = be_u64_from_slice(verifier_key, i);
    }
    w[14] = ((return_code as u64) << 24) | 0x0080_0000;
    of_block(&w, &mut state);

    let mut w = [0u64; 16];
    w[15] = 0x5C8;
    of_block(&w, &mut state);

    state_to_hash(&state)
}

/// Build the first SHA-256 block words for a Bitcoin header directly in the schedule buffer
/// with previous blockhash in state words.
#[inline(always)]
fn header_first_block_words(version: u32, prev_blockhash: &[u64; 8], merkle_root: &[u8; 32], state: &mut [u64; 8]) {
    let mut w = MaybeUninit::<[u64; 64]>::uninit();
    let ptr = w.as_mut_ptr().cast::<u64>();
    // SAFETY: words 0..16 are all written directly into w before compression.
    unsafe {
        ptr.add(0).write(word_from_le_u32(version));
        for i in 0..8 {
            ptr.add(1 + i).write(prev_blockhash[i]);
        }
        for i in 0..7 {
            ptr.add(9 + i).write(word_from_bytes(merkle_root, i));
        }
        compress_schedule(ptr, state);
    }
}

/// Build the first SHA-256 block words for a Bitcoin header directly in the schedule buffer
/// with previous blockhash in a 32-byte array.
#[inline(always)]
fn header_first_block(version: u32, prev_blockhash: &[u8; 32], merkle_root: &[u8; 32], state: &mut [u64; 8]) {
    let mut w = MaybeUninit::<[u64; 64]>::uninit();
    let ptr = w.as_mut_ptr().cast::<u64>();
    // SAFETY: words 0..16 are all written directly into w before compression.
    unsafe {
        ptr.add(0).write(word_from_le_u32(version));
        for i in 0..8 {
            ptr.add(1 + i).write(word_from_bytes(prev_blockhash, i));
        }
        for i in 0..7 {
            ptr.add(9 + i).write(word_from_bytes(merkle_root, i));
        }
        compress_schedule(ptr, state);
    }
}

/// Build the second SHA-256 block words for a Bitcoin header directly in the schedule buffer.
#[inline(always)]
fn header_second_block(merkle_root: &[u8; 32], timestamp: u32, compact_target: u32, nonce: u32, state: &mut [u64; 8]) {
    let mut w = MaybeUninit::<[u64; 64]>::uninit();
    let ptr = w.as_mut_ptr().cast::<u64>();
    // SAFETY: words 0..16 are all written directly into w before compression.
    unsafe {
        ptr.add(0).write(word_from_bytes(merkle_root, 7));
        ptr.add(1).write(word_from_le_u32(timestamp));
        ptr.add(2).write(word_from_le_u32(compact_target));
        ptr.add(3).write(word_from_le_u32(nonce));
        ptr.add(4).write(0x80000000);
        for i in 5..15 {
            ptr.add(i).write(0);
        }
        ptr.add(15).write(0x280);
        compress_schedule(ptr, state);
    }
}

/// Compute SHA256d over a Bitcoin header, returning the 8 SHA state words directly.
pub fn of_header_words(
    version: u32,
    prev_blockhash: &[u64; 8],
    merkle_root: &[u8; 32],
    timestamp: u32,
    compact_target: u32,
    nonce: u32,
) -> [u64; 8] {
    let mut state = SHA256_IV;
    header_first_block_words(version, prev_blockhash, merkle_root, &mut state);
    header_second_block(merkle_root, timestamp, compact_target, nonce, &mut state);
    of_digest_words(&state)
}

/// Compute SHA256d over a Bitcoin header represented by disjoint fields.
///
/// This is byte-for-byte equivalent to hashing:
/// version || prev_blockhash || merkle_root || timestamp || compact_target || nonce,
/// but avoids materializing either a full `zkpow_core::Header` or `[u8; 80]`.
pub fn of_header(
    version: u32,
    prev_blockhash: &[u8; 32],
    merkle_root: &[u8; 32],
    timestamp: u32,
    compact_target: u32,
    nonce: u32,
) -> [u8; 32] {
    let mut state = SHA256_IV;
    header_first_block(version, prev_blockhash, merkle_root, &mut state);
    header_second_block(merkle_root, timestamp, compact_target, nonce, &mut state);
    let final_state = of_digest_words(&state);
    state_to_hash(&final_state)
}

#[cfg(test)]
mod tests;
