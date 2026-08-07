use zkpow_core::{BlockHash, ChainWork};

use super::*;
use crate::diagnostics::{
    format_claim_mismatch_with_mode, highlight_with_expected_mode, value_prefix, HighlightMode,
};

#[test]
fn highlight_marks_mismatches_and_zero_padding() {
    let cases = [
        (
            "bracket mismatch",
            "00010203",
            "0001ff04",
            HighlightMode::Brackets,
            false,
            "0001[ff04]",
            "    [0203]",
        ),
        (
            "ansi mismatch",
            "00010203",
            "0001ff04",
            HighlightMode::Ansi,
            false,
            "0001\x1b[31mff04\x1b[0m",
            "    \x1b[32m0203\x1b[0m",
        ),
        (
            "bright leading zero bytes",
            "00000102",
            "00000002",
            HighlightMode::Ansi,
            true,
            "\x1b[97m0000\x1b[0m\x1b[31m00\x1b[0m02",
            "    \x1b[32m01\x1b[0m  ",
        ),
    ];

    for (name, expected_value, actual_value, mode, highlight_zeroes, expected_actual_row, expected_expected_row) in
        cases
    {
        let (actual_output, expected_output) =
            highlight_with_expected_mode(expected_value, actual_value, mode, highlight_zeroes);

        assert_eq!(actual_output, expected_actual_row, "{name}: actual row");
        assert_eq!(expected_output, expected_expected_row, "{name}: expected row");
    }
}

#[test]
fn claim_mismatch_splits_32_byte_fields_and_pairs_corrections() {
    let actual = Claim {
        genesis_hash: BlockHash::from_le_bytes([0x11; 32]),
        tip_hash: BlockHash::from_le_bytes([0x22; 32]),
        chain_work: ChainWork::from_le_bytes([0x33; 32]),
        height: 1,
    };
    let expected = Claim {
        tip_hash: BlockHash::from_le_bytes([0x44; 32]),
        height: 2,
        ..actual
    };

    let output = format_claim_mismatch_with_mode(&actual, &expected, HighlightMode::Brackets);

    let tip_hash_value_prefix = value_prefix("tip_hash");
    assert!(output.contains(
        &format!("tip_hash: [22222222222222222222222222222222]\n{tip_hash_value_prefix}[44444444444444444444444444444444]\n\n{tip_hash_value_prefix}[22222222222222222222222222222222]")
    ));
    assert!(output.contains(&format!("{tip_hash_value_prefix}[44444444444444444444444444444444]")));
    assert!(output.contains("height: 000000[01]"));
    assert!(output.contains(&format!("{}[02]", value_prefix("height"))));
    assert!(output.contains("expected claim:"));
    assert!(output
        .contains("tip_hash: 44444444444444444444444444444444\n                44444444444444444444444444444444"));
}

#[test]
fn claim_mismatch_keeps_matching_32_byte_fields_to_two_lines() {
    let claim = Claim {
        genesis_hash: BlockHash::from_le_bytes([0x11; 32]),
        tip_hash: BlockHash::from_le_bytes([0x22; 32]),
        chain_work: ChainWork::from_le_bytes([0x33; 32]),
        height: 1,
    };

    let output = format_claim_mismatch_with_mode(&claim, &claim, HighlightMode::Brackets);

    assert!(output.contains(
        "genesis_hash: 11111111111111111111111111111111\n                    11111111111111111111111111111111,"
    ));
}
