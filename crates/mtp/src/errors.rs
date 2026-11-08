use core::fmt;

/// Why a claimed median is not the window's median.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotMedian {
    MissingHint,
    TooNew,
    TooOld,
}

impl fmt::Display for NotMedian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingHint => "median hint not available for application",
            Self::TooNew => "claimed median is newer than half of the known window",
            Self::TooOld => "claimed median is older than half of the known window",
        })
    }
}

/// Why a header slice cannot be split into the cold prefix and warm remainder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsufficientColdHeaders {
    required: usize,
    actual:   usize,
}

impl InsufficientColdHeaders {
    pub(crate) fn new(actual: usize) -> Self {
        Self {
            required: crate::WINDOW_SIZE - 1,
            actual,
        }
    }

    #[must_use]
    pub const fn required(self) -> usize {
        self.required
    }

    #[must_use]
    pub const fn actual(self) -> usize {
        self.actual
    }
}

impl fmt::Display for InsufficientColdHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a cold start needs at least {} headers, got {}", self.required, self.actual)
    }
}
