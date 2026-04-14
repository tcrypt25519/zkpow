//! Utilities for the Bitcoin header chain prover script.

use sha2::{Digest, Sha256};

// ============================================================================
// Constants — must match the program
// ============================================================================

/// Size of the serialized State in bytes.
pub const STATE_SIZE: usize = 236;

const WINDOW_SIZE: usize = 11;
const NIBBLE_BITS: usize = 4;
const NIBBLE_MASK: u64 = 0xF;

// ============================================================================
// State Structure (host-side mirror)
// ============================================================================

/// Host-side mirror of the zkVM State structure.
/// Used for computing expected outputs and inspecting proofs.
#[derive(Clone)]
pub struct State {
    pub genesis_hash: [u8; 32],
    pub prev_header: [u8; 80],
    pub prev_header_hash: [u8; 32],
    pub height: u32,
    pub chain_work: [u64; 4],
    pub epoch_start_timestamp: u32,
    pub timestamps: [u32; WINDOW_SIZE],
    pub sorted_nibbles: u64,
}

impl State {
    /// Serialize to exactly 236 bytes (must match program's to_bytes).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(STATE_SIZE);
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

    /// Deserialize from exactly 236 bytes (must match program's from_bytes).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), STATE_SIZE, "State must be {} bytes", STATE_SIZE);
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

    /// Extract the error code and header index from error output bytes.
    /// Error output = state_bytes(236) + error_code(1) + header_index(4) = 241 bytes.
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

#[allow(dead_code)]
fn bits_from_header(header: &[u8; 80]) -> u32 {
    u32::from_le_bytes(header[72..76].try_into().unwrap())
}

#[allow(dead_code)]
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

/// Check median — returns true if ts <= median (i.e., violation).
#[allow(dead_code)]
fn check_median(timestamps: &[u32; WINDOW_SIZE], packed: u64, count: usize, ts: u32) -> bool {
    if count == 0 {
        return false;
    }
    let median_pos = (count - 1) / 2;
    let idx = get_nibble(packed, median_pos) as usize;
    ts <= timestamps[idx]
}

// ============================================================================
// State Computation (host-side simulation of zkVM logic)
// ============================================================================

/// Simulate the zkVM program locally to compute the expected State after
/// validating a batch of headers, optionally extending from a previous state.
pub fn compute_expected_state(
    _start_height: u32,
    num_headers: u32,
    headers_bytes: &[u8],
    prev_state: Option<&State>,
) -> State {
    let mut state = prev_state.cloned().unwrap_or(State {
        genesis_hash: [0u8; 32],
        prev_header: [0u8; 80],
        prev_header_hash: [0u8; 32],
        height: 0,
        chain_work: [0u64; 4],
        epoch_start_timestamp: 0,
        timestamps: [0u32; WINDOW_SIZE],
        sorted_nibbles: 0,
    });

    for i in 0..num_headers {
        let offset = (i * 80) as usize;
        let header: &[u8; 80] = headers_bytes[offset..offset + 80].try_into().unwrap();
        let new_height = state.height + 1;

        let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
        let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());
        let block_hash = double_sha256_host(header);

        if state.height == 0 {
            // Genesis block
            state.genesis_hash = block_hash;
            state.chain_work = work_from_bits(bits);
            state.epoch_start_timestamp = timestamp;
            state.timestamps[0] = timestamp;
            state.sorted_nibbles = 0;
        } else {
            // Median timestamp count (before adding this one)
            let timestamp_count = (state.height as usize).min(WINDOW_SIZE);

            // Retarget if this block completes an epoch
            if new_height % 2016 == 0 {
                state.epoch_start_timestamp = timestamp;
            }

            // Add timestamp to circular buffer
            let slot = (state.height as usize) % WINDOW_SIZE;
            state.sorted_nibbles = add_timestamp_window(
                &mut state.timestamps,
                timestamp_count,
                state.sorted_nibbles,
                timestamp,
                slot,
            );
            state.chain_work = u256_add(state.chain_work, work_from_bits(bits));
        }

        state.prev_header = *header;
        state.prev_header_hash = block_hash;
        state.height = new_height;
    }

    state
}
