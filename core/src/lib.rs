//! Shared consensus types and pure helper logic for the Bitcoin header chain prover.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

/// Size of the serialized [`State`] in bytes.
pub const STATE_SIZE: usize = 192;

/// Size of each [`NewHeader`] input from the prover.
pub const NEW_HEADER_SIZE: usize = 44;

/// Sliding window size used for median-time-past checks.
pub const WINDOW_SIZE: usize = 11;

/// Packed nibble width used for sorted timestamp indices.
pub const NIBBLE_BITS: usize = 4;

/// Bitmask for a single packed nibble.
pub const NIBBLE_MASK: u64 = 0xF;

/// Mainnet PoW limit in compact form.
pub const GENESIS_NBITS: u32 = 0x1d00ffff;

/// Raw little-endian mainnet genesis block hash.
pub const MAINNET_GENESIS_HASH_RAW: [u8; 32] = [
    0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72, 0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f,
    0x93, 0x1e, 0x83, 0x65, 0xe1, 0x5a, 0x08, 0x9c, 0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Prover-supplied fields for a new header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHeader {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub nonce: u32,
}

impl NewHeader {
    /// Parse a [`NewHeader`] from the flat input buffer at `offset`.
    #[must_use]
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

    /// Construct a [`NewHeader`] from a full raw 80-byte Bitcoin header.
    #[must_use]
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

/// Complete authenticated validation state, serialized between recursive iterations.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
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

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
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
        for limb in &mut chain_work {
            *limb = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            off += 8;
        }

        let epoch_start_timestamp = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;

        let mut timestamps = [0u32; WINDOW_SIZE];
        for ts in &mut timestamps {
            *ts = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
            off += 4;
        }

        let sorted_nibbles = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());

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

/// Validation status emitted by the program on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValidationErrorCode {
    HeaderCountMismatch = 1,
    PowInsufficient = 2,
    TimestampTooOld = 3,
    GenesisHashMismatch = 4,
}

impl ValidationErrorCode {
    /// Return the committed byte representation for the error code.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    /// Return a human-readable description for the code.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::HeaderCountMismatch => "Header count mismatch",
            Self::PowInsufficient => "PoW insufficient",
            Self::TimestampTooOld => "Timestamp too old",
            Self::GenesisHashMismatch => "Genesis hash mismatch",
        }
    }
}

impl core::fmt::Display for ValidationErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.description())
    }
}

impl TryFrom<u8> for ValidationErrorCode {
    type Error = PublicValuesParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HeaderCountMismatch),
            2 => Ok(Self::PowInsufficient),
            3 => Ok(Self::TimestampTooOld),
            4 => Ok(Self::GenesisHashMismatch),
            _ => Err(PublicValuesParseError::UnknownErrorCode { code: value }),
        }
    }
}

/// Failure payload committed by the program when validation stops early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFailure {
    pub last_valid_state: State,
    pub error_code: ValidationErrorCode,
    pub header_index: u32,
}

/// Typed public values committed by the prover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderChainPublicValues {
    Success(State),
    Failure(ProofFailure),
}

impl HeaderChainPublicValues {
    /// Parse committed public values into the typed representation.
    pub fn parse(bytes: &[u8]) -> Result<Self, PublicValuesParseError> {
        match bytes.len() {
            STATE_SIZE => Ok(Self::Success(State::from_bytes(bytes))),
            len if len == STATE_SIZE + 1 + 4 => {
                let state = State::from_bytes(&bytes[..STATE_SIZE]);
                let error_code = ValidationErrorCode::try_from(bytes[STATE_SIZE])?;
                let header_index =
                    u32::from_le_bytes(bytes[STATE_SIZE + 1..STATE_SIZE + 5].try_into().unwrap());
                Ok(Self::Failure(ProofFailure {
                    last_valid_state: state,
                    error_code,
                    header_index,
                }))
            }
            actual => Err(PublicValuesParseError::InvalidLength { actual }),
        }
    }

    /// Borrow the last authenticated state regardless of success or failure.
    #[must_use]
    pub fn state(&self) -> &State {
        match self {
            Self::Success(state) => state,
            Self::Failure(failure) => &failure.last_valid_state,
        }
    }
}

/// Parse errors for committed public values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicValuesParseError {
    InvalidLength { actual: usize },
    UnknownErrorCode { code: u8 },
}

