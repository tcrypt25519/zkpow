use zkvm_test::zkvm_test;

use crate::{assert_hint, assert_hint_eq, assert_hint_ne, invalid_hint, BlockTimestamp, CompactTarget};

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn bare_invalid_hint_panics_off_target() {
    invalid_hint!();
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn literal_invalid_hint_panics_off_target() {
    invalid_hint!("median hint missing");
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn formatted_invalid_hint_panics_off_target() {
    let (expected, actual) = (4usize, 7usize);
    invalid_hint!("expected {expected} hints, got {}", actual);
}

#[zkvm_test]
fn invalid_hint_is_usable_where_a_diverging_expression_is_required() {
    fn hint(value: Option<u32>) -> u32 {
        let Some(value) = value else {
            invalid_hint!("no hint");
        };
        value
    }

    assert_eq!(hint(Some(9)), 9);
}

#[zkvm_test]
fn assert_hint_success_cases() {
    assert_hint!(true);
    assert_hint!(true,);
    assert_hint!(true, "literal diagnostic message");
    assert_hint!(true, "literal diagnostic message",);
    assert_hint!(true, "formatted {} diagnostic with {} args", "hint", 2);
    assert_hint!(true, "formatted {} diagnostic with {} args", "hint", 2,);
    assert_hint!(1 + 1 == 2);
    assert_hint!(!false);
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_failure_bare() {
    assert_hint!(false);
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_failure_literal_message() {
    assert_hint!(false, "custom validation failure");
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_failure_formatted_message() {
    assert_hint!(false, "failed with error code {}", 42);
}

#[zkvm_test]
fn assert_hint_eq_success_cases() {
    assert_hint_eq!(1, 1);
    assert_hint_eq!(1, 1,);
    assert_hint_eq!(1, 1, "literal diagnostic message");
    assert_hint_eq!(1, 1, "literal diagnostic message",);
    assert_hint_eq!(1, 1, "formatted {} diagnostic: {}", "value", 1);
    assert_hint_eq!(1, 1, "formatted {} diagnostic: {}", "value", 1,);
    assert_hint_eq!(&[1u8, 2, 3], &[1u8, 2, 3]);
    assert_hint_eq!(BlockTimestamp::new(500), BlockTimestamp::new(500));
    assert_hint_eq!(CompactTarget::new(0x1d00ffff), CompactTarget::new(0x1d00ffff));
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_eq_failure_default_message() {
    assert_hint_eq!(1, 2);
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_eq_failure_literal_message() {
    assert_hint_eq!(1, 2, "median hint value mismatch");
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_eq_failure_formatted_message() {
    assert_hint_eq!(1, 2, "expected {} but got {}", 1, 2);
}

#[zkvm_test]
fn assert_hint_ne_success_cases() {
    assert_hint_ne!(1, 2);
    assert_hint_ne!(1, 2,);
    assert_hint_ne!(1, 2, "literal diagnostic message");
    assert_hint_ne!(1, 2, "literal diagnostic message",);
    assert_hint_ne!(1, 2, "formatted {} diagnostic: {}", "value", 1);
    assert_hint_ne!(1, 2, "formatted {} diagnostic: {}", "value", 1,);
    assert_hint_ne!(&[1u8, 2], &[3u8, 4]);
    assert_hint_ne!(BlockTimestamp::new(100), BlockTimestamp::new(200));
    assert_hint_ne!(CompactTarget::new(0x1d00ffff), CompactTarget::new(0x1d00fffe));
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_ne_failure_default_message() {
    assert_hint_ne!(1, 1);
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_ne_failure_literal_message() {
    assert_hint_ne!(1, 1, "values must not be identical");
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn assert_hint_ne_failure_formatted_message() {
    assert_hint_ne!(1, 1, "both operands evaluated to {}", 1);
}
