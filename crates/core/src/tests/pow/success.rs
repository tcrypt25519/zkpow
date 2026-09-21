use alloc::vec::Vec;
use sp1_cycle::run_track;
use zkvm_test::zkvm_test;

use crate::tests::*;
use crate::{check_proof_of_work, BlockHash, ChainWork, Header, HeaderInput, State};

#[zkvm_test(max_cycles = 150_000)]
fn proof_of_work_accepts_exact_target_boundary() {
    let hash = BlockHash::new(GENESIS_TARGET.to_le_bytes());
    assert!(check_proof_of_work(hash, &GENESIS_TARGET));
}

#[zkvm_test]
fn header_and_new_header_round_trip_exact_wire_bytes() {
    let mut header_bytes = [0u8; Header::SIZE];
    header_bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
    header_bytes[4..36].copy_from_slice(&[0x55; 32]);
    header_bytes[36..68].copy_from_slice(&[0x66; 32]);
    header_bytes[68..72].copy_from_slice(&0x7788_99aau32.to_le_bytes());
    header_bytes[72..76].copy_from_slice(&0x1d00_ffffu32.to_le_bytes());
    header_bytes[76..80].copy_from_slice(&0xbbcc_ddeeu32.to_le_bytes());

    let header = Header::parse(&header_bytes).unwrap();
    assert_eq!(header.version, 0x1122_3344);
    assert_eq!(header.prev_blockhash, BlockHash::new([0x55; 32]));
    assert_eq!(header.merkle_root, [0x66; 32]);
    assert_eq!(header.timestamp, BlockTimestamp::new(0x7788_99aa));
    assert_eq!(header.compact_target, CompactTarget::new(0x1d00_ffff));
    assert_eq!(header.nonce, 0xbbcc_ddee);
    assert_eq!(header.as_bytes(), header_bytes.as_slice());

    let mut new_header_bytes = [0u8; HeaderInput::SIZE];
    new_header_bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
    new_header_bytes[4..36].copy_from_slice(&[0x66; 32]);
    new_header_bytes[36..40].copy_from_slice(&0x7788_99aau32.to_le_bytes());
    new_header_bytes[40..44].copy_from_slice(&0xbbcc_ddeeu32.to_le_bytes());

    let new_header = HeaderInput::from_bytes(&new_header_bytes).unwrap();
    assert_eq!(new_header.version, 0x1122_3344);
    assert_eq!(new_header.merkle_root, [0x66; 32]);
    assert_eq!(new_header.timestamp, BlockTimestamp::new(0x7788_99aa));
    assert_eq!(new_header.nonce, 0xbbcc_ddee);
}

#[zkvm_test]
fn scoped_hash_mock_overrides_and_restores_host_hashing() {
    let real_hash = hash_sample_header();
    {
        let _hash_guard = set_hash_success();
        assert_eq!(hash_sample_header(), zero_hash());
    }
    assert_eq!(hash_sample_header(), real_hash);

    {
        let _hash_guard = set_hash_fail();
        assert_eq!(hash_sample_header(), BlockHash::new([0xFF; 32]));
    }
    assert_eq!(hash_sample_header(), real_hash);
}

#[cfg(not(target_os = "zkvm"))]
#[test]
fn scoped_hash_mock_rejects_nested_override() {
    let real_hash = hash_sample_header();
    let hash_guard = set_hash_success();

    let result = std::panic::catch_unwind(set_hash_fail);
    assert!(result.is_err());
    assert_eq!(hash_sample_header(), zero_hash());

    drop(hash_guard);
    assert_eq!(hash_sample_header(), real_hash);
}

#[zkvm_test]
fn header_version_field_is_not_constrained_by_pow() {
    let mut state = test_state();
    let mut headers10: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    let _hash_guard = set_hash_success();
    headers10[0].version = 0xFFFF_FFFF;
    let hints = median_hints_for_headers(&state, &headers10);
    state.apply_headers_from_genesis::<TestConsensus>(&headers10, &hints)
        .expect("unexpected failure due to version only");
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 12_000_000)]
fn unmocked_bitcoin_chain_verifies_first_50_blocks() {
    let fixture_str = zkvm_test::fixture_str();
    let blocks = parse_fixture_blocks(&fixture_str);
    assert_eq!(blocks.len(), 50, "fixture must contain genesis + 49 blocks");

    let genesis = &blocks[0];
    assert_eq!(genesis.expected_hash, TestConsensus::GENESIS_HASH);

    let mut state = State {
        chain_work:      ChainWork::from_limbs(*TestConsensus::PARAMS.genesis_work.as_limbs()),
        genesis_hash:    TestConsensus::GENESIS_HASH,
        block_hash:      TestConsensus::GENESIS_HASH,
        height:          0,
        current_nbits:   TestConsensus::PARAMS.genesis.compact_target,
        epoch_timestamp: TestConsensus::PARAMS.genesis.timestamp,
        timestamps:      {
            let mut arr = [BlockTimestamp::new(0); WINDOW_SIZE];
            arr[0] = TestConsensus::PARAMS.genesis.timestamp;
            arr
        },
    };

    let header_inputs: Vec<HeaderInput> = blocks[1..50]
        .iter()
        .map(|b| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   BlockTimestamp::new(b.timestamp),
            nonce:       b.nonce,
        })
        .collect();

    let hints = median_hints_for_headers(&state, &header_inputs);

    run_track("unmocked_bitcoin_chain_49_headers", || {
        state
            .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
            .expect("all 49 real Bitcoin headers must validate without mocks");
    });

    assert_eq!(state.height, 49);
    assert_eq!(state.block_hash, blocks[49].expected_hash);

    let expected_work = ChainWork::from_limbs([0x0000_0001_0001_0001 * 50, 0, 0, 0]);
    assert_eq!(state.chain_work, expected_work);
}

