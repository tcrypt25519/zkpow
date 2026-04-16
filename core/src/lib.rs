//! Shared consensus types and pure helper logic for the Bitcoin header chain prover.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

/// Size of the serialized [`State`] in bytes.
pub const STATE_SIZE: usize = 240;

/// Size of each [`NewHeader`] input from the prover.
pub const NEW_HEADER_SIZE: usize = 44;

/// Size of a serialized Bitcoin block header in bytes.
pub const BLOCK_HEADER_SIZE: usize = 80;

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

/// Bitcoin block hash stored in Bitcoin's internal little-endian byte order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockHash([u8; 32]);

impl BlockHash {
    /// Construct from raw little-endian bytes.
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    /// Borrow the raw little-endian bytes.
    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consume into raw little-endian bytes.
    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.0
    }
}

impl From<[u8; 32]> for BlockHash {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<BlockHash> for [u8; 32] {
    fn from(value: BlockHash) -> Self {
        value.0
    }
}

/// Bitcoin compact difficulty encoding (`nBits`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactTarget(u32);

impl CompactTarget {
    /// Construct directly from the compact representation.
    #[must_use]
    pub const fn from_consensus(bits: u32) -> Self {
        Self(bits)
    }

    /// Return the compact consensus encoding.
    #[must_use]
    pub const fn to_consensus(self) -> u32 {
        self.0
    }
}

impl From<u32> for CompactTarget {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<CompactTarget> for u32 {
    fn from(value: CompactTarget) -> Self {
        value.0
    }
}

/// Expanded 256-bit proof-of-work target in little-endian byte order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target([u8; 32]);

impl Target {
    /// Construct from raw little-endian bytes.
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    /// Borrow the raw little-endian bytes.
    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consume into raw little-endian bytes.
    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.0
    }
}

impl From<[u8; 32]> for Target {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<Target> for [u8; 32] {
    fn from(value: Target) -> Self {
        value.0
    }
}

/// Cumulative proof-of-work as a 256-bit little-endian limb array.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainWork([u64; 4]);

impl ChainWork {
    /// Construct from little-endian limbs.
    #[must_use]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Self(limbs)
    }

    /// Borrow the little-endian limbs.
    #[must_use]
    pub const fn as_limbs(&self) -> &[u64; 4] {
        &self.0
    }

    /// Consume into little-endian limbs.
    #[must_use]
    pub const fn into_limbs(self) -> [u64; 4] {
        self.0
    }
}

impl From<[u64; 4]> for ChainWork {
    fn from(value: [u64; 4]) -> Self {
        Self(value)
    }
}

impl From<ChainWork> for [u64; 4] {
    fn from(value: ChainWork) -> Self {
        value.0
    }
}

/// Bitcoin block timestamp encoded as Unix seconds in consensus serialization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockTimestamp(u32);

impl BlockTimestamp {
    /// Construct directly from the consensus timestamp.
    #[must_use]
    pub const fn from_consensus(timestamp: u32) -> Self {
        Self(timestamp)
    }

    /// Return the raw consensus timestamp.
    #[must_use]
    pub const fn to_consensus(self) -> u32 {
        self.0
    }

    /// Wrapping subtraction matching Bitcoin's timestamp arithmetic.
    #[must_use]
    pub const fn wrapping_sub(self, other: Self) -> u32 {
        self.0.wrapping_sub(other.0)
    }
}

impl From<u32> for BlockTimestamp {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<BlockTimestamp> for u32 {
    fn from(value: BlockTimestamp) -> Self {
        value.0
    }
}

/// A Bitcoin block header with typed fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub prev_blockhash: BlockHash,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nbits: CompactTarget,
    pub nonce: u32,
}

impl Header {
    /// Serialize to exactly [`BLOCK_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; BLOCK_HEADER_SIZE] {
        let mut bytes = [0u8; BLOCK_HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.version.to_le_bytes());
        bytes[4..36].copy_from_slice(self.prev_blockhash.as_raw());
        bytes[36..68].copy_from_slice(&self.merkle_root);
        bytes[68..72].copy_from_slice(&self.timestamp.to_consensus().to_le_bytes());
        bytes[72..76].copy_from_slice(&self.nbits.to_consensus().to_le_bytes());
        bytes[76..80].copy_from_slice(&self.nonce.to_le_bytes());
        bytes
    }

