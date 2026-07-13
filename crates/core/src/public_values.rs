use crate::{
    check_exact_len, BlockHash, BlockTimestamp, ChainWork, CompactTarget, InputParseError,
    ParseError, State, CLAIM_SIZE, CONTINUATION_DATA_SIZE, MINIMAL_PV_SIZE, WINDOW_SIZE,
};

/// Digest of a program verifier key.
#[repr(transparent)]
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

    #[must_use]
    pub fn to_bytes(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, word) in self.0.iter().enumerate() {
            out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut words = [0u32; 8];
        for (i, word) in words.iter_mut().enumerate() {
            *word = u32::from_le_bytes(bytes[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        Self(words)
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
#[repr(transparent)]
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

/// The verifier-visible portion of a validated chain segment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Claim {
    pub genesis_hash: BlockHash,
    pub tip_hash: BlockHash,
    pub chain_work: ChainWork,
    pub height: u32,
}

impl Claim {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; CLAIM_SIZE] {
        let mut out = [0u8; CLAIM_SIZE];
        out[0..32].copy_from_slice(self.genesis_hash.to_le_bytes_slice());
        out[32..64].copy_from_slice(self.tip_hash.to_le_bytes_slice());
        out[64..96].copy_from_slice(self.chain_work.to_le_bytes_slice());
        out[96..100].copy_from_slice(&self.height.to_le_bytes());
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, CLAIM_SIZE)?;
        let genesis_hash = BlockHash::new(bytes[0..32].try_into().unwrap());
        let tip_hash = BlockHash::new(bytes[32..64].try_into().unwrap());
        let chain_work = ChainWork::from_le_bytes(bytes[64..96].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[96..100].try_into().unwrap());
        Ok(Self {
            genesis_hash,
            tip_hash,
            chain_work,
            height,
        })
    }
}

/// Continuation data carried forward at each proof step.
///
/// Committed only as a digest in the public values; the raw bytes are
/// supplied as a private witness when extending to the next step.
///
/// Serialized without struct padding (52 bytes):
/// ```text
///  0..  4  current_nbits           u32 LE
///  4..  8  epoch_start_timestamp   u32 LE
///  8.. 52  timestamps              [u32; 11] LE
/// ```
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationData {
    pub current_nbits: CompactTarget,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

impl ContinuationData {
    #[must_use]
    pub fn from_state(state: &State) -> Self {
        Self {
            current_nbits: state.current_nbits,
            epoch_start_timestamp: state.epoch_start_timestamp,
            timestamps: state.timestamps,
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; CONTINUATION_DATA_SIZE] {
        let mut out = [0u8; CONTINUATION_DATA_SIZE];
        out[0..4].copy_from_slice(self.current_nbits.to_le_bytes_slice());
        out[4..8].copy_from_slice(self.epoch_start_timestamp.to_le_bytes_slice());
        for (i, ts) in self.timestamps.iter().enumerate() {
            out[8 + i * 4..8 + (i + 1) * 4].copy_from_slice(ts.to_le_bytes_slice());
        }
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, CONTINUATION_DATA_SIZE)?;
        let current_nbits = CompactTarget::new(u32::from_le_bytes(bytes[0..4].try_into().unwrap()));
        let epoch_start_timestamp = BlockTimestamp::from_le_bytes(bytes[4..8].try_into().unwrap());
        let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
        for (i, ts) in timestamps.iter_mut().enumerate() {
            *ts = BlockTimestamp::from_le_bytes(
                bytes[8 + i * 4..8 + (i + 1) * 4].try_into().unwrap(),
            );
        }
        Ok(Self {
            current_nbits,
            epoch_start_timestamp,
            timestamps,
        })
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
}

impl core::fmt::Display for ValidationErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::HeaderPayloadLengthInvalid => "Header payload length invalid",
            Self::PowInsufficient => "PoW insufficient",
            Self::TimestampTooOld => "Timestamp too old",
            Self::GenesisHashMismatch => "Genesis hash mismatch",
        })
    }
}

