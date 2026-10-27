use core::fmt;

/// An error representing a failed attempt to construct a [`RunId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunIdError {
    /// The input length was not exactly 8 bytes.
    InvalidLength,
    /// A byte in the identifier was not an ASCII alphanumeric character (`0..=9`, `a..=z`, `A..=Z`).
    InvalidCharacter(u8),
}

impl fmt::Display for RunIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "run ID must be exactly 8 ASCII alphanumeric characters"),
            Self::InvalidCharacter(b) => {
                write!(f, "run ID byte {b:#02x} is not ASCII alphanumeric (0-9, a-z, A-Z)")
            },
        }
    }
}

impl core::error::Error for RunIdError {}

/// A strongly-typed 8-byte ASCII alphanumeric identifier for proving runs.
///
/// Only ASCII digits (`0..=9`), uppercase letters (`A..=Z`), and lowercase letters (`a..=z`)
/// are permitted (62^8 ≈ 2.18 × 10^14 possible identifiers).
///
/// No symbols, path separators, or Unicode characters are allowed. This guarantees that a
/// `RunId` is always safe to use as a single path component without escaping or path traversal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId([u8; 8]);

impl RunId {
    /// Validates and constructs a [`RunId`] from an owned 8-byte array.
    ///
    /// This is the primary constructor enforcing that every byte is an ASCII alphanumeric
    /// character (`0..=9`, `a..=z`, `A..=Z`).
    pub const fn from_bytes(bytes: [u8; 8]) -> Result<Self, RunIdError> {
        let mut i = 0;
        while i < 8 {
            let b = bytes[i];
            if !((b >= b'0' && b <= b'9') || (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z')) {
                return Err(RunIdError::InvalidCharacter(b));
            }
            i += 1;
        }
        Ok(Self(bytes))
    }

    /// Alias for [`from_bytes`](Self::from_bytes).
    pub const fn new(bytes: [u8; 8]) -> Result<Self, RunIdError> {
        Self::from_bytes(bytes)
    }

    /// Validates and constructs a [`RunId`] from a string slice.
    ///
    /// Rejects strings that are not exactly 8 bytes or contain non-ASCII-alphanumeric characters.
    pub fn from_str(s: &str) -> Result<Self, RunIdError> {
        let bytes = s.as_bytes();
        if bytes.len() != 8 {
            return Err(RunIdError::InvalidLength);
        }
        let arr: [u8; 8] = [
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ];
        Self::from_bytes(arr)
    }

    /// Returns the underlying 8-byte array.
    pub const fn to_bytes(&self) -> [u8; 8] {
        self.0
    }

    /// Returns a reference to the underlying 8-byte array.
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Returns the run ID as a string slice.
    ///
    /// Invariant: all 8 bytes are guaranteed ASCII alphanumeric, so this is always valid UTF-8.
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }
}

impl core::ops::Deref for RunId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for RunId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for RunId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl core::borrow::Borrow<str> for RunId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RunId({})", self.as_str())
    }
}

impl TryFrom<[u8; 8]> for RunId {
    type Error = RunIdError;

    fn try_from(bytes: [u8; 8]) -> Result<Self, Self::Error> {
        Self::from_bytes(bytes)
    }
}

impl TryFrom<&str> for RunId {
    type Error = RunIdError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }
}

impl core::str::FromStr for RunId {
    type Err = RunIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s)
    }
}