impl core::fmt::Display for PublicValuesParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    f,
                    "invalid public values length: expected {} or {}, got {}",
                    STATE_SIZE,
                    STATE_SIZE + 1 + 4,
                    actual
                )
            }
            Self::UnknownErrorCode { code } => {
                write!(f, "unknown validation error code: {}", code)
            }
        }
    }
}

/// Convert compact `bits` encoding into a 256-bit target.
#[must_use]
pub fn bits_to_target(bits: u32) -> [u8; 32] {
    let exponent = bits >> 24;
    let mantissa = bits & 0x00ff_ffff;
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

/// Convert a full 256-bit target into compact `bits` encoding.
#[must_use]
pub fn target_to_bits(target: &[u8; 32]) -> u32 {
    let mut high_byte = 31usize;
    while high_byte > 0 && target[high_byte] == 0 {
        high_byte -= 1;
    }
    if target[high_byte] == 0 {
        return 0;
    }
    let bit_length = high_byte * 8 + (8 - target[high_byte].leading_zeros() as usize);
    let nbytes = bit_length.div_ceil(8);
    let mantissa = if high_byte >= 2 {
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
    ((nbytes as u32) << 24) | (mantissa & 0x00ff_ffff)
}

/// Return `true` when `lhs` is strictly greater than `rhs` as unsigned 256-bit integers.
#[must_use]
pub fn target_exceeds(lhs: &[u8; 32], rhs: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if lhs[i] > rhs[i] {
            return true;
        }
        if lhs[i] < rhs[i] {
            return false;
        }
    }
    false
}

/// Check whether a header hash satisfies the compact target in `nbits`.
#[must_use]
pub fn hash_meets_target(hash: &[u8; 32], nbits: u32) -> bool {
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

/// Add two little-endian `u256` values.
#[must_use]
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

/// Return `true` when `timestamp` violates median-time-past for the current window.
#[must_use]
pub fn check_median_timestamp(
    timestamps: &[u32; WINDOW_SIZE],
    packed: u64,
    count: usize,
    timestamp: u32,
) -> bool {
    if count == 0 {
        return false;
    }
    let median_pos = (count - 1) / 2;
    let idx = get_nibble(packed, median_pos) as usize;
    timestamp <= timestamps[idx]
}

/// Insert a new timestamp into the circular window and update the packed sort order.
#[must_use]
pub fn add_timestamp_window(
    timestamps: &mut [u32; WINDOW_SIZE],
    prev_count: usize,
    packed: u64,
    timestamp: u32,
    slot: usize,
) -> u64 {
    if prev_count < WINDOW_SIZE {
        timestamps[slot] = timestamp;
        let pos = find_insert_position(timestamps, packed, prev_count, timestamp);
        insert_nibble(packed, pos, slot as u8, prev_count)
    } else {
        let pos_old = find_index_position(packed, WINDOW_SIZE, slot);
        let without = remove_nibble(packed, pos_old);
        let pos_new = find_insert_position(timestamps, without, WINDOW_SIZE - 1, timestamp);
        timestamps[slot] = timestamp;
        insert_nibble(without, pos_new, slot as u8, WINDOW_SIZE - 1)
    }
}

/// Compute a retargeted 256-bit target from the previous target and measured timespan.
#[must_use]
pub fn retarget_target(
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

/// Convert compact `bits` into cumulative-work units.
#[must_use]
pub fn work_from_bits(bits: u32) -> [u64; 4] {
    let exponent = bits >> 24;
    let mantissa = bits & 0x00ff_ffff;
    let k = 8 * (exponent - 3);
    let n = 256 - k;

    if mantissa == 0 || n == 0 {
        return [0; 4];
    }

    let r = pow_mod_2(n, mantissa);
    let q = div_2n_minus_r_by_u32(n, r, mantissa);

    let mut work = q;
    if !q_le_r_shifted(&q, r, k) {
        for limb in &mut work {
            if *limb > 0 {
                *limb -= 1;
                break;
            }
            *limb = u64::MAX;
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
    for limb in limbs.iter_mut().take(bit_limb.min(4)).skip(1) {
        *limb = u64::MAX;
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

#[inline]
fn get_nibble(packed: u64, pos: usize) -> u8 {
    ((packed >> (pos * NIBBLE_BITS)) & NIBBLE_MASK) as u8
}

fn find_insert_position(
    timestamps: &[u32; WINDOW_SIZE],
    packed: u64,
    count: usize,
    timestamp: u32,
) -> usize {
    for i in 0..count {
        let idx = get_nibble(packed, i) as usize;
        if timestamp < timestamps[idx] {
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

fn remove_nibble(packed: u64, pos: usize) -> u64 {
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
