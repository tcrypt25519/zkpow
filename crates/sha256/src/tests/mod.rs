use sha2::{Digest, Sha256};
use zkvm_test::test;

use crate::{of_header, of_public_values};

fn decode_hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex character"),
    }
}

fn parse_hex_32(s: &str) -> [u8; 32] {
    let bytes = s.as_bytes();
    assert_eq!(bytes.len(), 64, "expected 64 hex characters for 32 bytes");
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = (decode_hex_nibble(bytes[i * 2]) << 4) | decode_hex_nibble(bytes[i * 2 + 1]);
    }
    out
}

fn parse_u32(s: &str) -> u32 {
    let mut val = 0u32;
    for &b in s.as_bytes() {
        assert!(b.is_ascii_digit(), "expected decimal digits for u32");
        val = val * 10 + (b - b'0') as u32;
    }
    val
}

fn assert_public_values_hash(bytes: [u8; 185]) {
    let expected: [u8; 32] = Sha256::digest(bytes).into();
    let actual = of_public_values(&bytes[..152], &bytes[152..184], bytes[184]);
    assert_eq!(actual, expected);
}

#[test]
fn public_values_optimized_hash_matches_standard_sha256() {
    assert_public_values_hash(core::array::from_fn(|i| (i & 0xFF) as u8));
}

#[test]
fn public_values_optimized_hash_matches_sha256_for_zero_input() {
    assert_public_values_hash([0u8; 185]);
}

#[test]
fn public_values_optimized_hash_matches_sha256_for_high_entropy_input() {
    assert_public_values_hash(core::array::from_fn(|i| (i.wrapping_mul(251) as u8).wrapping_add(1)));
}

#[test]
fn guest_output_capture_demonstration() {
    #[cfg(target_os = "zkvm")]
    {
        sp1_zkvm::io::write(1, b">>> HELLO FROM SP1 GUEST STDOUT! <<<\n");
        sp1_zkvm::io::write(2, b">>> HELLO FROM SP1 GUEST STDERR! <<<\n");
    }
}

#[test(max_cycles = 500_000)]
fn budgeted_hash_within_limit() {
    assert_public_values_hash([1u8; 185]);
}

#[test(max_cycles = 50_000)]
#[ignore]
fn budgeted_hash_exceeding_limit() {
    assert_public_values_hash([2u8; 185]);
}

#[test]
fn subroutine_cycle_profiling_demonstration() {
    sp1_cycle::run_track("outer_pipeline", || {
        sp1_cycle::run_track("first_hash_pass", || {
            assert_public_values_hash([10u8; 185]);
        });
        sp1_cycle::run_track("second_hash_pass", || {
            assert_public_values_hash([20u8; 185]);
        });
    });
}

#[test(fixture = "headers_fixture.bin")]
fn test_block_headers_fixture_verification() {
    let fixture_str = zkvm_test::fixture_str();
    let mut header_count = 0;

    for line in fixture_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let expected_hash_str = parts.next().expect("expected hash");
        let version_str = parts.next().expect("expected version");
        let prev_blockhash_str = parts.next().expect("expected prev_blockhash");
        let merkle_root_str = parts.next().expect("expected merkle_root");
        let timestamp_str = parts.next().expect("expected timestamp");
        let compact_target_str = parts.next().expect("expected compact_target");
        let nonce_str = parts.next().expect("expected nonce");

        let expected_hash = parse_hex_32(expected_hash_str);
        let version = parse_u32(version_str);
        let prev_blockhash = parse_hex_32(prev_blockhash_str);
        let merkle_root = parse_hex_32(merkle_root_str);
        let timestamp = parse_u32(timestamp_str);
        let compact_target = parse_u32(compact_target_str);
        let nonce = parse_u32(nonce_str);

        let actual_hash = of_header(
            version,
            &prev_blockhash,
            &merkle_root,
            timestamp,
            compact_target,
            nonce,
        );

        assert_eq!(
            actual_hash, expected_hash,
            "header hash mismatch for block with timestamp {}",
            timestamp
        );
        header_count += 1;
    }

    assert!(header_count > 0, "fixture contained no headers");
}

