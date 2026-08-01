use zkpow_core::Claim;

const ACTUAL_ANSI_START: &str = "\x1b[31m";
const EXPECTED_ANSI_START: &str = "\x1b[32m";
const LEADING_ZERO_ANSI_START: &str = "\x1b[97m";
const ANSI_END: &str = "\x1b[0m";
const BRACKET_START: &str = "[";
const BRACKET_END: &str = "]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightMode {
    Ansi,
    Brackets,
}

trait ClaimHex {
    fn genesis_hash_display_hex(&self) -> String;
    fn tip_hash_display_hex(&self) -> String;
    fn chain_work_display_hex(&self) -> String;
    fn height_display_hex(&self) -> String;
}

impl ClaimHex for Claim {
    fn genesis_hash_display_hex(&self) -> String {
        display_hex(self.genesis_hash.to_le_bytes())
    }

    fn tip_hash_display_hex(&self) -> String {
        display_hex(self.tip_hash.to_le_bytes())
    }

    fn chain_work_display_hex(&self) -> String {
        display_hex(self.chain_work.to_le_bytes())
    }

    fn height_display_hex(&self) -> String {
        hex::encode(self.height.to_be_bytes())
    }
}

fn format_claim_pretty(claim: &Claim) -> String {
    let mut output = String::new();
    output.push_str("    Claim {\n");
    push_claim_field(&mut output, "genesis_hash", &claim.genesis_hash_display_hex());
    push_claim_field(&mut output, "tip_hash", &claim.tip_hash_display_hex());
    push_claim_field(&mut output, "chain_work", &claim.chain_work_display_hex());
    push_claim_field(&mut output, "height", &claim.height_display_hex());
    output.push_str("    }");
    output
}

fn push_claim_field(output: &mut String, name: &str, value: &str) {
    let prefix = field_prefix(name);
    if value.len() == 64 {
        output.push_str(&prefix);
        output.push_str(&value[..32]);
        output.push('\n');
        output.push_str(&value_prefix(name));
        output.push_str(&value[32..]);
        output.push_str(",\n");
    } else {
        output.push_str(&prefix);
        output.push_str(value);
        output.push_str(",\n");
    }
}

pub fn format_claim_mismatch(actual: &Claim, expected: &Claim) -> String {
    format_claim_mismatch_with_mode(actual, expected, highlight_mode_from_env())
}

pub fn format_claim_mismatch_with_mode(actual: &Claim, expected: &Claim, mode: HighlightMode) -> String {
    let mut output = String::new();
    output.push_str("  actual claim:\n");
    output.push_str("    Claim {\n");
    push_claim_mismatch_field(
        &mut output,
        "genesis_hash",
        &actual.genesis_hash_display_hex(),
        &expected.genesis_hash_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "tip_hash",
        &actual.tip_hash_display_hex(),
        &expected.tip_hash_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "chain_work",
        &actual.chain_work_display_hex(),
        &expected.chain_work_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "height",
        &actual.height_display_hex(),
        &expected.height_display_hex(),
        mode,
    );
    output.push_str("    }\n");
    output.push_str("  expected claim:\n");
    output.push_str(&format_claim_pretty(expected));
    output
}

fn push_claim_mismatch_field(output: &mut String, name: &str, actual: &str, expected: &str, mode: HighlightMode) {
    if actual.len() == 64 {
        push_wide_claim_mismatch_field(output, name, actual, expected, mode);
    } else {
        let has_mismatch = actual != expected;
        let (actual, expected) = highlight_with_expected_mode(expected, actual, mode, actual.len() == 8);
        output.push_str(&field_prefix(name));
        output.push_str(&actual);
        output.push_str(",\n");
        if has_mismatch {
            output.push_str(&value_prefix(name));
            output.push_str(&expected);
            output.push('\n');
        }
    }
}

