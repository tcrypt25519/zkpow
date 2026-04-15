//! Bitcoin Header Chain Prover — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit constructs the full 80-byte header from
//! authenticated state, then hashes and validates.
//!
//! Input protocol:
//!   1. prev_height: u32
//!   2. If prev_height > 0: prev_vk([u32;8]), pv_digest([u8;32]), pv_bytes (192 bytes)
//!   3. num_headers: u32
//!   4. headers_bytes: Vec<u8> — num_headers * 44 bytes (NewHeader instances)
//!
//! Output: serialized State (192 bytes) on success, or state + error_code + header_index on error.

#![no_main]
sp1_zkvm::entrypoint!(main);

mod sha256;
use sha256::double_sha256_80;

// ============================================================================
// Constants & Error Codes
// ============================================================================

const WINDOW_SIZE: usize = 11;
const NIBBLE_BITS: usize = 4;
const NIBBLE_MASK: u64 = 0xF;
const NEW_HEADER_SIZE: usize = 44; // version(4) + merkle_root(32) + timestamp(4) + nonce(4)
const STATE_SIZE: usize = 192;     // 32+32+4+32+4+32+4+44+8
const GENESIS_NBITS: u32 = 0x1d00ffff;
const MAINNET_GENESIS_HASH_RAW: [u8; 32] = [
    0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72,
    0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f,
    0x93, 0x1e, 0x83, 0x65, 0xe1, 0x5a, 0x08, 0x9c,
    0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const STATUS_HEADER_COUNT_MISMATCH: u8 = 1;
const STATUS_POW_INSUFFICIENT: u8 = 2;
const STATUS_TIMESTAMP_TOO_OLD: u8 = 3;
const STATUS_GENESIS_HASH_MISMATCH: u8 = 4;
// Removed: prev_blockhash mismatch, bits mismatch — impossible by construction

// ============================================================================
// NewHeader — Prover-supplied fields (44 bytes)
//
// Only non-deterministic values. prev_blockhash and nbits are constructed
// from authenticated state, not supplied by the prover.
// ============================================================================

#[derive(Clone, Debug)]
struct NewHeader {
    version: u32,
    merkle_root: [u8; 32],
    timestamp: u32,
    nonce: u32,
}

impl NewHeader {
    /// Parse a NewHeader from 44 bytes at the given offset.
    fn from_bytes(data: &[u8], offset: usize) -> Self {
        let version = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(&data[offset + 4..offset + 36]);
        let timestamp = u32::from_le_bytes(data[offset + 36..offset + 40].try_into().unwrap());
        let nonce = u32::from_le_bytes(data[offset + 40..offset + 44].try_into().unwrap());
        Self {
            version,
            merkle_root,
            timestamp,
            nonce,
        }
    }
}

// ============================================================================
// State Structure (192 bytes)
//
/// The complete validation state, persisted between IVC iterations.
///
/// Layout:
///   0..32    genesis_hash       — anchor of the chain (set at height 0)
///  32..64    prev_blockhash     — SHA256d of last validated header
///  64..68    nbits              — current difficulty compact form
///  68..100   target             — current difficulty as 256-bit integer
/// 100..104   height             — number of validated blocks
/// 104..136   chain_work         — 256-bit cumulative work (LE [u64; 4])
/// 136..140   epoch_start_ts     — timestamp of first block in current epoch
/// 140..184   timestamps         — circular buffer of last min(height,11) timestamps
/// 184..192   sorted_nibbles     — packed sorted indices for O(1) median lookup
///
/// Total: 32 + 32 + 4 + 32 + 4 + 32 + 4 + 44 + 8 = 192 bytes
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
struct State {
    genesis_hash: [u8; 32],
    prev_blockhash: [u8; 32],
    nbits: u32,
    target: [u8; 32],
    height: u32,
    chain_work: [u64; 4],
    epoch_start_timestamp: u32,
    timestamps: [u32; WINDOW_SIZE],
    sorted_nibbles: u64,
}

impl State {
    /// Serialize to exactly 192 bytes. No length prefixes — fixed layout.
    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(STATE_SIZE);
        out.extend_from_slice(&self.genesis_hash);
        out.extend_from_slice(&self.prev_blockhash);
        out.extend_from_slice(&self.nbits.to_le_bytes());
        out.extend_from_slice(&self.target);
        out.extend_from_slice(&self.height.to_le_bytes());
        for &limb in &self.chain_work {
            out.extend_from_slice(&limb.to_le_bytes());
        }
        out.extend_from_slice(&self.epoch_start_timestamp.to_le_bytes());
        for &ts in &self.timestamps {
            out.extend_from_slice(&ts.to_le_bytes());
        }
        out.extend_from_slice(&self.sorted_nibbles.to_le_bytes());
        out
    }

    /// Deserialize from exactly 192 bytes.
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut off = 0;

        let genesis_hash: [u8; 32] = bytes[off..off + 32].try_into().unwrap();
        off += 32;

        let prev_blockhash: [u8; 32] = bytes[off..off + 32].try_into().unwrap();
        off += 32;

        let nbits = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;

        let mut target = [0u8; 32];
        target.copy_from_slice(&bytes[off..off + 32]);
        off += 32;

        let height = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;

        let mut chain_work = [0u64; 4];
        for limb in chain_work.iter_mut() {
            *limb = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            off += 8;
        }

        let epoch_start_timestamp =
            u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;

        let mut timestamps = [0u32; WINDOW_SIZE];
        for ts in timestamps.iter_mut() {
            *ts = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
            off += 4;
        }

        let sorted_nibbles =
            u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());

        Self {
            genesis_hash,
            prev_blockhash,
            nbits,
            target,
            height,
            chain_work,
            epoch_start_timestamp,
            timestamps,
            sorted_nibbles,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            genesis_hash: [0u8; 32],
            prev_blockhash: [0u8; 32],
            nbits: 0,
            target: [0u8; 32],
            height: 0,
            chain_work: [0u64; 4],
            epoch_start_timestamp: 0,
            timestamps: [0u32; WINDOW_SIZE],
            sorted_nibbles: 0,
        }
    }
}

