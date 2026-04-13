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

mod sha256;
use sha256::double_sha256_80;

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
const STATUS_HEIGHT_MISMATCH: u8 = 5;
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
fn bits_to_target_from_header_bytes(header: &[u8; 80]) -> [u8; 32] {
    let bits = header[72] as u32
        | (header[73] as u32) << 8
        | (header[74] as u32) << 16
        | (header[75] as u32) << 24;
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
        prev_num_headers,
    ) = if has_prev_proof {
        let _prev_vk_digest = sp1_zkvm::io::read::<[u32; 8]>();
        let _prev_pv_digest = sp1_zkvm::io::read::<[u8; 32]>();
        let prev_public_values = sp1_zkvm::io::read_vec();

        sp1_zkvm::lib::verify::verify_sp1_proof(&_prev_vk_digest, &_prev_pv_digest);

        if prev_public_values.len() < 237 {
            panic!("Previous proof public values too short");
        }
        let pv = &prev_public_values;

        // Read PV fields with direct indexing — no unwrap, no slices.
        // All ranges are provably within bounds because we checked pv.len() ≥ 237.
        let prev_genesis: [u8; 32] = {
            let mut a = [0u8; 32];
            a.copy_from_slice(&pv[0..32]);
            a
        };
        let prev_final_hash: [u8; 32] = {
            let mut a = [0u8; 32];
            a.copy_from_slice(&pv[32..64]);
            a
        };
        let prev_num_headers = pv[64] as u64
            | (pv[65] as u64) << 8
            | (pv[66] as u64) << 16
            | (pv[67] as u64) << 24
            | (pv[68] as u64) << 32
            | (pv[69] as u64) << 40
            | (pv[70] as u64) << 48
            | (pv[71] as u64) << 56;

        let prev_chain_work: [u64; 4] = [
            pv[152] as u64 | (pv[153] as u64) << 8 | (pv[154] as u64) << 16 | (pv[155] as u64) << 24
                | (pv[156] as u64) << 32 | (pv[157] as u64) << 40 | (pv[158] as u64) << 48 | (pv[159] as u64) << 56,
            pv[160] as u64 | (pv[161] as u64) << 8 | (pv[162] as u64) << 16 | (pv[163] as u64) << 24
                | (pv[164] as u64) << 32 | (pv[165] as u64) << 40 | (pv[166] as u64) << 48 | (pv[167] as u64) << 56,
            pv[168] as u64 | (pv[169] as u64) << 8 | (pv[170] as u64) << 16 | (pv[171] as u64) << 24
                | (pv[172] as u64) << 32 | (pv[173] as u64) << 40 | (pv[174] as u64) << 48 | (pv[175] as u64) << 56,
            pv[176] as u64 | (pv[177] as u64) << 8 | (pv[178] as u64) << 16 | (pv[179] as u64) << 24
                | (pv[180] as u64) << 32 | (pv[181] as u64) << 40 | (pv[182] as u64) << 48 | (pv[183] as u64) << 56,
        ];
        let prev_epoch_ts = pv[184] as u32
            | (pv[185] as u32) << 8
            | (pv[186] as u32) << 16
            | (pv[187] as u32) << 24;
        let prev_median: [u32; WINDOW_SIZE] = core::array::from_fn(|i| {
            let off = 188 + i * 4;
            pv[off] as u32
                | (pv[off + 1] as u32) << 8
                | (pv[off + 2] as u32) << 16
                | (pv[off + 3] as u32) << 24
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
            bits_to_target_from_header_bytes({
                // SAFETY: pv.len() >= 237, so pv[72..152] is always exactly 80 bytes.
                let slice: &[u8; 80] = pv[72..152].try_into().unwrap();
                slice
            }),
            prev_median,
            prev_median_head,
            prev_median_len,
            prev_median_packed,
            prev_num_headers,
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
            0u64,
        )
    };

    let start_height = sp1_zkvm::io::read::<u64>();

    // Validate chain continuity: the new batch must start exactly where the
    // previous proof left off. If no previous proof, must start from genesis (0).
    let expected_start = if has_prev_proof {
        prev_num_headers
    } else {
        0u64
    };
    if start_height != expected_start {
        commit_error_and_exit(
            &prev_hash, cumulative_chain_work,
            last_epoch_start_timestamp, median_timestamps,
            start_height, 0, // num_headers = 0 (we haven't read it yet)
            STATUS_HEIGHT_MISMATCH, 0,
        );
    }

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

        // Parse header fields with direct indexing — no unwrap, no slices.
        let mut prev_blockhash = [0u8; 32];
        prev_blockhash.copy_from_slice(&header[4..36]);

        let timestamp = header[68] as u32
            | (header[69] as u32) << 8
            | (header[70] as u32) << 16
            | (header[71] as u32) << 24;

        let bits = header[72] as u32
            | (header[73] as u32) << 8
            | (header[74] as u32) << 16
            | (header[75] as u32) << 24;

        // Validate bits range: exponent must be 3..=29 for a valid target.
        let exponent = bits >> 24;
        if !(3..=29).contains(&exponent) {
            println!("cycle-tracker-end: parse");
            commit_error_and_exit(
                &prev_hash, cumulative_chain_work,
                last_epoch_start_timestamp, median_timestamps,
                start_height, num_headers,
                STATUS_BITS_MISMATCH, i as u32,
            );
        }

        println!("cycle-tracker-end: parse");

        if current_height == 0 {
            println!("cycle-tracker-start: sha256d");
            // SAFETY: headers_bytes.len() == num_headers * 80 was verified above.
            // This slice is always exactly 80 bytes.
            let header_array: &[u8; 80] = header.try_into().unwrap();
            let computed_hash = double_sha256_80(header_array);
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
            // SAFETY: header is &headers_bytes[offset..offset+80], always exactly 80 bytes.
            let block_hash = double_sha256_80(header.try_into().unwrap());
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
    // Convert [u8; 32] to [u64; 4] LE with direct indexing — no unwrap.
    let old_u64 = [
        old_target[0] as u64 | (old_target[1] as u64) << 8 | (old_target[2] as u64) << 16
            | (old_target[3] as u64) << 24 | (old_target[4] as u64) << 32
            | (old_target[5] as u64) << 40 | (old_target[6] as u64) << 48 | (old_target[7] as u64) << 56,
        old_target[8] as u64 | (old_target[9] as u64) << 8 | (old_target[10] as u64) << 16
            | (old_target[11] as u64) << 24 | (old_target[12] as u64) << 32
            | (old_target[13] as u64) << 40 | (old_target[14] as u64) << 48 | (old_target[15] as u64) << 56,
        old_target[16] as u64 | (old_target[17] as u64) << 8 | (old_target[18] as u64) << 16
            | (old_target[19] as u64) << 24 | (old_target[20] as u64) << 32
            | (old_target[21] as u64) << 40 | (old_target[22] as u64) << 48 | (old_target[23] as u64) << 56,
        old_target[24] as u64 | (old_target[25] as u64) << 8 | (old_target[26] as u64) << 16
            | (old_target[27] as u64) << 24 | (old_target[28] as u64) << 32
            | (old_target[29] as u64) << 40 | (old_target[30] as u64) << 48 | (old_target[31] as u64) << 56,
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
