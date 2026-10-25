use crate::{BlockWork, CompactTarget, Target};

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

impl Into<Target> for TargetTriplet {
    fn into(self) -> Target {
        self.target
    }
}

impl Into<BlockWork> for TargetTriplet {
    fn into(self) -> BlockWork {
        self.block_work
    }
}

impl Into<CompactTarget> for TargetTriplet {
    fn into(self) -> CompactTarget {
        self.compact_target
    }
}