// ============================================================================
// Bitcoin Consensus Helpers
// ============================================================================

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

fn hash_meets_target(hash: &[u8; 32], nbits: u32) -> bool {
    let target = bits_to_target(nbits);
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

fn retarget_target(
    old_target: &[u8; 32],
    actual_timespan: u32,
    expected_timespan: u32,
) -> [u8; 32] {
    let mut old_u64 = [0u64; 4];
    for (i, limb) in old_u64.iter_mut().enumerate() {
        let base = i * 8;
        *limb = u64::from_le_bytes(old_target[base..base + 8].try_into().unwrap());
    }

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

// ============================================================================
// Median Time Past (nibble-packed circular buffer)
// ============================================================================

#[inline]
fn get_nibble(packed: u64, pos: usize) -> u8 {
    ((packed >> (pos * NIBBLE_BITS)) & NIBBLE_MASK) as u8
}

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

fn find_index_position(packed: u64, count: usize, target: usize) -> usize {
    for i in 0..count {
        if get_nibble(packed, i) as usize == target {
            return i;
        }
    }
    count
}

fn remove_nibble(packed: u64, pos: usize, _count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed >> ((pos + 1) * NIBBLE_BITS)) << (pos * NIBBLE_BITS);
    lower | upper
}

fn insert_nibble(packed: u64, pos: usize, val: u8, count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed & !lower_mask) << NIBBLE_BITS;
    let new_packed = lower | ((val as u64) << (pos * NIBBLE_BITS)) | upper;
    let new_mask = (1u64 << ((count + 1) * NIBBLE_BITS)) - 1;
    new_packed & new_mask
}

/// Add timestamp at the given slot and update packed sorted indices.
/// `prev_count` = number of timestamps already in window (before adding this one).
fn add_timestamp(
    timestamps: &mut [u32; WINDOW_SIZE],
    prev_count: usize,
    packed: u64,
    ts: u32,
    slot: usize,
) -> u64 {
    if prev_count < WINDOW_SIZE {
        // Window still growing
        timestamps[slot] = ts;
        let pos = find_insert_position(timestamps, packed, prev_count, ts);
        insert_nibble(packed, pos, slot as u8, prev_count)
    } else {
        // Window full: evict oldest (at slot), insert new
        let pos_old = find_index_position(packed, WINDOW_SIZE, slot);
        let without = remove_nibble(packed, pos_old, WINDOW_SIZE);
        let pos_new = find_insert_position(timestamps, without, WINDOW_SIZE - 1, ts);
        timestamps[slot] = ts;
        insert_nibble(without, pos_new, slot as u8, WINDOW_SIZE - 1)
    }
}

/// Check if timestamp is <= median of the current window.
/// Returns `true` if the timestamp violates the median time past rule.
/// `count` = number of timestamps currently in window (before adding).
fn check_median(timestamps: &[u32; WINDOW_SIZE], packed: u64, count: usize, ts: u32) -> bool {
    if count == 0 {
        return false;
    }
    let median_pos = (count - 1) / 2;
    let idx = get_nibble(packed, median_pos) as usize;
    ts <= timestamps[idx]
}

// ============================================================================
// Header Construction
// ============================================================================

/// Build the full 80-byte Bitcoin block header from authenticated state + prover input.
///
/// | Offset | Field         | Source                    |
/// |--------|---------------|---------------------------|
/// | 0..4   | version       | Prover input              |
/// | 4..36  | prev_blockhash| state.prev_blockhash      |
/// | 36..68 | merkle_root   | Prover input              |
/// | 68..72 | timestamp     | Prover input              |
/// | 72..76 | nbits         | state.nbits               |
/// | 76..80 | nonce         | Prover input              |
fn construct_header(state: &State, new_header: &NewHeader) -> [u8; 80] {
    let mut header = [0u8; 80];
    header[0..4].copy_from_slice(&new_header.version.to_le_bytes());
    header[4..36].copy_from_slice(&state.prev_blockhash);
    header[36..68].copy_from_slice(&new_header.merkle_root);
    header[68..72].copy_from_slice(&new_header.timestamp.to_le_bytes());
    header[72..76].copy_from_slice(&state.nbits.to_le_bytes());
    header[76..80].copy_from_slice(&new_header.nonce.to_le_bytes());
    header
}

