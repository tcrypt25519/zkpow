use zkvm_test::zkvm_test;

#[zkvm_test(should_panic)]
fn bare_invalid_hint_panics_off_target() {
    crate::invalid_hint!();
}

#[zkvm_test(should_panic)]
fn literal_invalid_hint_panics_off_target() {
    crate::invalid_hint!("median hint missing");
}

#[zkvm_test(should_panic)]
fn formatted_invalid_hint_panics_off_target() {
    let (expected, actual) = (4usize, 7usize);
    crate::invalid_hint!("expected {expected} hints, got {}", actual);
}

#[zkvm_test]
fn invalid_hint_is_usable_where_a_diverging_expression_is_required() {
    fn hint(value: Option<u32>) -> u32 {
        let Some(value) = value else {
            crate::invalid_hint!("no hint");
        };
        value
    }

    assert_eq!(hint(Some(9)), 9);
}
