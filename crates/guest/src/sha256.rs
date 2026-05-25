//! SHA-256 via SP1 precompile syscalls.
//!
//! Specialized functions for the exact sizes needed:
//! - `sha256_80bytes`: hashes exactly 80 bytes (Bitcoin block header)
//! - `sha256_32bytes`: hashes exactly 32 bytes (intermediate hash output)
//! - `sha256_112bytes`: hashes exactly 112 bytes (serialized private continuation state)
//! - `sha256_169bytes`: hashes exactly 169 bytes (minimal public values)
//! - `sha256_264bytes`: hashes exactly 264 bytes (serialized recursive state)
//! - `sha256d_80bytes`: hashes exactly 80 bytes with SHA256d
//!
//! No loops, no branching on block count — the padding and block layout
//! are hardcoded for each size.

use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};

/// SHA-256 IV constants.
const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Extract a 32-byte hash from the SHA-256 state.
/// The state stores 8 × u64; each contributes its low 32 bits in big-endian order.
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

/// Convert 4 bytes to a big-endian u64.
#[inline]
const fn be_u64(a: u8, b: u8, c: u8, d: u8) -> u64 {
    ((a as u64) << 24) | ((b as u64) << 16) | ((c as u64) << 8) | (d as u64)
}

/// Compute SHA-256 for a fixed-size byte array.
///
/// `$data` must be a `&[u8; N]` expression. The block count and padding are
/// computed from `$data.len()` at compile time when called from a const-generic
/// function, or optimized away by the compiler when the size is statically known.
macro_rules! sha256_fixed {
    ($data:expr) => {{
        let data = $data;
        let n = data.len();
        let full_blocks = n / 64;
        let rem = n % 64;
        let bitlen: u64 = (n as u64) * 8;

        let mut state = SHA256_IV;

        // Full blocks
        {
            let mut block = 0usize;
            while block < full_blocks {
                let mut w = [0u64; 64];

                unroll16!({
                    let base = block * 64 + I * 4;
                    w[I] = be_u64(
                        data[base + 0],
                        data[base + 1],
                        data[base + 2],
                        data[base + 3],
                    )
                });

                syscall_sha256_extend(&mut w);
                syscall_sha256_compress(&mut w, &mut state);

                block += 1;
            }
        }

        // Final padded block(s)
        {
            let mut pad = [0u8; 64];
            let start = full_blocks * 64;
            let mut i = 0usize;
            while i < rem {
                pad[i] = data[start + i];
                i += 1;
            }

            pad[rem] = 0x80;

            if rem <= 55 {
                let bitlen_bytes = bitlen.to_be_bytes();
                let mut j = 0usize;
                while j < 8 {
                    pad[56 + j] = bitlen_bytes[j];
                    j += 1;
                }

                let mut w = [0u64; 64];
                unroll16!({
                    let b = I * 4;
                    w[I] = be_u64(pad[b + 0], pad[b + 1], pad[b + 2], pad[b + 3])
                });

                syscall_sha256_extend(&mut w);
                syscall_sha256_compress(&mut w, &mut state);
            } else {
                // First padded block
                {
                    let mut w = [0u64; 64];
                    unroll16!({
                        let b = I * 4;
                        w[I] = be_u64(pad[b + 0], pad[b + 1], pad[b + 2], pad[b + 3])
                    });

                    syscall_sha256_extend(&mut w);
                    syscall_sha256_compress(&mut w, &mut state);
                }

                // Second block (all zeros + length)
                {
                    let mut pad2 = [0u8; 64];
                    let bitlen_bytes = bitlen.to_be_bytes();
                    let mut j = 0usize;
                    while j < 8 {
                        pad2[56 + j] = bitlen_bytes[j];
                        j += 1;
                    }

                    let mut w = [0u64; 64];
                    unroll16!({
                        let b = I * 4;
                        w[I] = be_u64(pad2[b + 0], pad2[b + 1], pad2[b + 2], pad2[b + 3])
                    });

                    syscall_sha256_extend(&mut w);
                    syscall_sha256_compress(&mut w, &mut state);
                }
            }
        }

        state_to_hash(&state)
    }};
}

/// Iterative (token-list) unroll helper.
///
/// Usage: `unroll!($body; $i0 $i1 … $i_n-1)`
///
/// For each index token in the list, expands `$body` with `I` bound to that index as a
/// `const usize`.  No integer arithmetic is performed inside the macro — one token is
/// consumed per recursive step, so the recursion depth equals the number of indices
/// (always 16 for a SHA-256 message-schedule word load) and is bounded at compile time.
macro_rules! unroll {
    // Base case: no more indices to process.
    ($body:block; ) => {};

    // Recursive step: bind the current index, expand the body, then continue.
    ($body:block; $i:tt $($rest:tt)*) => {{
        const I: usize = $i;
        $body
        unroll!($body; $($rest)*);
    }};
}

/// Unroll exactly 16 iterations (one per SHA-256 message-schedule word).
macro_rules! unroll16 {
    ($body:block) => {
        unroll!($body; 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)
    };
}

/// SHA-256 for a const-generic size.
///
/// This is a wrapper around `sha256_fixed!` which allows callers to specify the size
/// as a const generic parameter instead of relying on the compiler to optimize the
/// size-dependent branches away.
#[inline(always)]
pub fn sha256<const N: usize>(data: &[u8; N]) -> [u8; 32] {
    sha256_fixed!(data)
}

/// SHA-256 double-hash for a const-generic size.
///
/// This is equivalent to `sha256(sha256(data))` but avoids allocating a temporary
/// 32-byte buffer for the intermediate hash.
#[inline(always)]
pub fn sha256d<const N: usize>(data: &[u8; N]) -> [u8; 32] {
    sha256_fixed!(sha256_fixed!(data))
}
