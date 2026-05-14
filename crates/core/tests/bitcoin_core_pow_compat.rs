//! Compatibility tests ported from Bitcoin Core's `pow_tests.cpp` and
//! `arith_uint256_tests.cpp`.
//!
//! These vectors intentionally mirror Bitcoin Core's compact-target, proof of
//! work, and 256-bit arithmetic expectations where this crate exposes the same
//! consensus surface. Bitcoin Core tests for operators that zkpow does not
//! expose are omitted.

use zkpow_core::{
    bits_to_target, calculate_next_work_required, check_proof_of_work, target_gt, target_to_bits,
    work_from_target, BlockHash, ChainWork, CompactTarget, CompactTargetError, Target,
    EXPECTED_EPOCH_TIMESPAN, GENESIS_TARGET, MAX_EPOCH_TIMESPAN, MIN_EPOCH_TIMESPAN,
};

fn le_bytes_from_be_hex(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64);

    let mut bytes = [0u8; 32];
    for (idx, byte) in bytes.iter_mut().enumerate() {
        let offset = idx * 2;
        *byte = u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap();
    }
    bytes.reverse();
    bytes
}

fn target_from_be_hex(hex: &str) -> Target {
    Target::from_le_bytes(le_bytes_from_be_hex(hex))
}

fn chain_work_from_be_hex(hex: &str) -> ChainWork {
    ChainWork::from_le_bytes(le_bytes_from_be_hex(hex))
}

fn target_mul_u32(target: Target, multiplier: u32) -> Target {
    let limbs = target.as_limbs();
    let mut out = [0u64; 4];
    let mut carry = 0u128;
    for idx in 0..4 {
        let product = limbs[idx] as u128 * multiplier as u128 + carry;
        out[idx] = product as u64;
        carry = product >> 64;
    }
    Target::from_limbs(out)
}

fn bitcoin_core_next_work_bits(old_bits: u32, first_time: i64, last_time: i64) -> u32 {
    let old_target = bits_to_target(CompactTarget::from_consensus(old_bits)).unwrap();
    let clamped_timespan = (last_time - first_time).clamp(MIN_EPOCH_TIMESPAN, MAX_EPOCH_TIMESPAN);
    let mut next_target =
        calculate_next_work_required(old_target, clamped_timespan as u32, EXPECTED_EPOCH_TIMESPAN);

    if target_gt(next_target, GENESIS_TARGET) {
        next_target = GENESIS_TARGET;
    }

    target_to_bits(next_target).to_consensus()
}

#[test]
fn bitcoin_core_arith_uint256_basic_conversion_vectors() {
    let cases = [
        (
            "7D1DE5EAF9B156D53208F033B5AA8122D2d2355d5e12292b121156cfdb4a529c",
            [
                0x9c, 0x52, 0x4a, 0xdb, 0xcf, 0x56, 0x11, 0x12, 0x2b, 0x29, 0x12, 0x5e, 0x5d, 0x35,
                0xd2, 0xd2, 0x22, 0x81, 0xaa, 0xb5, 0x33, 0xf0, 0x08, 0x32, 0xd5, 0x56, 0xb1, 0xf9,
                0xea, 0xe5, 0x1d, 0x7d,
            ],
        ),
        (
            "D781CAB4F072134971DA2D19A3473013BFB69CA6C30A7E26406BA5477C1D3270",
            [
                0x70, 0x32, 0x1d, 0x7c, 0x47, 0xa5, 0x6b, 0x40, 0x26, 0x7e, 0x0a, 0xc3, 0xa6, 0x9c,
                0xb6, 0xbf, 0x13, 0x30, 0x47, 0xa3, 0x19, 0x2d, 0xda, 0x71, 0x49, 0x13, 0x72, 0xf0,
                0xb4, 0xca, 0x81, 0xd7,
            ],
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000000",
            [0u8; 32],
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000001",
            {
                let mut bytes = [0u8; 32];
                bytes[0] = 1;
                bytes
            },
        ),
        (
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            [0xffu8; 32],
        ),
    ];

    for (hex, expected_le_bytes) in cases {
        let value = ChainWork::from_le_bytes(expected_le_bytes);
        assert_eq!(value.to_le_bytes(), le_bytes_from_be_hex(hex));
    }
}

