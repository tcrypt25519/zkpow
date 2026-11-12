use crate::{BlockWork, CompactTarget, Target};

/// A target, its compact form, and the block work it represents in a single struct. These values all are different
/// forms of the same information, so they're generated at the same time and often used near each other. Binding them
/// together helps protect against unnecessary recalculation and ensures they have the same lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetTriplet {
    compact_target: CompactTarget,
    target:         Target,
    block_work:     BlockWork,
}

impl TargetTriplet {
    #[inline]
    #[must_use]
    pub const fn new(compact_target: CompactTarget, target: Target, block_work: BlockWork) -> Self {
        Self {
            compact_target,
            target,
            block_work,
        }
    }

    #[inline]
    #[must_use]
    pub const fn compact_target(&self) -> CompactTarget {
        self.compact_target
    }

    #[inline]
    #[must_use]
    pub const fn target(&self) -> Target {
        self.target
    }

    #[inline]
    #[must_use]
    pub const fn block_work(&self) -> BlockWork {
        self.block_work
    }

    /// Return the three components as a tuple.
    #[inline]
    #[must_use]
    pub const fn components(&self) -> (CompactTarget, Target, BlockWork) {
        (self.compact_target, self.target, self.block_work)
    }
}

impl From<(CompactTarget, Target, BlockWork)> for TargetTriplet {
    #[inline]
    fn from((compact_target, target, block_work): (CompactTarget, Target, BlockWork)) -> Self {
        Self {
            compact_target,
            target,
            block_work,
        }
    }
}

impl From<TargetTriplet> for CompactTarget {
    fn from(value: TargetTriplet) -> Self {
        value.compact_target
    }
}

impl From<TargetTriplet> for Target {
    fn from(value: TargetTriplet) -> Self {
        value.target
    }
}

impl From<TargetTriplet> for BlockWork {
    fn from(value: TargetTriplet) -> Self {
        value.block_work
    }
}
