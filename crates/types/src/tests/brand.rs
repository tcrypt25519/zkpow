extern crate alloc;
use alloc::string::ToString;

use zkvm_test::test;

use crate::{BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget, Target};

#[test]
fn newtype_construction_and_accessors() {
    let mut val = BlockTimestamp::new(42);
    assert_eq!(*val.as_inner(), 42);
    assert_eq!(*val, 42);

    *val.as_inner_mut() = 100;
    assert_eq!(*val, 100);

    assert_eq!(val.into_inner(), 100);
}

#[test]
fn newtype_zero_copy_byte_conversions() {
    let original: [u8; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
        31, 32,
    ];
    let hash = BlockHash::new(original);

    let bytes: &[u8; 32] = hash.to_bytes::<32>();
    assert_eq!(bytes, &original);

    let borrowed = BlockHash::from_bytes(&original);
    assert_eq!(borrowed, &original);
}

#[test]
fn newtype_u32_le_conversions() {
    let raw = 0x1122_3344u32;
    let ts = BlockTimestamp::new(raw);

    assert_eq!(ts.to_le_bytes(), raw.to_le_bytes());
    assert_eq!(ts.to_le_bytes_slice(), &raw.to_le_bytes());
    assert_eq!(BlockTimestamp::from_le_bytes(raw.to_le_bytes()), ts);

    let target = CompactTarget::new(0x1d00_ffff);
    assert_eq!(target.to_le_bytes(), 0x1d00_ffffu32.to_le_bytes());
    assert_eq!(CompactTarget::from_le_bytes(0x1d00_ffffu32.to_le_bytes()), target);
}

#[test]
fn newtype_32byte_array_conversions() {
    let raw = [0x5au8; 32];
    let hash = BlockHash::new(raw);

    assert_eq!(hash.to_le_bytes(), raw);
    assert_eq!(hash.to_le_bytes_slice(), &raw);
    assert_eq!(BlockHash::from_le_bytes(raw), hash);
    assert_eq!(BlockHash::from_le_bytes_slice(&raw), hash);
}

#[test]
fn newtype_u256_conversions() {
    let limbs = [1u64, 2, 3, 4];
    let work = ChainWork::from_limbs(limbs);

    assert_eq!(work.as_limbs(), &limbs);
    assert_eq!(work.to_le_bytes_slice().len(), 32);

    let bytes = work.to_le_bytes();
    assert_eq!(ChainWork::from_le_bytes(bytes), work);
    assert_eq!(ChainWork::from_le_bytes_slice(&bytes), work);
}

#[test]
fn newtype_addition_with_carry_and_wrap() {
    let a = ChainWork::from_limbs([u64::MAX, 0, 0, 0]);
    let b = ChainWork::from_limbs([1, 0, 0, 0]);
    let c = a + b;
    assert_eq!(c, ChainWork::from_limbs([0, 1, 0, 0]));

    let mut accum = ChainWork::from_limbs([u64::MAX; 4]);
    accum += ChainWork::from_limbs([1, 0, 0, 0]);
    assert_eq!(accum, ChainWork::from_limbs([0, 0, 0, 0]));
}

#[test]
fn block_hash_display_and_ordering() {
    let mut bytes = [0u8; 32];
    bytes[0] = 0x01;
    bytes[31] = 0x80;
    let hash = BlockHash::new(bytes);

    // Display order is reversed (big-endian display for Bitcoin block hashes)
    let display = hash.to_string();
    assert!(display.starts_with("80"));
    assert!(display.ends_with("01"));

    let zero = BlockHash::new([0u8; 32]);
    assert_eq!(
        zero.to_string(),
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert!(zero < hash);
}

#[test]
fn block_timestamp_ordering_and_comparisons() {
    let t1 = BlockTimestamp::new(100);
    let t2 = BlockTimestamp::new(200);
    assert!(t1 < t2);
    assert_eq!(t2.into_inner() - t1.into_inner(), 100);
}

#[test]
fn target_and_block_work_types() {
    let target = Target::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000]);
    assert_eq!(target.as_limbs()[3], 0x0000_0000_FFFF_0000);

    let block_work = BlockWork::from_limbs([0x0001_0001_0001_0001, 0, 0, 0]);
    assert_eq!(block_work.as_limbs()[0], 0x0001_0001_0001_0001);
}