    /// Deserialize from exactly [`BLOCK_HEADER_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() != BLOCK_HEADER_SIZE {
            return Err(ParseError::InvalidLength {
                expected: BLOCK_HEADER_SIZE,
                actual: bytes.len(),
            });
        }

        let mut off = 0;
        let version = u32::from_le_bytes(take::<4>(bytes, &mut off)?);
        let prev_blockhash = BlockHash::from_raw(take::<32>(bytes, &mut off)?);
        let merkle_root = take::<32>(bytes, &mut off)?;
        let timestamp =
            BlockTimestamp::from_consensus(u32::from_le_bytes(take::<4>(bytes, &mut off)?));
        let nbits = CompactTarget::from_consensus(u32::from_le_bytes(take::<4>(bytes, &mut off)?));
        let nonce = u32::from_le_bytes(take::<4>(bytes, &mut off)?);

        Ok(Self {
            version,
            prev_blockhash,
            merkle_root,
            timestamp,
            nbits,
            nonce,
        })
    }
}

/// Digest of a program verifier key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerifierKeyDigest([u32; 8]);

impl VerifierKeyDigest {
    #[must_use]
    pub const fn from_raw(raw: [u32; 8]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_raw(&self) -> &[u32; 8] {
        &self.0
    }

    #[must_use]
    pub const fn into_raw(self) -> [u32; 8] {
        self.0
    }
}

impl From<[u32; 8]> for VerifierKeyDigest {
    fn from(value: [u32; 8]) -> Self {
        Self(value)
    }
}

impl From<VerifierKeyDigest> for [u32; 8] {
    fn from(value: VerifierKeyDigest) -> Self {
        value.0
    }
}

/// Digest of committed public values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublicValuesDigest([u8; 32]);

impl PublicValuesDigest {
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.0
    }
}

impl From<[u8; 32]> for PublicValuesDigest {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<PublicValuesDigest> for [u8; 32] {
    fn from(value: PublicValuesDigest) -> Self {
        value.0
    }
}

/// Parse errors for fixed-width serialized core types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidLength {
        expected: usize,
        actual: usize,
    },
    Truncated {
        offset: usize,
        needed: usize,
        actual: usize,
    },
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(f, "invalid length: expected {}, got {}", expected, actual)
            }
            Self::Truncated {
                offset,
                needed,
                actual,
            } => {
                write!(
                    f,
                    "truncated input at offset {}: need {} bytes, got {}",
                    offset, needed, actual
                )
            }
        }
    }
}

fn take<const N: usize>(data: &[u8], off: &mut usize) -> Result<[u8; N], ParseError> {
    let start = *off;
    let end = start.checked_add(N).ok_or(ParseError::Truncated {
        offset: start,
        needed: N,
        actual: data.len().saturating_sub(start),
    })?;
    let bytes = data.get(start..end).ok_or(ParseError::Truncated {
        offset: start,
        needed: N,
        actual: data.len().saturating_sub(start),
    })?;
    *off = end;
    bytes.try_into().map_err(|_| ParseError::Truncated {
        offset: start,
        needed: N,
        actual: data.len().saturating_sub(start),
    })
}

/// Prover-supplied fields for a new header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewHeader {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nonce: u32,
}

impl NewHeader {
    /// Parse a [`NewHeader`] from the flat input buffer at `offset`.
    pub fn parse_at(data: &[u8], offset: usize) -> Result<Self, ParseError> {
        let end = offset
            .checked_add(NEW_HEADER_SIZE)
            .ok_or(ParseError::Truncated {
                offset,
                needed: NEW_HEADER_SIZE,
                actual: data.len().saturating_sub(offset),
            })?;
        let bytes = data.get(offset..end).ok_or(ParseError::Truncated {
            offset,
            needed: NEW_HEADER_SIZE,
            actual: data.len().saturating_sub(offset),
        })?;

        let version = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(&bytes[4..36]);
        let timestamp =
            BlockTimestamp::from_consensus(u32::from_le_bytes(bytes[36..40].try_into().unwrap()));
        let nonce = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
        Ok(Self {
            version,
            merkle_root,
            timestamp,
            nonce,
        })
    }

