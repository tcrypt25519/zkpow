use zkvm_test::test;

use crate::{u256, BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget, Header};

#[test]
fn layout_sizes_and_alignments() {
    for (layout, expected) in [
        ((size_of::<Header>(), align_of::<Header>()), (80, 4)),
        ((size_of::<BlockHash>(), align_of::<BlockHash>()), (32, 1)),
        ((size_of::<BlockTimestamp>(), align_of::<BlockTimestamp>()), (4, 4)),
        ((size_of::<CompactTarget>(), align_of::<CompactTarget>()), (4, 4)),
        ((size_of::<u256>(), align_of::<u256>()), (32, 8)),
        ((size_of::<ChainWork>(), align_of::<ChainWork>()), (32, 8)),
        ((size_of::<BlockWork>(), align_of::<BlockWork>()), (32, 8)),
    ] {
        assert_eq!(layout, expected);
    }
}