#[zkvm_test]
fn target_conversions_and_limb_rollover_edge_cases() {
    // 1. target_to_bits with 0
    let zero_target = Target::from_limbs([0, 0, 0, 0]);
    let zero_bits = crate::target_to_bits(zero_target);
    assert_eq!(zero_bits, CompactTarget::new(0));

    // 2. target_to_bits with nbytes <= 3
    let small_target = Target::from_limbs([0x1234, 0, 0, 0]);
    let small_bits = crate::target_to_bits(small_target);
    assert_eq!(small_bits, CompactTarget::new(0x0212_3400));

    // 3. target_from_bits with size <= 3
    let decoded_small = crate::target_from_bits(CompactTarget::new(0x0212_3400));
    assert_eq!(decoded_small, small_target);

    // 4. block_work_from_target with saturated limb 0 rolling over to limb 1
    let rollover_target = Target::from_limbs([u64::MAX, 1, 0, 0]);
    let work = crate::block_work_from_target(rollover_target);
    assert!(work.into_inner() > zkpow_types::u256::from_limbs([0; 4]));

    // 5. NonMaxU256 returns None on u256::MAX
    let max_u256 = zkpow_types::u256::from_limbs([u64::MAX; 4]);
    assert!(zkpow_types::NonMaxU256::new(max_u256).is_none());

    // 6. Branded and PublicValues formatting
    let branded: zkpow_types::Branded<(), u32> = zkpow_types::Branded::new(42);
    assert_eq!(alloc::format!("{branded:?}"), "42");
}


#[zkvm_test]
fn mul2_with_overflow_doubles_and_reports_evicted_msb() {
    let cases: [([u64; 4], [u64; 4], bool); 5] = [
        ([0; 4], [0; 4], false),
        ([1, 0, 0, 0], [2, 0, 0, 0], false),
        ([1 << 63, 1 << 63, 1 << 63, 0], [0, 1, 1, 1], false),
        ([1 << 63, 0, 0, 1 << 63], [0, 1, 0, 0], true),
        ([u64::MAX; 4], [u64::MAX - 1, u64::MAX, u64::MAX, u64::MAX], true),
    ];
    for (input, expected, overflow) in cases {
        let mut value = zkpow_types::u256::from_limbs(input);
        assert_eq!(crate::mul2_with_overflow(&mut value), overflow, "{input:x?}");
        assert_eq!(value.into_limbs(), expected, "{input:x?}");
    }
}

#[zkvm_test]
fn block_work_from_target_computes_exact_quotients() {
    use zkpow_types::Target;

    // 1. Genesis target work matches known constant:
    let genesis_work = crate::block_work_from_target(zkpow_params::Mainnet::PARAMS.genesis_target);
    assert_eq!(genesis_work.into_inner().into_limbs(), [0x100010001, 0, 0, 0]);

    // 2. Power of 2 divisors:
    // target = 1 => divisor = 2 => floor(2^256 / 2) = 2^255
    let target_1 = Target::from_limbs([1, 0, 0, 0]);
    assert_eq!(
        crate::block_work_from_target(target_1).into_inner().into_limbs(),
        [0, 0, 0, 1 << 63]
    );

    // target = 3 => divisor = 4 => floor(2^256 / 4) = 2^254
    let target_3 = Target::from_limbs([3, 0, 0, 0]);
    assert_eq!(
        crate::block_work_from_target(target_3).into_inner().into_limbs(),
        [0, 0, 0, 1 << 62]
    );

    // target = 2^64 - 1 => divisor = 2^64 => floor(2^256 / 2^64) = 2^192
    let target_2_64 = Target::from_limbs([u64::MAX, 0, 0, 0]);
    assert_eq!(
        crate::block_work_from_target(target_2_64).into_inner().into_limbs(),
        [0, 0, 0, 1]
    );

    // target = 2^128 - 1 => divisor = 2^128 => floor(2^256 / 2^128) = 2^128
    let target_2_128 = Target::from_limbs([u64::MAX, u64::MAX, 0, 0]);
    assert_eq!(
        crate::block_work_from_target(target_2_128).into_inner().into_limbs(),
        [0, 0, 1, 0]
    );

    // 3. Non-power of 2 divisors:
    // target = 2 => divisor = 3 => floor(2^256 / 3) = 0x5555...5555
    let target_2 = Target::from_limbs([2, 0, 0, 0]);
    assert_eq!(
        crate::block_work_from_target(target_2).into_inner().into_limbs(),
        [0x5555555555555555; 4]
    );
}