#[test(snapshot = "snapshots/genesis_header_hash.hex")]
fn test_genesis_header_hash_snapshot() {
    let version = 1u32;
    let prev_blockhash = [0u8; 32];
    let merkle_root = parse_hex_32("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b");
    let timestamp = 1231006505u32;
    let compact_target = 0x1d00ffffu32;
    let nonce = 2083236893u32;

    let hash = of_header(
        version,
        &prev_blockhash,
        &merkle_root,
        timestamp,
        compact_target,
        nonce,
    );

    zkvm_test::commit_slice(&hash);
}

fn reference_sha256d(bytes: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(bytes);
    let second = Sha256::digest(first);
    second.into()
}

fn build_wire_header(
    version: u32,
    prev_blockhash: [u8; 32],
    merkle_root: [u8; 32],
    timestamp: u32,
    compact_target: u32,
    nonce: u32,
) -> [u8; 80] {
    let mut out = [0u8; 80];
    out[0..4].copy_from_slice(&version.to_le_bytes());
    out[4..36].copy_from_slice(&prev_blockhash);
    out[36..68].copy_from_slice(&merkle_root);
    out[68..72].copy_from_slice(&timestamp.to_le_bytes());
    out[72..76].copy_from_slice(&compact_target.to_le_bytes());
    out[76..80].copy_from_slice(&nonce.to_le_bytes());
    out
}

#[test]
fn public_values_bit_flip_sensitivity_test() {
    let base: [u8; 185] = core::array::from_fn(|i| (i.wrapping_mul(37) ^ 0x5a) as u8);
    let base_hash = of_public_values(&base[..152], &base[152..184], base[184]);

    // Test sensitivity across key boundary positions
    let probe_indices = [0, 15, 63, 64, 127, 128, 151, 152, 170, 183, 184];

    for &idx in &probe_indices {
        for bit in 0..8 {
            let mut mutated = base;
            mutated[idx] ^= 1 << bit;

            let mutated_hash = of_public_values(&mutated[..152], &mutated[152..184], mutated[184]);
            assert_ne!(
                mutated_hash, base_hash,
                "bit flip at byte {idx}, bit {bit} did not change public values hash"
            );

            let expected_ref: [u8; 32] = Sha256::digest(mutated).into();
            assert_eq!(
                mutated_hash, expected_ref,
                "mutated hash at byte {idx}, bit {bit} does not match reference Sha256"
            );
        }
    }
}

#[test]
fn of_header_differential_testing_against_reference_sha256d() {
    let test_cases = [
        // All zeros
        (0u32, [0u8; 32], [0u8; 32], 0u32, 0u32, 0u32),
        // All ones
        (u32::MAX, [0xFFu8; 32], [0xFFu8; 32], u32::MAX, u32::MAX, u32::MAX),
        // Alternating bit patterns
        (0x5555_5555, [0xAAu8; 32], [0x55u8; 32], 0xAAAA_AAAA, 0x1d00_ffff, 0x1234_5678),
        // High timestamp & low target
        (2, [0x01u8; 32], [0x02u8; 32], 0x7FFF_FFFF, 0x1b04_04cb, 100_000),
    ];

    for (v, prev, mr, ts, target, nonce) in test_cases {
        let wire = build_wire_header(v, prev, mr, ts, target, nonce);
        let expected = reference_sha256d(&wire);
        let actual = of_header(v, &prev, &mr, ts, target, nonce);
        assert_eq!(actual, expected, "of_header mismatch for version {v}");
    }

    // Pseudo-random fuzz loop (deterministic PRNG)
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next_u32 = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as u32
    };

    for _ in 0..10 {
        let v = next_u32();
        let prev = core::array::from_fn(|_| next_u32() as u8);
        let mr = core::array::from_fn(|_| next_u32() as u8);
        let ts = next_u32();
        let target = next_u32();
        let nonce = next_u32();

        let wire = build_wire_header(v, prev, mr, ts, target, nonce);
        let expected = reference_sha256d(&wire);
        let actual = of_header(v, &prev, &mr, ts, target, nonce);
        assert_eq!(actual, expected, "of_header fuzz mismatch for seed {seed}");
    }
}

