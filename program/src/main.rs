//! Bitcoin Header Chain Prover — IVC State Machine
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! Input: previous height (0 for genesis), optional previous proof + public values,
//!        number of new headers, and the header bytes.
//! Output: serialized State (236 bytes) on success or error.
//! The program verifies the previous proof in-circuit (if height > 0),
//! then validates each new header against Bitcoin consensus rules.

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

const STATUS_PREV_BLOCKHASH_MISMATCH: u8 = 1;
const STATUS_POW_INSUFFICIENT: u8 = 2;
const STATUS_TIMESTAMP_TOO_OLD: u8 = 3;
const STATUS_BITS_MISMATCH: u8 = 4;
const STATUS_HEADER_COUNT_MISMATCH: u8 = 5;
// Reserved: 6, 7, 8, 9, 10

// ============================================================================
// State Structure (236 bytes)
// ============================================================================

/// The complete validation state, persisted between IVC iterations.
///
/// Layout:
///   0..32    genesis_hash        — anchor of the chain (set at height 0)
///  32..112   prev_header         — last validated header (raw 80 bytes)
/// 112..144   prev_header_hash    — cached SHA256d(prev_header), saves 1 hash/iter
/// 144..148   height              — number of validated blocks (0 = none yet)
/// 148..180   chain_work          — 256-bit cumulative work (LE [u64; 4])
/// 180..184   epoch_start_ts      — timestamp of first block in current epoch
/// 184..228   timestamps          — circular buffer of last min(height,11) timestamps
/// 228..236   sorted_nibbles      — packed sorted indices for O(1) median lookup
///
/// Total: 32 + 80 + 32 + 4 + 32 + 4 + 44 + 8 = 236 bytes
#[derive(Clone, Debug, PartialEq)]
struct State {
    genesis_hash: [u8; 32],
    prev_header: [u8; 80],
    prev_header_hash: [u8; 32],
    height: u32,
    chain_work: [u64; 4],
    epoch_start_timestamp: u32,
    timestamps: [u32; WINDOW_SIZE],
    sorted_nibbles: u64,
}

impl State {
    /// Serialize to exactly 236 bytes. No length prefixes — fixed layout.
    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(236);
        out.extend_from_slice(&self.genesis_hash);
        out.extend_from_slice(&self.prev_header);
        out.extend_from_slice(&self.prev_header_hash);
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

    /// Deserialize from exactly 236 bytes.
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut off = 0;

        let genesis_hash: [u8; 32] = bytes[off..off + 32].try_into().unwrap();
        off += 32;

        let prev_header: [u8; 80] = bytes[off..off + 80].try_into().unwrap();
        off += 80;

        let prev_header_hash: [u8; 32] = bytes[off..off + 32].try_into().unwrap();
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
            prev_header,
            prev_header_hash,
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
            prev_header: [0u8; 80],
            prev_header_hash: [0u8; 32],
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

fn bits_from_header(header: &[u8; 80]) -> u32 {
    u32::from_le_bytes(header[72..76].try_into().unwrap())
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
/// Returns `true` if the timestamp violates the median time past rule
/// (i.e., it is not strictly greater than the median).
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
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
/// This produces a valid proof that "validation failed at header X for reason Y".
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

        if prev_pv_bytes.len() != 236 {
            // We can't commit a proper error here because we haven't established
            // a valid state yet. Panic is acceptable — this is a protocol violation.
            panic!("Previous proof public values wrong size: expected 236, got {}", prev_pv_bytes.len());
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

    let num_headers = sp1_zkvm::io::read::<u32>();
    let headers_bytes = sp1_zkvm::io::read_vec();

    // Validate header byte count
    if headers_bytes.len() != (num_headers as usize) * 80 {
        commit_error(&state, STATUS_HEADER_COUNT_MISMATCH, 0);
    }

    // --- Process each header ------------------------------------------------
    for i in 0..num_headers {
        let offset = (i * 80) as usize;
        let header: &[u8; 80] = &headers_bytes[offset..offset + 80].try_into().unwrap();
        let new_height = state.height + 1;

        // Parse header fields
        let prev_blockhash: [u8; 32] = header[4..36].try_into().unwrap();
        let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
        let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());

        // Validate bits range: exponent must be 3..=29 for a valid target
        let exponent = bits >> 24;
        if !(3..=29).contains(&exponent) {
            commit_error(&state, STATUS_BITS_MISMATCH, i);
        }

        // Early PoW check (cheapest rejection — hash + comparison)
        println!("cycle-tracker-start: sha256d");
        let block_hash = double_sha256_80(header);
        println!("cycle-tracker-end: sha256d");
        if !hash_meets_target(&block_hash, bits) {
            commit_error(&state, STATUS_POW_INSUFFICIENT, i);
        }

        // Chain linkage check (skip for genesis block at height 0)
        if state.height > 0 && prev_blockhash != state.prev_header_hash {
            commit_error(&state, STATUS_PREV_BLOCKHASH_MISMATCH, i);
        }

        // Median timestamp check — always runs when height > 0
        // count = min(height, 11); median computed from last `count` timestamps
        let timestamp_count = (state.height as usize).min(WINDOW_SIZE);
        if timestamp_count > 0
            && check_median(&state.timestamps, state.sorted_nibbles, timestamp_count, timestamp)
        {
            commit_error(&state, STATUS_TIMESTAMP_TOO_OLD, i);
        }

        // Compute expected bits (carry over or retarget)
        let expected_bits = if state.height == 0 {
            // Genesis: any bits are allowed (PoW already checked)
            bits
        } else if new_height % 2016 == 0 {
            // This block completes an epoch → retarget
            println!("cycle-tracker-start: retarget");
            let actual_timespan = timestamp.wrapping_sub(state.epoch_start_timestamp);
            let expected_timespan: u32 = 2016 * 600;
            let clamped = actual_timespan
                .max(expected_timespan / 4)
                .min(expected_timespan * 4);
            let old_target = bits_to_target(bits_from_header(&state.prev_header));
            let new_target = retarget_target(&old_target, clamped, expected_timespan);
            // Next epoch starts with this block
            state.epoch_start_timestamp = timestamp;
            println!("cycle-tracker-end: retarget");
            target_to_bits(&new_target)
        } else {
            // Same epoch: copy bits from previous header
            bits_from_header(&state.prev_header)
        };

        if bits != expected_bits {
            commit_error(&state, STATUS_BITS_MISMATCH, i);
        }

        // --- Update state (all checks passed) --------------------------------
        if state.height == 0 {
            // Genesis block: set chain anchor and initial values
            state.genesis_hash = block_hash;
            state.chain_work = work_from_bits(bits);
            state.epoch_start_timestamp = timestamp;
            state.timestamps[0] = timestamp;
            state.sorted_nibbles = 0; // single element
        } else {
            // Normal update: add timestamp to circular buffer
            let slot = (state.height as usize) % WINDOW_SIZE;
            state.sorted_nibbles = add_timestamp(
                &mut state.timestamps,
                timestamp_count,
                state.sorted_nibbles,
                timestamp,
                slot,
            );
            state.chain_work = u256_add(state.chain_work, work_from_bits(bits));
        }

        // Always update header and height
        state.prev_header = *header;
        state.prev_header_hash = block_hash;
        state.height = new_height;
    }

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&final_state_bytes);
    sp1_zkvm::syscalls::syscall_halt(0);
}