impl TryFrom<u8> for ValidationErrorCode {
    type Error = InputParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HeaderPayloadLengthInvalid),
            2 => Ok(Self::PowInsufficient),
            3 => Ok(Self::TimestampTooOld),
            4 => Ok(Self::GenesisHashMismatch),
            _ => Err(InputParseError::UnknownErrorCode { code: value }),
        }
    }
}

/// Failure payload returned by [`State::apply_headers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyFailure {
    pub last_valid_state: State,
    pub error_code: ValidationErrorCode,
    /// Absolute chain height of the failed header (last_valid_height + 1).
    pub failure_height: u32,
}

/// Failure payload in committed public values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFailure {
    pub error_code: ValidationErrorCode,
    /// Absolute chain height of the failed header (last_valid_height + 1).
    pub failure_height: u32,
}

/// Wire layout of the minimal public values committed by the prover.
///
/// Layout (169 bytes, little-endian):
/// ```text
///  0.. 32  genesis_hash        [u8; 32]
/// 32.. 64  tip_hash            [u8; 32]
/// 64.. 96  chain_work          [u8; 32]  (u256 LE)
/// 96..100  height              u32 LE
/// 100      return_code         u8  (0 = success, nonzero = failure)
/// 101..105 failure_height      u32 LE  (0 on success)
/// 105..137 continuation_digest [u8; 32]
/// 137..169 verifier_key         [u8; 32]  ([u32; 8] LE words)
/// ```
///
/// Serialized manually to avoid struct padding; use [`MinimalPublicValues::to_bytes`]
/// and [`MinimalPublicValues::parse`] rather than transmuting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimalPublicValues {
    pub genesis_hash: BlockHash,
    pub tip_hash: BlockHash,
    pub chain_work: [u8; 32],
    pub height: u32,
    pub return_code: u8,
    pub failure_height: u32,
    pub continuation_digest: [u8; 32],
    pub verifier_key: VerifierKeyDigest,
}

impl MinimalPublicValues {
    /// Build success public values from a public claim and continuation digest.
    #[must_use]
    pub fn success(
        claim: &Claim,
        continuation_digest: [u8; 32],
        verifier_key: VerifierKeyDigest,
    ) -> Self {
        Self {
            genesis_hash: claim.genesis_hash,
            tip_hash: claim.tip_hash,
            chain_work: claim.chain_work.to_le_bytes(),
            height: claim.height,
            return_code: 0,
            failure_height: 0,
            continuation_digest,
            verifier_key,
        }
    }

    /// Build failure public values from a public claim, error, and continuation digest.
    #[must_use]
    pub fn failure(
        claim: &Claim,
        error_code: ValidationErrorCode,
        failure_height: u32,
        continuation_digest: [u8; 32],
        verifier_key: VerifierKeyDigest,
    ) -> Self {
        Self {
            genesis_hash: claim.genesis_hash,
            tip_hash: claim.tip_hash,
            chain_work: claim.chain_work.to_le_bytes(),
            height: claim.height,
            return_code: error_code.as_byte(),
            failure_height,
            continuation_digest,
            verifier_key,
        }
    }

