//! Utilities for the Bitcoin header chain prover script.
//!
//! Host-side mirror of the zkVM program with header-construction architecture.
//! The prover supplies 44-byte NewHeader structs (version, merkle_root, timestamp, nonce).
//! The host constructs full 80-byte headers from state + NewHeader, matching the circuit.

use sha2::{Digest, Sha256};

// ============================================================================
// Constants — must match the program
// ============================================================================

/// Size of the serialized State in bytes.
pub const STATE_SIZE: usize = 192;

/// Size of each NewHeader input from the prover.
pub const NEW_HEADER_SIZE: usize = 44;
const GENESIS_NBITS: u32 = 0x1d00ffff;
const MAINNET_GENESIS_HASH_RAW: [u8; 32] = [
    0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72,
    0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f,
    0x93, 0x1e, 0x83, 0x65, 0xe1, 0x5a, 0x08, 0x9c,
    0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const WINDOW_SIZE: usize = 11;
const NIBBLE_BITS: usize = 4;
const NIBBLE_MASK: u64 = 0xF;

// ============================================================================
// NewHeader — Prover-supplied fields (44 bytes)
// ============================================================================

/// A new header as supplied by the prover — only non-deterministic fields.
/// prev_blockhash and nbits are constructed from authenticated state.
pub struct NewHeader {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub nonce: u32,
}

impl NewHeader {
    /// Parse a NewHeader from 44 bytes at the given offset in a flat buffer.
    pub fn from_bytes(data: &[u8], offset: usize) -> Self {
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

    /// Construct a NewHeader from a full 80-byte raw header (for loading from DB).
    pub fn from_raw_header(raw: &[u8; 80]) -> Self {
        let version = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(&raw[36..68]);
        let timestamp = u32::from_le_bytes(raw[68..72].try_into().unwrap());
        let nonce = u32::from_le_bytes(raw[76..80].try_into().unwrap());
        Self {
            version,
            merkle_root,
            timestamp,
            nonce,
        }
    }
}

// ============================================================================
// State Structure (host-side mirror, 192 bytes)
// ============================================================================

/// Host-side mirror of the zkVM State structure.
///
/// Layout:
///   0..32    genesis_hash
///  32..64    prev_blockhash
///  64..68    nbits
///  68..100   target
/// 100..104   height
/// 104..136   chain_work
/// 136..140   epoch_start_ts
/// 140..184   timestamps
/// 184..192   sorted_nibbles
#[derive(Clone)]
pub struct State {
    pub genesis_hash: [u8; 32],
    pub prev_blockhash: [u8; 32],
    pub nbits: u32,
    pub target: [u8; 32],
    pub height: u32,
    pub chain_work: [u64; 4],
    pub epoch_start_timestamp: u32,
    pub timestamps: [u32; WINDOW_SIZE],
    pub sorted_nibbles: u64,
}

impl State {
    /// Serialize to exactly 192 bytes (must match program's to_bytes).
    pub fn to_bytes(&self) -> Vec<u8> {
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

    /// Deserialize from exactly 192 bytes (must match program's from_bytes).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), STATE_SIZE, "State must be {} bytes", STATE_SIZE);
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

    /// Extract the error code and header index from error output bytes.
    /// Error output = state_bytes(192) + error_code(1) + header_index(4) = 197 bytes.
    pub fn parse_error(error_bytes: &[u8]) -> Option<(Self, u8, u32)> {
        if error_bytes.len() != STATE_SIZE + 1 + 4 {
            return None;
        }
        let state = Self::from_bytes(&error_bytes[..STATE_SIZE]);
        let error_code = error_bytes[STATE_SIZE];
        let header_index = u32::from_le_bytes(
            error_bytes[STATE_SIZE + 1..STATE_SIZE + 5].try_into().unwrap()
        );
        Some((state, error_code, header_index))
    }
}

// ============================================================================
// Database & I/O
// ============================================================================

/// Load raw 80-byte concatenated headers from the SQLite database.
pub fn load_headers_from_db(db_path: &str, start_height: u64, count: u64) -> Vec<u8> {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT raw_header FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .expect("failed to prepare SQL statement");

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .expect("failed to execute query");

    let mut all_headers = Vec::with_capacity((count * 80) as usize);
    let mut loaded = 0u64;

    for row_result in rows {
        let header_bytes: Vec<u8> = row_result.expect("failed to read raw_header from database");
        assert_eq!(
            header_bytes.len(),
            80,
            "Expected 80-byte header at height {}, got {} bytes",
            start_height + loaded,
            header_bytes.len(),
        );
        all_headers.extend_from_slice(&header_bytes);
        loaded += 1;
    }

    assert_eq!(
        loaded, count,
        "Expected to load {} headers, but only loaded {} from database",
        count, loaded,
    );

    all_headers
}

/// Convert raw 80-byte headers (from DB) to 44-byte NewHeader format (for zkVM input).
pub fn raw_headers_to_new_headers(raw_headers: &[u8]) -> Vec<u8> {
    assert_eq!(raw_headers.len() % 80, 0, "raw_headers must be a multiple of 80 bytes");
    let count = raw_headers.len() / 80;
    let mut out = Vec::with_capacity(count * NEW_HEADER_SIZE);
    for i in 0..count {
        let offset = i * 80;
        let raw: [u8; 80] = raw_headers[offset..offset + 80].try_into().unwrap();
        let nh = NewHeader::from_raw_header(&raw);
        out.extend_from_slice(&nh.version.to_le_bytes());
        out.extend_from_slice(&nh.merkle_root);
        out.extend_from_slice(&nh.timestamp.to_le_bytes());
        out.extend_from_slice(&nh.nonce.to_le_bytes());
    }
    out
}

// ============================================================================
// SHA-256 (host-side)
// ============================================================================

/// Compute double SHA-256 of the given data (host-side).
pub fn double_sha256_host(data: &[u8]) -> [u8; 32] {
    let inner = Sha256::digest(data);
    let outer = Sha256::digest(inner);
    outer.into()
}

/// Compute SHA-256 digest (host-side).
pub fn compute_pv_digest(committed_bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(committed_bytes).into()
}

// ============================================================================
// Bitcoin Consensus Math (host-side, identical to program)
// ============================================================================

pub fn bits_to_target(bits: u32) -> [u8; 32] {
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

/// Convert compact "bits" to work = floor(2^256 / (target + 1)) as [u64; 4] LE.
pub fn work_from_bits(bits: u32) -> [u64; 4] {
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

/// Add two u256 values represented as [u64; 4] little-endian arrays.
pub fn u256_add(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
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
// Header Construction (host-side, identical to program)
// ============================================================================

/// Build the full 80-byte Bitcoin block header from authenticated state + NewHeader.
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
// Median Timestamp Window (host-side, identical to program)
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

fn add_timestamp_window(
    timestamps: &mut [u32; WINDOW_SIZE],
    prev_count: usize,
    packed: u64,
    ts: u32,
    slot: usize,
) -> u64 {
    if prev_count < WINDOW_SIZE {
        timestamps[slot] = ts;
        let pos = find_insert_position(timestamps, packed, prev_count, ts);
        insert_nibble(packed, pos, slot as u8, prev_count)
    } else {
        let pos_old = find_index_position(packed, WINDOW_SIZE, slot);
        let without = remove_nibble(packed, pos_old, WINDOW_SIZE);
        let pos_new = find_insert_position(timestamps, without, WINDOW_SIZE - 1, ts);
        timestamps[slot] = ts;
        insert_nibble(without, pos_new, slot as u8, WINDOW_SIZE - 1)
    }
}

// ============================================================================
// State Computation (host-side simulation of zkVM logic)
// ============================================================================

/// Simulate the zkVM program locally to compute the expected State after
/// validating a batch of headers, optionally extending from a previous state.
///
/// `headers_bytes` must be in NewHeader format (44 bytes per header).
pub fn compute_expected_state(
    _start_height: u32,
    num_headers: u32,
    new_headers_bytes: &[u8],
    prev_state: Option<&State>,
) -> State {
    let mut state = prev_state.cloned().unwrap_or(State {
        genesis_hash: [0u8; 32],
        prev_blockhash: [0u8; 32],
        nbits: GENESIS_NBITS,
        target: bits_to_target(GENESIS_NBITS),
        height: 0,
        chain_work: [0u64; 4],
        epoch_start_timestamp: 0,
        timestamps: [0u32; WINDOW_SIZE],
        sorted_nibbles: 0,
    });

    for i in 0..num_headers {
        let offset = (i as usize) * NEW_HEADER_SIZE;
        let new_header = NewHeader::from_bytes(new_headers_bytes, offset);
        let new_height = state.height + 1;

        // Construct the full header (matching the circuit)
        let header = construct_header(&state, &new_header);
        let block_hash = double_sha256_host(&header);

        if state.height == 0 {
            assert_eq!(
                block_hash,
                MAINNET_GENESIS_HASH_RAW,
                "constructed genesis header does not match mainnet genesis hash",
            );
            // Genesis block
            state.genesis_hash = block_hash;
            state.chain_work = work_from_bits(state.nbits);
            state.epoch_start_timestamp = new_header.timestamp;
            state.timestamps[0] = new_header.timestamp;
            state.sorted_nibbles = 0;
        } else {
            // Median timestamp count (before adding this one)
            let timestamp_count = (state.height as usize).min(WINDOW_SIZE);

            // Retarget if this block completes an epoch
            if new_height % 2016 == 0 {
                let actual_timespan = new_header.timestamp.wrapping_sub(state.epoch_start_timestamp);
                let expected_timespan: u32 = 2016 * 600;
                let clamped = actual_timespan
                    .max(expected_timespan / 4)
                    .min(expected_timespan * 4);
                let new_target = retarget_target(&state.target, clamped, expected_timespan);
                state.nbits = target_to_bits(&new_target);
                state.target = new_target;
                state.epoch_start_timestamp = new_header.timestamp;
            }

            // Add timestamp to circular buffer
            let slot = (state.height as usize) % WINDOW_SIZE;
            state.sorted_nibbles = add_timestamp_window(
                &mut state.timestamps,
                timestamp_count,
                state.sorted_nibbles,
                new_header.timestamp,
                slot,
            );
            state.chain_work = u256_add(state.chain_work, work_from_bits(state.nbits));
        }

        state.prev_blockhash = block_hash;
        state.height = new_height;
    }

    state
}

/// Retarget computation (host-side, identical to program).
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
