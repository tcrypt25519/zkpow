//! Bitcoin Header Chain Prover — zkVM Program
//!
//! Validates a batch of Bitcoin block headers by verifying:
//! - Genesis block hash matches the expected genesis (for height 0)
//! - Chain linkage: each header's prev_blockhash matches the previous block's hash
//! - Proof-of-work: each header's double SHA-256 hash meets the difficulty target from bits
//! - Difficulty retargeting every 2016 blocks
//! - BIP113 median timestamp check
//! - Cumulative chain work using Bitcoin's canonical formula

#![no_main]
sp1_zkvm::entrypoint!(main);

use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};

// ============================================================================
// Error codes — committed to public values on failure.
// The program exits cleanly (HALT 0) with a non-zero status, producing a
// valid proof that "validation failed at header X for reason Y".
// ============================================================================

const STATUS_SUCCESS: u8 = 0;
const STATUS_GENESIS_HASH_MISMATCH: u8 = 1;
const STATUS_PREV_BLOCKHASH_MISMATCH: u8 = 2;
const STATUS_POW_INSUFFICIENT: u8 = 3;
const STATUS_TIMESTAMP_TOO_OLD: u8 = 4;
const STATUS_BITS_MISMATCH: u8 = 6;
const STATUS_HEADER_COUNT_MISMATCH: u8 = 7;
// STATUS_TIMESTAMP_FUTURE = 5        (network policy, not consensus)
// STATUS_PREV_PROOF_TOO_SHORT = 8     (tested via recursive_chain_success)
// STATUS_PREV_PROOF_GENESIS_MISMATCH = 9  (tested via recursive_chain_success)
// STATUS_PREV_PROOF_FAILED = 10       (tested via recursive_chain_success)

#[allow(clippy::too_many_arguments)]
fn commit_error_and_exit(
    prev_hash: &[u8; 32],
    cumulative_chain_work: [u64; 4],
    last_epoch_start_timestamp: u32,
    median_timestamps: [u32; 11],
    start_height: u64,
    num_headers: u64,
    error_code: u8,
    detail: u32,
) -> ! {
    sp1_zkvm::io::commit_slice(prev_hash);
    let total_validated = start_height + num_headers;
    sp1_zkvm::io::commit_slice(&total_validated.to_le_bytes());
    sp1_zkvm::io::commit_slice(&[0u8; 80]); // placeholder final_header
    for &word in &cumulative_chain_work {
        sp1_zkvm::io::commit_slice(&word.to_le_bytes());
    }
    sp1_zkvm::io::commit_slice(&last_epoch_start_timestamp.to_le_bytes());
    for &ts in &median_timestamps {
        sp1_zkvm::io::commit_slice(&ts.to_le_bytes());
    }
    sp1_zkvm::io::commit_slice(&[error_code]);
    sp1_zkvm::io::commit_slice(&detail.to_le_bytes());

    sp1_zkvm::syscalls::syscall_halt(0);
}

/// SHA-256 IV constants (big-endian u32 as u64)
const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute SHA-256 of arbitrary data using SP1's SHA-256 precompile syscalls.
///
/// The precompile works on 64-byte blocks. Each block's 16 words (4 bytes each)
/// are stored as u64 in big-endian order in w[0..16].
///
/// For each block:
/// 1. Fill w[0..16] with data bytes (big-endian u32 as u64)
/// 2. Call syscall_sha256_extend(&mut w) to expand to w[16..64]
/// 3. Call syscall_sha256_compress(&mut w, &mut state) to compress
fn sha256_syscall(data: &[u8]) -> [u8; 32] {
    let mut state = SHA256_IV;
    let total_bits = (data.len() as u64) * 8;

    let mut offset = 0;
    while offset < data.len() {
        let mut w = [0u64; 64];
        let remaining = data.len() - offset;

        if remaining >= 64 {
            // Full block: 64 bytes of data
            for (j, chunk) in data[offset..offset + 64].chunks(4).enumerate() {
                w[j] = u32::from_be_bytes(chunk.try_into().unwrap()) as u64;
            }
            offset += 64;
        } else {
            // Last block: data + padding + length
            let mut block = [0u8; 64];
            let data_bytes = remaining.min(64);
            block[..data_bytes].copy_from_slice(&data[offset..offset + data_bytes]);

            if data_bytes < 64 {
                block[data_bytes] = 0x80;
                // If length doesn't fit (need < 56 bytes for data), this is the
                // last block and length goes here
                if data_bytes <= 55 {
                    let len_bytes = total_bits.to_be_bytes();
                    block[56..64].copy_from_slice(&len_bytes);
                }
            }

            for (j, chunk) in block.chunks(4).enumerate() {
                w[j] = u32::from_be_bytes(chunk.try_into().unwrap()) as u64;
            }
            offset = data.len();

            // If we need a second block for the length
            if data_bytes > 55 {
                syscall_sha256_extend(&mut w);
                syscall_sha256_compress(&mut w, &mut state);
                // Second block: just length
                let mut w2 = [0u64; 64];
                let len_bytes = total_bits.to_be_bytes();
                w2[14] = u32::from_be_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as u64;
                w2[15] = u32::from_be_bytes([len_bytes[4], len_bytes[5], len_bytes[6], len_bytes[7]]) as u64;
                w = w2;
            }
        }

        syscall_sha256_extend(&mut w);
        syscall_sha256_compress(&mut w, &mut state);
    }

    // Extract hash from state (big-endian u32 → bytes)
    let mut hash = [0u8; 32];
    for i in 0..8 {
        let bytes = state[i].to_be_bytes();
        hash[i * 4..(i + 1) * 4].copy_from_slice(&bytes[4..8]);
    }
    hash
}

