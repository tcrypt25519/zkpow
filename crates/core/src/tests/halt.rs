#[test]
#[should_panic(expected = "halt: invalid hint (3)")]
fn bare_invalid_hint_panics_off_target() {
    crate::invalid_hint!();
}

#[test]
#[should_panic(expected = "halt: invalid hint (3)")]
fn literal_invalid_hint_panics_off_target() {
    crate::invalid_hint!("median hint missing");
}

#[test]
#[should_panic(expected = "halt: invalid hint (3)")]
fn formatted_invalid_hint_panics_off_target() {
    let (expected, actual) = (4usize, 7usize);
    crate::invalid_hint!("expected {expected} hints, got {}", actual);
}

#[test]
fn invalid_hint_is_usable_where_a_diverging_expression_is_required() {
    fn hint(value: Option<u32>) -> u32 {
        let Some(value) = value else {
            crate::invalid_hint!("no hint");
        };
        value
    }

    assert_eq!(hint(Some(9)), 9);
}