#[test]
fn bitcoin_core_arith_uint256_add_and_mul_vectors() {
    let r1 =
        chain_work_from_be_hex("7D1DE5EAF9B156D53208F033B5AA8122D2d2355d5e12292b121156cfdb4a529c");
    let r2 =
        chain_work_from_be_hex("D781CAB4F072134971DA2D19A3473013BFB69CA6C30A7E26406BA5477C1D3270");

    assert_eq!(
        r1 + r2,
        chain_work_from_be_hex("549fb09fea236a1ea3e31d4d58f1b1369288d204211ca751527cfc175767850c",)
    );
    assert_eq!(
        r1 * 3,
        chain_work_from_be_hex("7759b1c0ed14047f961ad09b20ff83687876a0181a367b813634046f91def7d4",)
    );
    assert_eq!(
        r2 * 0x8765_4321,
        chain_work_from_be_hex("23f7816e30c4ae2017257b7a0fa64d60402f5234d46e746b61c960d09a26d070",)
    );
}

#[test]
fn bitcoin_core_setcompact_valid_vectors() {
    let cases = [
        (
            0,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0012_3456,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0100_3456,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0200_0056,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0300_0000,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0400_0000,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0092_3456,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0180_3456,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0280_0056,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0380_0000,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0480_0000,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0,
        ),
        (
            0x0112_3456,
            "0000000000000000000000000000000000000000000000000000000000000012",
            0x0112_0000,
        ),
        (
            0x0212_3456,
            "0000000000000000000000000000000000000000000000000000000000001234",
            0x0212_3400,
        ),
        (
            0x0312_3456,
            "0000000000000000000000000000000000000000000000000000000000123456",
            0x0312_3456,
        ),
        (
            0x0412_3456,
            "0000000000000000000000000000000000000000000000000000000012345600",
            0x0412_3456,
        ),
        (
            0x0500_9234,
            "0000000000000000000000000000000000000000000000000000000092340000",
            0x0500_9234,
        ),
        (
            0x2012_3456,
            "1234560000000000000000000000000000000000000000000000000000000000",
            0x2012_3456,
        ),
    ];

    for (compact, expected_target, expected_canonical) in cases {
        let target = bits_to_target(CompactTarget::from_consensus(compact)).unwrap();
        assert_eq!(target, target_from_be_hex(expected_target));
        assert_eq!(target_to_bits(target).to_consensus(), expected_canonical);
    }

    assert_eq!(
        target_to_bits(target_from_be_hex(
            "0000000000000000000000000000000000000000000000000000000000000080",
        ))
        .to_consensus(),
        0x0200_8000,
    );
}

#[test]
fn bitcoin_core_setcompact_rejects_invalid_vectors() {
    let cases = [
        (0x01fe_dcba, CompactTargetError::Negative),
        (0x0492_3456, CompactTargetError::Negative),
        (0x1d80_ffff, CompactTargetError::Negative),
        (0xff12_3456, CompactTargetError::Overflow),
    ];

    for (compact, expected_error) in cases {
        assert_eq!(
            bits_to_target(CompactTarget::from_consensus(compact)),
            Err(expected_error),
        );
    }
}

#[test]
fn bitcoin_core_pow_next_work_vectors() {
    let cases = [
        (1261130161, 1262152739, 0x1d00_ffff, 0x1d00_d86a),
        (1231006505, 1233061996, 0x1d00_ffff, 0x1d00_ffff),
        (1279008237, 1279297671, 0x1c05_a3f4, 0x1c01_68fd),
        (1263163443, 1269211443, 0x1c38_7f6f, 0x1d00_e1fd),
    ];

    for (first_time, last_time, old_bits, expected_bits) in cases {
        assert_eq!(
            bitcoin_core_next_work_bits(old_bits, first_time, last_time),
            expected_bits,
        );
    }
}

#[test]
fn bitcoin_core_check_proof_of_work_vectors() {
    let one_hash = {
        let mut bytes = [0u8; 32];
        bytes[0] = 1;
        BlockHash::from_raw(bytes)
    };

    assert!(!check_proof_of_work(
        one_hash,
        target_mul_u32(GENESIS_TARGET, 2)
    ));
    assert!(!check_proof_of_work(
        BlockHash::from_raw(target_mul_u32(GENESIS_TARGET, 2).to_le_bytes()),
        GENESIS_TARGET,
    ));
    assert!(!check_proof_of_work(
        BlockHash::from_raw([0u8; 32]),
        Target::from_limbs([0; 4]),
    ));
    assert!(check_proof_of_work(
        BlockHash::from_raw(GENESIS_TARGET.to_le_bytes()),
        GENESIS_TARGET,
    ));
}

#[test]
fn bitcoin_core_genesis_work_matches_compact_target() {
    let genesis_target = bits_to_target(CompactTarget::from_consensus(0x1d00_ffff)).unwrap();

    assert_eq!(genesis_target, GENESIS_TARGET);
    assert_eq!(
        work_from_target(genesis_target),
        work_from_target(GENESIS_TARGET)
    );
}