/// Compute double SHA-256: SHA-256(SHA-256(data)).
fn double_sha256(data: &[u8]) -> [u8; 32] {
    let inner = sha256_syscall(data);
    sha256_syscall(&inner)
}

fn bits_to_target(bits: u32) -> [u8; 32] {
    let exponent = bits >> 24;
    let mantissa = bits & 0x00ffffff;
    let mut target = [0u8; 32];
    if mantissa == 0 {
        return target;
    }
    let byte_offset = (exponent - 3) as usize;
    if byte_offset < 32 {
        target[byte_offset] = (mantissa & 0xff) as u8;
    }
    if byte_offset + 1 < 32 {
        target[byte_offset + 1] = ((mantissa >> 8) & 0xff) as u8;
    }
    if byte_offset + 2 < 32 {
        target[byte_offset + 2] = ((mantissa >> 16) & 0xff) as u8;
    }
    target
}

/// Extract bits from raw 80-byte header and convert to target.
fn bits_to_target_from_header_bytes(header_bytes: &[u8]) -> [u8; 32] {
    let bits = u32::from_le_bytes(header_bytes[72..76].try_into().unwrap());
    bits_to_target(bits)
}

fn hash_meets_target(hash: &[u8; 32], bits: u32) -> bool {
    let target = bits_to_target(bits);
    for i in (0..32).rev() {
        if hash[i] > target[i] {
            return false;
        }
        if hash[i] < target[i] {
            return true;
        }
    }
    true
}

fn target_to_bits(target: &[u8; 32]) -> u32 {
    let mut high_byte = 31usize;
    while high_byte > 0 && target[high_byte] == 0 {
        high_byte -= 1;
    }
    if target[high_byte] == 0 {
        return 0;
    }
    let bit_length = high_byte * 8 + (8 - target[high_byte].leading_zeros() as usize);
    let nbytes = bit_length.div_ceil(8);
    let mantissa: u32 = if high_byte >= 2 {
        (target[high_byte] as u32) << 16
            | (target[high_byte - 1] as u32) << 8
            | target[high_byte - 2] as u32
    } else if high_byte == 1 {
        (target[1] as u32) << 8 | target[0] as u32
    } else {
        target[0] as u32
    };
    let (mantissa, nbytes) = if mantissa & 0x800000 != 0 {
        (mantissa >> 8, nbytes + 1)
    } else {
        (mantissa, nbytes)
    };
    ((nbytes as u32) << 24) | (mantissa & 0x00ffffff)
}

fn u256_add(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut result = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let sum = (a[i] as u128) + (b[i] as u128) + carry;
        result[i] = sum as u64;
        carry = sum >> 64;
    }
    result
}

// ============================================================================
// Sliding window of the 11 most recent block timestamps (BIP113 MTP)
//
// Maintains sorted order incrementally using packed nibble indices, avoiding
// full re-sort on every block. The timestamps buffer stores in insertion
// (height) order; the packed field stores sorted indices as 4-bit nibbles.
// ============================================================================

const WINDOW_SIZE: usize = 11;
const NIBBLE_BITS: usize = 4;
const NIBBLE_MASK: u64 = 0xF;