    /// Serialize to exactly [`NEW_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; NEW_HEADER_SIZE] {
        let mut bytes = [0u8; NEW_HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.version.to_le_bytes());
        bytes[4..36].copy_from_slice(&self.merkle_root);
        bytes[36..40].copy_from_slice(&self.timestamp.to_consensus().to_le_bytes());
        bytes[40..44].copy_from_slice(&self.nonce.to_le_bytes());
        bytes
    }

    /// Materialize a full [`Header`] using authenticated chain context.
    #[must_use]
    pub fn into_header(self, prev_blockhash: BlockHash, nbits: CompactTarget) -> Header {
        Header {
            version: self.version,
            prev_blockhash,
            merkle_root: self.merkle_root,
            timestamp: self.timestamp,
            nbits,
            nonce: self.nonce,
        }
    }

    /// Construct a [`NewHeader`] from a full [`Header`].
    #[must_use]
    pub fn from_header(header: &Header) -> Self {
        Self {
            version: header.version,
            merkle_root: header.merkle_root,
            timestamp: header.timestamp,
            nonce: header.nonce,
        }
    }

    /// Construct a [`NewHeader`] from a full raw 80-byte Bitcoin header.
    #[must_use]
    pub fn from_raw_header(raw: &[u8; 80]) -> Self {
        let header = Header::parse(raw).expect("raw Bitcoin header should parse");
        Self::from_header(&header)
    }
}

/// Complete authenticated validation state, serialized between recursive iterations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub header: Header,
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub next_nbits: CompactTarget,
    pub height: u32,
    pub chain_work: ChainWork,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
    pub sorted_nibbles: u64,
}

impl State {
    /// The number of timestamps currently tracked for median-time-past.
    #[must_use]
    pub fn timestamp_count(&self) -> usize {
        (self.height as usize).min(WINDOW_SIZE)
    }

    /// The circular-buffer slot where the next timestamp should be written.
    #[must_use]
    fn next_timestamp_slot(&self) -> usize {
        (self.height as usize) % WINDOW_SIZE
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; STATE_SIZE] {
        let mut out = [0u8; STATE_SIZE];
        let mut off = 0;

        out[off..off + BLOCK_HEADER_SIZE].copy_from_slice(&self.header.to_bytes());
        off += BLOCK_HEADER_SIZE;
        out[off..off + 32].copy_from_slice(self.block_hash.as_raw());
        off += 32;
        out[off..off + 32].copy_from_slice(self.genesis_hash.as_raw());
        off += 32;
        out[off..off + 4].copy_from_slice(&self.next_nbits.to_consensus().to_le_bytes());
        off += 4;
        out[off..off + 4].copy_from_slice(&self.height.to_le_bytes());
        off += 4;
        for &limb in self.chain_work.as_limbs() {
            out[off..off + 8].copy_from_slice(&limb.to_le_bytes());
            off += 8;
        }
        out[off..off + 4].copy_from_slice(&self.epoch_start_timestamp.to_consensus().to_le_bytes());
        off += 4;
        for &ts in &self.timestamps {
            out[off..off + 4].copy_from_slice(&ts.to_consensus().to_le_bytes());
            off += 4;
        }
        out[off..off + 8].copy_from_slice(&self.sorted_nibbles.to_le_bytes());
        out
    }

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() != STATE_SIZE {
            return Err(ParseError::InvalidLength {
                expected: STATE_SIZE,
                actual: bytes.len(),
            });
        }

        let mut off = 0;
        let header = Header::parse(&take::<BLOCK_HEADER_SIZE>(bytes, &mut off)?)?;
        let block_hash = BlockHash::from_raw(take::<32>(bytes, &mut off)?);
        let genesis_hash = BlockHash::from_raw(take::<32>(bytes, &mut off)?);
        let next_nbits =
            CompactTarget::from_consensus(u32::from_le_bytes(take::<4>(bytes, &mut off)?));
        let height = u32::from_le_bytes(take::<4>(bytes, &mut off)?);

        let mut chain_work = [0u64; 4];
        for limb in &mut chain_work {
            *limb = u64::from_le_bytes(take::<8>(bytes, &mut off)?);
        }

        let epoch_start_timestamp =
            BlockTimestamp::from_consensus(u32::from_le_bytes(take::<4>(bytes, &mut off)?));

        let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
        for ts in &mut timestamps {
            *ts = BlockTimestamp::from_consensus(u32::from_le_bytes(take::<4>(bytes, &mut off)?));
        }

