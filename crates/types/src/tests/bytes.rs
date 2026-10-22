use zkvm_test::test;

use crate::{u256, BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget, Header};

#[test]
fn layout_sizes_and_alignments() {
    assert_eq!(size_of::<Header>(), 80);
    assert_eq!(align_of::<Header>(), 4);

    assert_eq!(size_of::<BlockHash>(), 32);
    assert_eq!(align_of::<BlockHash>(), 1);

    assert_eq!(size_of::<BlockTimestamp>(), 4);
    assert_eq!(align_of::<BlockTimestamp>(), 4);

    assert_eq!(size_of::<CompactTarget>(), 4);
    assert_eq!(align_of::<CompactTarget>(), 4);

    assert_eq!(size_of::<u256>(), 32);
    assert_eq!(align_of::<u256>(), 8);

    assert_eq!(size_of::<ChainWork>(), 32);
    assert_eq!(align_of::<ChainWork>(), 8);

    assert_eq!(size_of::<BlockWork>(), 32);
    assert_eq!(align_of::<BlockWork>(), 8);
}