/// Returns true if `ts` is ≤ the median of the sorted window.
///
/// For len < 11, no check is performed (MTP requires a full window).
/// When len == 11, returns `ts <= sorted[5]`.
fn check_median(
    timestamps: &[u32; WINDOW_SIZE],
    len: usize,
    _head: u8,
    packed: u64,
    ts: u32,
) -> bool {
    if len < WINDOW_SIZE {
        return false; // no check needed
    }
    let median_pos = (len - 1) / 2; // = 5 for len=11
    let idx = get_nibble(packed, median_pos) as usize;
    ts <= timestamps[idx]
}

/// Add a timestamp to the circular buffer and update the packed sorted indices.
/// Returns updated (head, packed). When the window is full, the oldest entry
/// (at `head`) is evicted before inserting the new one.
fn add_timestamp(
    timestamps: &mut [u32; WINDOW_SIZE],
    mut head: u8,
    mut len: usize,
    mut packed: u64,
    ts: u32,
) -> (u8, usize, u64) {
    if len < WINDOW_SIZE {
        // Growing phase: insert at head position
        timestamps[head as usize] = ts;

        // Find sorted position for this timestamp
        let pos = find_insert_position(timestamps, packed, len, ts);
        packed = insert_nibble(packed, pos, head, len);

        len += 1;
        head = (head + 1) % WINDOW_SIZE as u8;
    } else {
        // Full window: evict oldest (at head), replace with new

        // 1. Find where the evicted entry sits in the sorted order
        let pos_old = find_index_position(packed, len, head as usize);

        // 2. Remove that nibble from packed (now has 10 entries)
        packed = remove_nibble(packed, pos_old, len);

        // 3. Find where the new timestamp belongs in the sorted order
        let pos_new = find_insert_position(timestamps, packed, len - 1, ts);

        // 4. Insert the reused buffer index at the correct sorted position
        packed = insert_nibble(packed, pos_new, head, len - 1);

        // 5. Overwrite the timestamp and advance head
        timestamps[head as usize] = ts;
        head = (head + 1) % WINDOW_SIZE as u8;
    }

    (head, len, packed)
}

/// Linear scan over sorted indices to find where `ts` belongs.
/// Equal timestamps are inserted after existing ones (stable sort).
fn find_insert_position(
    timestamps: &[u32; WINDOW_SIZE],
    packed: u64,
    count: usize,
    ts: u32,
) -> usize {
    for i in 0..count {
        let idx = get_nibble(packed, i) as usize;
        if ts < timestamps[idx] {
            return i;
        }
    }
    count
}

/// Find the sorted position (0-based) of a given buffer index in packed.
fn find_index_position(packed: u64, count: usize, target: usize) -> usize {
    for i in 0..count {
        if get_nibble(packed, i) as usize == target {
            return i;
        }
    }
    count
}

/// Rebuild the packed sorted indices from the timestamps buffer.
/// This is needed when resuming from a previous proof's PV.
fn rebuild_packed(timestamps: &[u32; WINDOW_SIZE], len: usize) -> u64 {
    // Sort indices 0..len by their timestamp values (simple insertion sort)
    let mut indices: [u8; WINDOW_SIZE] = [0; WINDOW_SIZE];
    let mut sorted_count = 0;

    for (i, ts) in timestamps.iter().take(len).enumerate() {
        // Find insertion position
        let mut pos = sorted_count;
        for (j, idx) in indices.iter().take(sorted_count).enumerate() {
            if *ts < timestamps[*idx as usize] {
                pos = j;
                break;
            }
        }
        // Shift right and insert
        for k in (pos + 1..sorted_count + 1).rev() {
            indices[k] = indices[k - 1];
        }
        indices[pos] = i as u8;
        sorted_count += 1;
    }

    // Pack into nibbles
    let mut packed = 0u64;
    for (i, idx) in indices.iter().take(sorted_count).enumerate() {
        packed |= (*idx as u64) << (i * NIBBLE_BITS);
    }
    packed
}

#[inline]
fn get_nibble(packed: u64, pos: usize) -> u8 {
    ((packed >> (pos * NIBBLE_BITS)) & NIBBLE_MASK) as u8
}

/// Remove nibble at `pos` from packed (which has `count` nibbles).
/// Returns packed with `count-1` nibbles.
fn remove_nibble(packed: u64, pos: usize, _count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed >> ((pos + 1) * NIBBLE_BITS)) << (pos * NIBBLE_BITS);
    lower | upper
}