        let sorted_nibbles = u64::from_le_bytes(take::<8>(bytes, &mut off)?);

        Ok(Self {
            header,
            block_hash,
            genesis_hash,
            next_nbits,
            height,
            chain_work: ChainWork::from_limbs(chain_work),
            epoch_start_timestamp,
            timestamps,
            sorted_nibbles,
        })
    }

    /// The expanded proof-of-work target required for the next header.
    #[must_use]
    pub fn next_target(&self) -> Target {
        bits_to_target(self.next_nbits)
    }

    /// Return the median time past for the currently tracked timestamps.
    #[must_use]
    pub fn median_time_past(&self) -> Option<BlockTimestamp> {
        let count = self.timestamp_count();
        if count == 0 {
            return None;
        }

        let median_pos = (count - 1) / 2;
        let idx = get_nibble(self.sorted_nibbles, median_pos) as usize;
        Some(self.timestamps[idx])
    }

    /// Fill in the authenticated genesis hashes from the genesis header itself.
    #[must_use]
    pub fn bootstrap_genesis<F>(mut self, hash_header: F) -> Self
    where
        F: FnOnce(&Header) -> BlockHash,
    {
        let block_hash = hash_header(&self.header);
        self.block_hash = block_hash;
        self.genesis_hash = block_hash;
        self
    }

    /// Build the next authenticated state from the current state and a prover-supplied header.
    #[must_use]
    pub fn next<F>(&self, new_header: NewHeader, hash_header: F) -> NextState<'_>
    where
        F: FnOnce(&Header) -> BlockHash,
    {
        let mut next_state = self.clone();
        let new_height = self.height + 1;
        let required_nbits = self.next_nbits;
        let header = new_header.into_header(self.block_hash, required_nbits);
        let block_hash = hash_header(&header);

        next_state.sorted_nibbles = add_timestamp_window(
            &mut next_state.timestamps,
            self.timestamp_count(),
            self.sorted_nibbles,
            new_header.timestamp,
            self.next_timestamp_slot(),
        );

        if new_height % 2016 == 0 {
            next_state.epoch_start_timestamp = new_header.timestamp;
        }

        if (new_height + 1) % 2016 == 0 {
            let actual_timespan = new_header
                .timestamp
                .wrapping_sub(next_state.epoch_start_timestamp);
            let expected_timespan: u32 = 2016 * 600;
            let clamped = actual_timespan
                .max(expected_timespan / 4)
                .min(expected_timespan * 4);
            let pow_limit = bits_to_target(CompactTarget::from_consensus(GENESIS_NBITS));
            let mut new_target = retarget_target(self.next_target(), clamped, expected_timespan);
            if target_exceeds(new_target, pow_limit) {
                new_target = pow_limit;
            }
            next_state.next_nbits = target_to_bits(new_target);
        }

        next_state.chain_work = u256_add(self.chain_work, work_from_bits(required_nbits));
        next_state.header = header;
        next_state.block_hash = block_hash;
        next_state.height = new_height;
        NextState {
            current: self,
            next: next_state,
        }
    }

    /// Apply a batch of new headers, returning either the final state or the first failure.
    pub fn apply_headers<F>(
        &self,
        headers: &[NewHeader],
        hash_header: F,
    ) -> Result<Self, ProofFailure>
    where
        F: Copy + Fn(&Header) -> BlockHash,
    {
        let mut state = self.clone();

        for (header_index, new_header) in headers.iter().copied().enumerate() {
            let next_state = state.next(new_header, hash_header);
            if let Err(error_code) = next_state.validate() {
                return Err(ProofFailure {
                    last_valid_state: state,
                    error_code,
                    header_index: header_index as u32,
                });
            }
            state = next_state.into_state();
        }

        Ok(state)
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            header: Header::default(),
            block_hash: BlockHash::default(),
            genesis_hash: BlockHash::default(),
            next_nbits: CompactTarget::default(),
            height: 0,
            chain_work: ChainWork::default(),
            epoch_start_timestamp: BlockTimestamp::default(),
            timestamps: [BlockTimestamp::default(); WINDOW_SIZE],
            sorted_nibbles: 0,
        }
    }
}

/// A typed state transition built from an authenticated current state and a new header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextState<'a> {
    current: &'a State,
    next: State,
}

