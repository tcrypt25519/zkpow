use zkvm_test::test;

use crate::{
    u256, BlockHash, BlockTimestamp, BlockWork, ByteInfo, ChainWork, CompactTarget, Header,
    NonMaxU256, Target,
};

#[test]
fn byte_info_sizes_and_alignments() {
    assert_eq!(Header::SIZE, 80);
    assert_eq!(Header::ALIGN, 4);

    assert_eq!(BlockHash::SIZE, 32);
    assert_eq!(BlockHash::ALIGN, 1);

    assert_eq!(BlockTimestamp::SIZE, 4);
    assert_eq!(BlockTimestamp::ALIGN, 4);

    assert_eq!(CompactTarget::SIZE, 4);
    assert_eq!(CompactTarget::ALIGN, 4);

    assert_eq!(u256::SIZE, 32);
    assert_eq!(u256::ALIGN, 8);

    assert_eq!(NonMaxU256::SIZE, 32);
    assert_eq!(NonMaxU256::ALIGN, 8);

    assert_eq!(ChainWork::SIZE, 32);
    assert_eq!(ChainWork::ALIGN, 8);

    assert_eq!(BlockWork::SIZE, 32);
    assert_eq!(BlockWork::ALIGN, 8);

    assert_eq!(Target::SIZE, 32);
    assert_eq!(Target::ALIGN, 8);
}