/// Insert `val` at position `pos` into packed (which has `count` nibbles).
/// Returns packed with `count+1` nibbles.
fn insert_nibble(packed: u64, pos: usize, val: u8, count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed & !lower_mask) << NIBBLE_BITS;

    let new_packed = lower | ((val as u64) << (pos * NIBBLE_BITS)) | upper;
    let new_mask = (1u64 << ((count + 1) * NIBBLE_BITS)) - 1;
    new_packed & new_mask
}

// ============================================================================
// Canonical chain work: floor(2^256 / (target + 1))
// ============================================================================

fn pow_mod_2(exp: u32, m: u32) -> u32 {
    if m <= 1 {
        return 0;
    }
    let m64 = m as u64;
    let mut result: u64 = 1;
    let mut base: u64 = 2;
    let mut e = exp;
    while e > 0 {
        if e & 1 != 0 {
            result = (result * base) % m64;
        }
        base = (base * base) % m64;
        e >>= 1;
    }
    result as u32
}

fn div_2n_minus_r_by_u32(n: u32, r: u32, m: u32) -> [u64; 4] {
    if n <= 63 {
        let val = ((1u64 << n) - r as u64) / m as u64;
        return [val, 0, 0, 0];
    }
    if n <= 127 {
        let val = ((1u128 << n) - r as u128) / m as u128;
        return [val as u64, (val >> 64) as u64, 0, 0];
    }

    let bit_limb = (n / 64) as usize;
    let bit_offset = n % 64;

    let mut limbs = [0u64; 5];
    limbs[0] = u64::MAX.wrapping_sub(r as u64 - 1);
    for item in limbs.iter_mut().take(bit_limb.min(4)).skip(1) {
        *item = u64::MAX;
    }
    if bit_limb < 4 {
        limbs[bit_limb] = (1u64 << bit_offset).wrapping_sub(1);
    } else if bit_limb == 4 {
        limbs[4] = (1u64 << bit_offset).wrapping_sub(1);
    }

    let m64 = m as u64;
    let mut q = [0u64; 4];
    let mut rem = 0u64;
    for i in (0..5).rev() {
        let val = ((rem as u128) << 64) | (limbs[i] as u128);
        let quot = (val / m64 as u128) as u64;
        rem = (val % m64 as u128) as u64;
        if i < 4 {
            q[i] = quot;
        }
    }

    q
}

fn q_le_r_shifted(q: &[u64; 4], r: u32, k: u32) -> bool {
    if r == 0 {
        return q.iter().all(|&x| x == 0);
    }
    if k >= 256 {
        return true;
    }

    let r64 = r as u64;
    let lo = k % 64;
    let hi_limb = (k / 64) as usize;

    let mut rv = [0u64; 4];
    if hi_limb < 4 {
        rv[hi_limb] = r64 << lo;
    }
    if hi_limb + 1 < 4 && lo > 0 {
        rv[hi_limb + 1] = r64 >> (64 - lo);
    }

    for i in (0..4).rev() {
        if q[i] < rv[i] {
            return true;
        }
        if q[i] > rv[i] {
            return false;
        }
    }
    true
}

fn work_from_bits(bits: u32) -> [u64; 4] {
    let exponent = bits >> 24;
    let mantissa = bits & 0x00ffffff;
    let k = 8 * (exponent - 3);
    let n = 256 - k;

    if mantissa == 0 || n == 0 {
        return [0; 4];
    }

    let r = pow_mod_2(n, mantissa);
    let q = div_2n_minus_r_by_u32(n, r, mantissa);

    let mut work = q;
    if !q_le_r_shifted(&q, r, k) {
        for item in &mut work {
            if *item > 0 {
                *item -= 1;
                break;
            } else {
                *item = u64::MAX;
            }
        }
    }

    work
}