impl NextState<'_> {
    /// Validate the transition-specific constraints on the candidate next state.
    pub fn validate(&self) -> Result<(), ValidationErrorCode> {
        if let Some(median_time_past) = self.current.median_time_past() {
            if self.next.header.timestamp <= median_time_past {
                return Err(ValidationErrorCode::TimestampTooOld);
            }
        }

        if !hash_meets_target(self.next.block_hash, self.next.header.nbits) {
            return Err(ValidationErrorCode::PowInsufficient);
        }

        Ok(())
    }

    /// Borrow the next-state candidate.
    #[must_use]
    pub fn state(&self) -> &State {
        &self.next
    }

    /// Consume the transition and return the next-state candidate.
    #[must_use]
    pub fn into_state(self) -> State {
        self.next
    }
}

/// Validation status emitted by the program on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValidationErrorCode {
    HeaderPayloadLengthInvalid = 1,
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
            Self::HeaderPayloadLengthInvalid => "Header payload length invalid",
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
            1 => Ok(Self::HeaderPayloadLengthInvalid),
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
            STATE_SIZE => Ok(Self::Success(
                State::parse(bytes).map_err(PublicValuesParseError::StateParse)?,
            )),
            len if len == STATE_SIZE + 1 + 4 => {
                let state = State::parse(&bytes[..STATE_SIZE])
                    .map_err(PublicValuesParseError::StateParse)?;
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
    StateParse(ParseError),
}

/// Recursive proof metadata that authenticates the current [`State`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursiveProof {
    pub verifier_key: VerifierKeyDigest,
    pub public_values_digest: PublicValuesDigest,
}

/// Parse and validation errors for [`Input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    Parse(ParseError),
    MissingRecursiveProof,
    UnexpectedRecursiveProof,
    HeaderPayloadLengthInvalid { actual: usize },
}

impl core::fmt::Display for InputError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "invalid input encoding: {}", err),
            Self::MissingRecursiveProof => {
                write!(f, "missing recursive proof metadata for non-genesis state")
            }
            Self::UnexpectedRecursiveProof => {
                write!(f, "genesis state must not carry recursive proof metadata")
            }
            Self::HeaderPayloadLengthInvalid { actual } => {
                write!(
                    f,
                    "header payload length {} is not a multiple of {} bytes",
                    actual, NEW_HEADER_SIZE
                )
            }
        }
    }
}

impl From<ParseError> for InputError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

/// Complete typed prover input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub state: State,
    pub recursive_proof: Option<RecursiveProof>,
    pub headers: Vec<NewHeader>,
}

impl Input {
    /// Construct typed input with invariant validation.
    pub fn new(
        state: State,
        recursive_proof: Option<RecursiveProof>,
        headers: Vec<NewHeader>,
    ) -> Result<Self, InputError> {
        if state.height == 0 && recursive_proof.is_some() {
            return Err(InputError::UnexpectedRecursiveProof);
        }
        if state.height > 0 && recursive_proof.is_none() {
            return Err(InputError::MissingRecursiveProof);
        }
        Ok(Self {
            state,
            recursive_proof,
            headers,
        })
    }

    /// Serialize to the host/guest wire format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut state_bytes = self.state.to_bytes();
        if self.state.height == 0 {
            let genesis_hash_offset = BLOCK_HEADER_SIZE + 32;
            state_bytes[genesis_hash_offset..genesis_hash_offset + 32].fill(0);
        }