// ============================================================================
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
fn commit_error(state: &State, error_code: u8, header_index: u32) -> ! {
    let state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&state_bytes);
    sp1_zkvm::io::commit_slice(&[error_code]);
    sp1_zkvm::io::commit_slice(&header_index.to_le_bytes());
    sp1_zkvm::syscalls::syscall_halt(0);
}

// ============================================================================
// Main Program
// ============================================================================

pub fn main() {
    // --- Read inputs --------------------------------------------------------
    let prev_height = sp1_zkvm::io::read::<u32>();

    // Initialize state: either from previous proof or default (genesis start)
    let mut state = if prev_height > 0 {
        // Read previous proof verification data
        let prev_vk = sp1_zkvm::io::read::<[u32; 8]>();
        let prev_pv_digest = sp1_zkvm::io::read::<[u8; 32]>();
        let prev_pv_bytes = sp1_zkvm::io::read_vec();

        if prev_pv_bytes.len() != STATE_SIZE {
            panic!("Previous proof public values wrong size: expected {}, got {}", STATE_SIZE, prev_pv_bytes.len());
        }

        // In-circuit verification of the previous proof
        sp1_zkvm::lib::verify::verify_sp1_proof(&prev_vk, &prev_pv_digest);

        let s = State::from_bytes(&prev_pv_bytes);

        if s.height != prev_height {
            panic!("Height mismatch in previous state: expected {}, got {}", prev_height, s.height);
        }

        s
    } else {
        State::default()
    };

    if state.height == 0 {
        state.nbits = GENESIS_NBITS;
        state.target = bits_to_target(GENESIS_NBITS);
    }

    let num_headers = sp1_zkvm::io::read::<u32>();
    let headers_bytes = sp1_zkvm::io::read_vec();

    // Validate header byte count: must be num_headers * 44 (NewHeader size)
    if headers_bytes.len() != (num_headers as usize) * NEW_HEADER_SIZE {
        commit_error(&state, STATUS_HEADER_COUNT_MISMATCH, 0);
    }

    // --- Process each header ------------------------------------------------
    for i in 0..num_headers {
        let offset = (i as usize) * NEW_HEADER_SIZE;
        let new_header = NewHeader::from_bytes(&headers_bytes, offset);
        let new_height = state.height + 1;

        // Median timestamp check — always runs when height > 0.
        // Run this before hashing so timestamp violations surface directly.
        let timestamp_count = (state.height as usize).min(WINDOW_SIZE);
        if timestamp_count > 0
            && check_median(&state.timestamps, state.sorted_nibbles, timestamp_count, new_header.timestamp)
        {
            commit_error(&state, STATUS_TIMESTAMP_TOO_OLD, i);
        }

        // Construct the full 80-byte header from authenticated state + prover input
        println!("cycle-tracker-start: sha256d");
        let header = construct_header(&state, &new_header);
        let block_hash = double_sha256_80(&header);
        println!("cycle-tracker-end: sha256d");

        // PoW check: hash must meet target (uses state.nbits directly — no conversion)
        if !hash_meets_target(&block_hash, state.nbits) {
            commit_error(&state, STATUS_POW_INSUFFICIENT, i);
        }

        // Genesis special case: the first constructed header must match Bitcoin mainnet genesis.
        if state.height == 0 {
            if block_hash != MAINNET_GENESIS_HASH_RAW {
                commit_error(&state, STATUS_GENESIS_HASH_MISMATCH, i);
            }
            state.genesis_hash = block_hash;
            state.chain_work = work_from_bits(state.nbits);
            state.epoch_start_timestamp = new_header.timestamp;
            state.timestamps[0] = new_header.timestamp;
            state.sorted_nibbles = 0;
        } else {
            // Median timestamp count (before adding this one)
            // Add timestamp to circular buffer
            let slot = (state.height as usize) % WINDOW_SIZE;
            state.sorted_nibbles = add_timestamp(
                &mut state.timestamps,
                timestamp_count,
                state.sorted_nibbles,
                new_header.timestamp,
                slot,
            );

            // Retarget if this block completes an epoch
            if new_height % 2016 == 0 {
                println!("cycle-tracker-start: retarget");
                let actual_timespan = new_header.timestamp.wrapping_sub(state.epoch_start_timestamp);
                let expected_timespan: u32 = 2016 * 600;
                let clamped = actual_timespan
                    .max(expected_timespan / 4)
                    .min(expected_timespan * 4);
                let new_target = retarget_target(&state.target, clamped, expected_timespan);
                state.nbits = target_to_bits(&new_target);
                state.target = new_target;
                state.epoch_start_timestamp = new_header.timestamp;
                println!("cycle-tracker-end: retarget");
            }

            state.chain_work = u256_add(state.chain_work, work_from_bits(state.nbits));
        }

        // Always update prev_blockhash and height
        state.prev_blockhash = block_hash;
        state.height = new_height;
    }

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&final_state_bytes);
    sp1_zkvm::syscalls::syscall_halt(0);
}