pub fn main() {
    // Read genesis hash and commit
    let expected_genesis_hash = sp1_zkvm::io::read::<[u8; 32]>();
    sp1_zkvm::io::commit_slice(&expected_genesis_hash);

    // Check if we're extending a previous proof or starting from genesis
    let has_prev_proof = sp1_zkvm::io::read::<bool>();

    let (
        mut prev_hash,
        mut cumulative_chain_work,
        mut last_epoch_start_timestamp,
        mut prev_target,
        mut median_timestamps,
        mut median_head,
        mut median_len,
        mut median_packed,
    ) = if has_prev_proof {
        let _prev_vk_digest = sp1_zkvm::io::read::<[u32; 8]>();
        let _prev_pv_digest = sp1_zkvm::io::read::<[u8; 32]>();
        let prev_public_values = sp1_zkvm::io::read_vec();

        sp1_zkvm::lib::verify::verify_sp1_proof(&_prev_vk_digest, &_prev_pv_digest);

        if prev_public_values.len() < 237 {
            panic!("Previous proof public values too short");
        }

        let prev_genesis: [u8; 32] = prev_public_values[0..32].try_into().unwrap();
        let prev_final_hash: [u8; 32] = prev_public_values[32..64].try_into().unwrap();
        let prev_num_headers = u64::from_le_bytes(prev_public_values[64..72].try_into().unwrap());

        let prev_chain_work: [u64; 4] = [
            u64::from_le_bytes(prev_public_values[152..160].try_into().unwrap()),
            u64::from_le_bytes(prev_public_values[160..168].try_into().unwrap()),
            u64::from_le_bytes(prev_public_values[168..176].try_into().unwrap()),
            u64::from_le_bytes(prev_public_values[176..184].try_into().unwrap()),
        ];
        let prev_epoch_ts = u32::from_le_bytes(prev_public_values[184..188].try_into().unwrap());
        let prev_median: [u32; WINDOW_SIZE] = core::array::from_fn(|i| {
            u32::from_le_bytes(prev_public_values[(188 + i * 4)..(192 + i * 4)].try_into().unwrap())
        });

        // Derive median window state from total headers
        let prev_median_len = if prev_num_headers == 0 {
            0usize
        } else {
            prev_num_headers.min(11) as usize
        };
        let prev_median_head = if prev_median_len < WINDOW_SIZE {
            prev_median_len as u8
        } else {
            (prev_num_headers % WINDOW_SIZE as u64) as u8
        };
        let prev_median_packed = rebuild_packed(&prev_median, prev_median_len);

        let prev_success = prev_public_values[232];

        if prev_genesis != expected_genesis_hash {
            panic!("Genesis hash mismatch with previous proof");
        }
        if prev_success != STATUS_SUCCESS {
            panic!("Previous proof did not succeed");
        }

        (
            prev_final_hash,
            prev_chain_work,
            prev_epoch_ts,
            bits_to_target_from_header_bytes(&prev_public_values[72..152]),
            prev_median,
            prev_median_head,
            prev_median_len,
            prev_median_packed,
        )
    } else {
        (
            expected_genesis_hash,
            [0u64; 4],
            1231006505u32,
            bits_to_target(0x1d00ffff),
            [0u32; WINDOW_SIZE],
            0u8,
            0usize,
            0u64,
        )
    };

    let start_height = sp1_zkvm::io::read::<u64>();
    let num_headers = sp1_zkvm::io::read::<u64>();
    let headers_bytes = sp1_zkvm::io::read_vec();

    let expected_len = (num_headers * 80) as usize;
    if headers_bytes.len() != expected_len {
        commit_error_and_exit(
            &prev_hash, cumulative_chain_work,
            last_epoch_start_timestamp, median_timestamps,
            start_height, num_headers,
            STATUS_HEADER_COUNT_MISMATCH, 0,
        );
    }

    let mut final_header: [u8; 80] = [0; 80];

    for i in 0..num_headers {
        let offset = (i * 80) as usize;
        let header = &headers_bytes[offset..offset + 80];
        let current_height = start_height + i;

        println!("cycle-tracker-start: parse");
        let prev_blockhash: [u8; 32] = header[4..36].try_into().unwrap();
        let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
        let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());
        println!("cycle-tracker-end: parse");

        if current_height == 0 {
            println!("cycle-tracker-start: sha256d");
            let computed_hash = double_sha256(header);
            println!("cycle-tracker-end: sha256d");
            if computed_hash != expected_genesis_hash {
                commit_error_and_exit(
                    &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_GENESIS_HASH_MISMATCH, i as u32,
                );
            }
            prev_hash = computed_hash;
            last_epoch_start_timestamp = timestamp;
            prev_target = bits_to_target(bits);
            cumulative_chain_work = u256_add(cumulative_chain_work, work_from_bits(bits));
            // Initialize median window with genesis timestamp
            (median_head, median_len, median_packed) =
                add_timestamp(&mut median_timestamps, median_head, median_len, median_packed, timestamp);
        } else {
            if prev_blockhash != prev_hash {
                commit_error_and_exit(
                    &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_PREV_BLOCKHASH_MISMATCH, i as u32,
                );
            }

            // === BIP113: Median timestamp check ===
            if check_median(&median_timestamps, median_len, median_head, median_packed, timestamp) {
                commit_error_and_exit(
                    &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_TIMESTAMP_TOO_OLD, i as u32,
                );
            }

            // Note: Bitcoin's "timestamp ≤ MTP + 2h" rule uses the network's adjusted
            // time (median of peer clocks), not the previous block's timestamp. Since
            // we don't have access to wall clock time in the zkVM, and the 2h rule is
            // a network policy rather than a consensus rule, we omit this check here.
            // The BIP113 median check (above) is the consensus-critical timestamp rule.

            // Difficulty retargeting every 2016 blocks
            if current_height > 0 && current_height.is_multiple_of(2016) {
                println!("cycle-tracker-start: retarget");
                let actual_timespan = timestamp.wrapping_sub(last_epoch_start_timestamp);
                let expected_timespan: u32 = 2016 * 600;
                let clamped = actual_timespan
                    .max(expected_timespan / 4)
                    .min(expected_timespan * 4);
                prev_target = retarget_target(&prev_target, clamped, expected_timespan);
                last_epoch_start_timestamp = timestamp;
                println!("cycle-tracker-end: retarget");
            }

            // Verify bits match expected target
            let expected_bits = target_to_bits(&prev_target);
            if bits != expected_bits {
                commit_error_and_exit(
                    &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_BITS_MISMATCH, i as u32,
                );
            }

            println!("cycle-tracker-start: sha256d");
            let block_hash = double_sha256(header);
            println!("cycle-tracker-end: sha256d");
            if !hash_meets_target(&block_hash, bits) {
                commit_error_and_exit(
                    &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_POW_INSUFFICIENT, i as u32,
                );
            }

            prev_hash = block_hash;
            prev_target = bits_to_target(bits);
            cumulative_chain_work = u256_add(cumulative_chain_work, work_from_bits(bits));

            // === Update median timestamp window ===
            (median_head, median_len, median_packed) =
                add_timestamp(&mut median_timestamps, median_head, median_len, median_packed, timestamp);
        }

        final_header.copy_from_slice(header);
    }

    // Commit outputs — success path
    sp1_zkvm::io::commit_slice(&prev_hash);
    let total_validated = start_height + num_headers;
    sp1_zkvm::io::commit_slice(&total_validated.to_le_bytes());
    sp1_zkvm::io::commit_slice(&final_header);
    for &word in &cumulative_chain_work {
        sp1_zkvm::io::commit_slice(&word.to_le_bytes());
    }
    sp1_zkvm::io::commit_slice(&last_epoch_start_timestamp.to_le_bytes());
    for &ts in &median_timestamps {
        sp1_zkvm::io::commit_slice(&ts.to_le_bytes());
    }
    // Success code + detail (0 = no error)
    sp1_zkvm::io::commit_slice(&[STATUS_SUCCESS]);
    sp1_zkvm::io::commit_slice(&0u32.to_le_bytes());

    // median_count is NOT committed — it's derivable from total_validated:
    //   count = min(11, total_validated - 1) for total_validated > 0, else 0
    // This saves 4 bytes per proof.

    println!(
        "Successfully validated {} headers from height {} to height {}",
        num_headers,
        start_height,
        start_height + num_headers - 1
    );
}

fn retarget_target(
    old_target: &[u8; 32],
    actual_timespan: u32,
    expected_timespan: u32,
) -> [u8; 32] {
    let old_u64 = [
        u64::from_le_bytes(old_target[0..8].try_into().unwrap()),
        u64::from_le_bytes(old_target[8..16].try_into().unwrap()),
        u64::from_le_bytes(old_target[16..24].try_into().unwrap()),
        u64::from_le_bytes(old_target[24..32].try_into().unwrap()),
    ];

    let mut product = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let prod = (old_u64[i] as u128) * (actual_timespan as u128) + carry;
        product[i] = prod as u64;
        carry = prod >> 64;
    }

    let mut result = [0u64; 4];
    let mut remainder = 0u128;
    for i in (0..4).rev() {
        let val = (remainder << 64) | (product[i] as u128);
        result[i] = (val / (expected_timespan as u128)) as u64;
        remainder = val % (expected_timespan as u128);
    }

    let mut target = [0u8; 32];
    for i in 0..4 {
        target[i * 8..(i + 1) * 8].copy_from_slice(&result[i].to_le_bytes());
    }
    target
}
