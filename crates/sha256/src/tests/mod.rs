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


