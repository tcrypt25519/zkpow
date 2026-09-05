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
            prev_blockhash,
            merkle_root,
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
        prev_blockhash,
        merkle_root,
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
        let actual = of_header(v, prev, mr, ts, target, nonce);
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
        let actual = of_header(v, prev, mr, ts, target, nonce);
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