fn push_wide_claim_mismatch_field(output: &mut String, name: &str, actual: &str, expected: &str, mode: HighlightMode) {
    let first_actual = &actual[..32];
    let first_expected = &expected[..32];
    let second_actual = &actual[32..];
    let second_expected = &expected[32..];
    let first_mismatch = first_actual != first_expected;
    let second_mismatch = second_actual != second_expected;
    let has_mismatch = first_mismatch || second_mismatch;

    let (first_actual, first_expected) = highlight_with_expected_mode(first_expected, first_actual, mode, false);
    let (second_actual, second_expected) = highlight_with_expected_mode(second_expected, second_actual, mode, false);

    output.push_str(&field_prefix(name));
    output.push_str(&first_actual);
    output.push('\n');
    if first_mismatch {
        output.push_str(&value_prefix(name));
        output.push_str(&first_expected);
        output.push('\n');
    }
    if has_mismatch {
        output.push('\n');
    }
    output.push_str(&value_prefix(name));
    output.push_str(&second_actual);
    output.push_str(",\n");
    if second_mismatch {
        output.push_str(&value_prefix(name));
        output.push_str(&second_expected);
        output.push('\n');
    }
}

pub fn highlight_with_expected_mode(
    expected: &str,
    actual: &str,
    mode: HighlightMode,
    highlight_leading_zeros: bool,
) -> (String, String) {
    assert_eq!(expected.len(), actual.len());
    assert_eq!(expected.len() % 2, 0);

    let mut actual_out = String::with_capacity(actual.len() * 2);
    let mut expected_out = String::with_capacity(expected.len() * 2);
    let mut in_mismatch_run = false;
    let mut in_leading_zero_run = false;
    let mut still_leading_zeros = highlight_leading_zeros;

    for i in (0..actual.len()).step_by(2) {
        let exp = &expected[i..i + 2];
        let act = &actual[i..i + 2];

        if exp == act {
            if in_mismatch_run {
                actual_out.push_str(mode.end());
                expected_out.push_str(mode.end());
                in_mismatch_run = false;
            }
            if still_leading_zeros && exp == "00" && mode == HighlightMode::Ansi {
                if !in_leading_zero_run {
                    actual_out.push_str(LEADING_ZERO_ANSI_START);
                    in_leading_zero_run = true;
                }
            } else {
                if in_leading_zero_run {
                    actual_out.push_str(ANSI_END);
                    in_leading_zero_run = false;
                }
                still_leading_zeros = false;
            }
            actual_out.push_str(act);
            expected_out.push_str("  ");
        } else {
            if in_leading_zero_run {
                actual_out.push_str(ANSI_END);
                in_leading_zero_run = false;
            }
            still_leading_zeros = false;
            if !in_mismatch_run {
                actual_out.push_str(mode.actual_start());
                expected_out.push_str(mode.expected_start());
                in_mismatch_run = true;
            }
            actual_out.push_str(act);
            expected_out.push_str(exp);
        }
    }

    if in_leading_zero_run {
        actual_out.push_str(ANSI_END);
    }
    if in_mismatch_run {
        actual_out.push_str(mode.end());
        expected_out.push_str(mode.end());
    }

    (actual_out, expected_out)
}

impl HighlightMode {
    fn actual_start(self) -> &'static str {
        match self {
            Self::Ansi => ACTUAL_ANSI_START,
            Self::Brackets => BRACKET_START,
        }
    }

    fn expected_start(self) -> &'static str {
        match self {
            Self::Ansi => EXPECTED_ANSI_START,
            Self::Brackets => BRACKET_START,
        }
    }

    fn end(self) -> &'static str {
        match self {
            Self::Ansi => ANSI_END,
            Self::Brackets => BRACKET_END,
        }
    }
}

fn highlight_mode_from_env() -> HighlightMode {
    match std::env::var_os("CLICOLOR") {
        Some(value) if value.to_string_lossy() != "0" => HighlightMode::Ansi,
        _ => HighlightMode::Brackets,
    }
}

fn field_prefix(name: &str) -> String {
    format!("      {name}: ")
}

pub fn value_prefix(name: &str) -> String {
    format!("      {}  ", " ".repeat(name.len()))
}

fn display_hex<const N: usize>(mut bytes: [u8; N]) -> String {
    bytes.reverse();
    hex::encode(bytes)
}