        let mut out = Vec::with_capacity(
            STATE_SIZE
                + (self.recursive_proof.is_some() as usize) * (8 * 4 + 32)
                + (self.headers.len() * NEW_HEADER_SIZE),
        );
        out.extend_from_slice(&state_bytes);
        if let Some(recursive_proof) = self.recursive_proof {
            for limb in recursive_proof.verifier_key.into_raw() {
                out.extend_from_slice(&limb.to_le_bytes());
            }
            out.extend_from_slice(recursive_proof.public_values_digest.as_raw());
        }
        for header in &self.headers {
            out.extend_from_slice(&header.to_bytes());
        }
        out
    }

    /// Parse and validate input from the host/guest wire format.
    pub fn parse<F>(bytes: &[u8], hash_header: F) -> Result<Self, InputError>
    where
        F: FnOnce(&Header) -> BlockHash,
    {
        let mut off = 0;
        let mut state = State::parse(&take::<STATE_SIZE>(bytes, &mut off)?)?;
        if state.height == 0 {
            state = state.bootstrap_genesis(hash_header);
        }

        let recursive_proof = if state.height == 0 {
            None
        } else {
            let mut verifier_key = [0u32; 8];
            for limb in &mut verifier_key {
                *limb = u32::from_le_bytes(take::<4>(bytes, &mut off)?);
            }
            let public_values_digest = PublicValuesDigest::from_raw(take::<32>(bytes, &mut off)?);
            Some(RecursiveProof {
                verifier_key: VerifierKeyDigest::from_raw(verifier_key),
                public_values_digest,
            })
        };

        let header_bytes = bytes.len().saturating_sub(off);
        if header_bytes % NEW_HEADER_SIZE != 0 {
            return Err(InputError::HeaderPayloadLengthInvalid {
                actual: header_bytes,
            });
        }

        let header_count = header_bytes / NEW_HEADER_SIZE;
        let mut headers = Vec::with_capacity(header_count);
        for index in 0..header_count {
            headers.push(NewHeader::parse_at(bytes, off + (index * NEW_HEADER_SIZE))?);
        }

        Self::new(state, recursive_proof, headers)
    }
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
            Self::StateParse(err) => write!(f, "invalid state payload: {}", err),
        }
    }
}

/// Convert compact `bits` encoding into a 256-bit target.
#[must_use]
pub fn bits_to_target(bits: CompactTarget) -> Target {
    let bits = bits.to_consensus();
    let exponent = bits >> 24;
    let mantissa = bits & 0x00ff_ffff;
    let mut target = [0u8; 32];
    if mantissa == 0 {
        return Target::from_raw(target);
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
    Target::from_raw(target)
}

/// Convert a full 256-bit target into compact `bits` encoding.
#[must_use]
pub fn target_to_bits(target: Target) -> CompactTarget {
    let target = target.as_raw();
    let mut high_byte = 31usize;
    while high_byte > 0 && target[high_byte] == 0 {
        high_byte -= 1;
    }
    if target[high_byte] == 0 {
        return CompactTarget::from_consensus(0);
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
    CompactTarget::from_consensus(((nbytes as u32) << 24) | (mantissa & 0x00ff_ffff))
}

/// Return `true` when `lhs` is strictly greater than `rhs` as unsigned 256-bit integers.
#[must_use]
pub fn target_exceeds(lhs: Target, rhs: Target) -> bool {
    let lhs = lhs.as_raw();
    let rhs = rhs.as_raw();
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
pub fn hash_meets_target(hash: BlockHash, nbits: CompactTarget) -> bool {
    let target = bits_to_target(nbits);
    let hash = hash.as_raw();
    let target = target.as_raw();
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
pub fn u256_add(a: ChainWork, b: ChainWork) -> ChainWork {
    let a = a.as_limbs();
    let b = b.as_limbs();
    let mut result = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let sum = (a[i] as u128) + (b[i] as u128) + carry;
        result[i] = sum as u64;
        carry = sum >> 64;
    }
    ChainWork::from_limbs(result)
}

/// Insert a new timestamp into the circular window and update the packed sort order.
#[must_use]
fn add_timestamp_window(
    timestamps: &mut [BlockTimestamp; WINDOW_SIZE],
    prev_count: usize,
    packed: u64,
    timestamp: BlockTimestamp,
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
pub fn retarget_target(old_target: Target, actual_timespan: u32, expected_timespan: u32) -> Target {
    let old_target = old_target.as_raw();
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
    Target::from_raw(target)
}

/// Convert compact `bits` into cumulative-work units.
#[must_use]
pub fn work_from_bits(bits: CompactTarget) -> ChainWork {
    let bits = bits.to_consensus();
    let exponent = bits >> 24;
    let mantissa = bits & 0x00ff_ffff;
    let k = 8 * (exponent - 3);
    let n = 256 - k;

    if mantissa == 0 || n == 0 {
        return ChainWork::default();
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

    ChainWork::from_limbs(work)
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
    timestamps: &[BlockTimestamp; WINDOW_SIZE],
    packed: u64,
    count: usize,
    timestamp: BlockTimestamp,
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