    /// Serialize to exactly [`MINIMAL_PV_SIZE`] bytes (no padding).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; MINIMAL_PV_SIZE] {
        let mut out = [0u8; MINIMAL_PV_SIZE];
        out[0..32].copy_from_slice(self.genesis_hash.to_le_bytes_slice());
        out[32..64].copy_from_slice(self.tip_hash.to_le_bytes_slice());
        out[64..96].copy_from_slice(&self.chain_work);
        out[96..100].copy_from_slice(&self.height.to_le_bytes());
        out[100] = self.return_code;
        out[101..105].copy_from_slice(&self.failure_height.to_le_bytes());
        out[105..137].copy_from_slice(&self.continuation_digest);
        out[137..169].copy_from_slice(&self.verifier_key.to_bytes());
        out
    }

    /// Deserialize from exactly [`MINIMAL_PV_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputParseError> {
        if bytes.len() != MINIMAL_PV_SIZE {
            return Err(InputParseError::InvalidLength {
                expected: MINIMAL_PV_SIZE,
                actual: bytes.len(),
            });
        }
        let genesis_hash = BlockHash::new(bytes[0..32].try_into().unwrap());
        let tip_hash = BlockHash::new(bytes[32..64].try_into().unwrap());
        let chain_work: [u8; 32] = bytes[64..96].try_into().unwrap();
        let height = u32::from_le_bytes(bytes[96..100].try_into().unwrap());
        let return_code = bytes[100];
        let failure_height = u32::from_le_bytes(bytes[101..105].try_into().unwrap());
        let continuation_digest: [u8; 32] = bytes[105..137].try_into().unwrap();
        let verifier_key = VerifierKeyDigest::from_bytes(bytes[137..169].try_into().unwrap());
        Ok(Self {
            genesis_hash,
            tip_hash,
            chain_work,
            height,
            return_code,
            failure_height,
            continuation_digest,
            verifier_key,
        })
    }

    /// Extract the chain work as a [`ChainWork`].
    #[must_use]
    pub fn chain_work(&self) -> ChainWork {
        ChainWork::from_le_bytes(self.chain_work)
    }

    /// Return the error code if this is a failure, or `None` on success.
    pub fn error_code(&self) -> Option<Result<ValidationErrorCode, InputParseError>> {
        if self.return_code == 0 {
            None
        } else {
            Some(ValidationErrorCode::try_from(self.return_code))
        }
    }
}

/// Typed public values committed by the prover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderChainPublicValues {
    Success {
        claim: Claim,
        continuation_digest: [u8; 32],
        verifier_key: VerifierKeyDigest,
    },
    Failure {
        failure: ProofFailure,
        last_valid_claim: Claim,
        continuation_digest: [u8; 32],
        verifier_key: VerifierKeyDigest,
    },
}

impl HeaderChainPublicValues {
    /// Parse committed public values from the minimal 169-byte format.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputParseError> {
        let pv = MinimalPublicValues::parse(bytes)?;
        let claim = Claim {
            genesis_hash: pv.genesis_hash,
            tip_hash: pv.tip_hash,
            chain_work: pv.chain_work(),
            height: pv.height,
        };
        if pv.return_code == 0 {
            Ok(Self::Success {
                claim,
                continuation_digest: pv.continuation_digest,
                verifier_key: pv.verifier_key,
            })
        } else {
            let error_code = ValidationErrorCode::try_from(pv.return_code)?;
            Ok(Self::Failure {
                failure: ProofFailure {
                    error_code,
                    failure_height: pv.failure_height,
                },
                last_valid_claim: claim,
                continuation_digest: pv.continuation_digest,
                verifier_key: pv.verifier_key,
            })
        }
    }

    /// Borrow the public claim regardless of success or failure.
    #[must_use]
    pub fn claim(&self) -> &Claim {
        match self {
            Self::Success { claim, .. } => claim,
            Self::Failure {
                last_valid_claim, ..
            } => last_valid_claim,
        }
    }

    /// Return the continuation digest.
    #[must_use]
    pub fn continuation_digest(&self) -> &[u8; 32] {
        match self {
            Self::Success {
                continuation_digest,
                ..
            } => continuation_digest,
            Self::Failure {
                continuation_digest,
                ..
            } => continuation_digest,
        }
    }

    /// Return the verifier-key digest committed by the proof.
    #[must_use]
    pub fn verifier_key(&self) -> &VerifierKeyDigest {
        match self {
            Self::Success { verifier_key, .. } | Self::Failure { verifier_key, .. } => verifier_key,
        }
    }
}