#[test]
fn word_extraction_helpers_match_le_be_definitions() {
    use crate::{be_u64_from_slice, word_from_bytes, word_from_le_u32};

    let slice = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
    assert_eq!(be_u64_from_slice(&slice, 0), 0x12345678);
    assert_eq!(be_u64_from_slice(&slice, 1), 0x9abcdef0);

    let le_val = 0x12345678u32;
    assert_eq!(word_from_le_u32(le_val), 0x78563412);

    assert_eq!(word_from_bytes(&slice, 0), 0x12345678);
    assert_eq!(word_from_bytes(&slice, 1), 0x9abcdef0);
}

#[test(snapshot = "snapshots/mainnet_headers_0_to_7_hashes.hex")]
fn test_mainnet_headers_0_to_7_hash_vectors() {
    const HEADERS_HEX: &[&str] = &[
        "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c",
        "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e36299",
        "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd61",
        "01000000bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a0000000044f672226090d85db9a9f2fbfe5f0f9609b387af7be5b7fbb7a1767c831c9e995dbe6649ffff001d05e0ed6d",
        "010000004944469562ae1c2c74d9a535e00b6f3e40ffbad4f2fda3895501b582000000007a06ea98cd40ba2e3288262b28638cec5337c1456aaf5eedc8e9e5a20f062bdf8cc16649ffff001d2bfee0a9",
        "0100000085144a84488ea88d221c8bd6c059da090e88f8a2c99690ee55dbba4e00000000e11c48fecdd9e72510ca84f023370c9a38bf91ac5cae88019bee94d24528526344c36649ffff001d1d03e477",
        "01000000fc33f596f822a0a1951ffdbf2a897b095636ad871707bf5d3162729b00000000379dfb96a5ea8c81700ea4ac6b97ae9a9312b2d4301a29580e924ee6761a2520adc46649ffff001d189c4c97",
        "010000008d778fdc15a2d3fb76b7122a3b5582bea4f21f5a0c693537e7a03130000000003f674005103b42f984169c7d008370967e91920a6a5d64fd51282f75bc73a68af1c66649ffff001d39a59c86",
    ];

    let mut committed_hashes = [0u8; 8 * 32];
    for (i, hex_str) in HEADERS_HEX.iter().enumerate() {
        let mut wire = [0u8; 80];
        for j in 0..80 {
            wire[j] = (decode_hex_nibble(hex_str.as_bytes()[j * 2]) << 4)
                | decode_hex_nibble(hex_str.as_bytes()[j * 2 + 1]);
        }
        let version = u32::from_le_bytes(wire[0..4].try_into().unwrap());
        let mut prev = [0u8; 32];
        prev.copy_from_slice(&wire[4..36]);
        let mut mr = [0u8; 32];
        mr.copy_from_slice(&wire[36..68]);
        let timestamp = u32::from_le_bytes(wire[68..72].try_into().unwrap());
        let compact_target = u32::from_le_bytes(wire[72..76].try_into().unwrap());
        let nonce = u32::from_le_bytes(wire[76..80].try_into().unwrap());

        let hash = of_header(version, &prev, &mr, timestamp, compact_target, nonce);
        assert_eq!(hash, reference_sha256d(&wire));
        committed_hashes[i * 32..(i + 1) * 32].copy_from_slice(&hash);
    }
    zkvm_test::commit_slice(&committed_hashes);
}



