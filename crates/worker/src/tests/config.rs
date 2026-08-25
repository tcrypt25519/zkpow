use crate::config::parse_shard_prover_permits;

#[test]
fn defaults_when_unset() {
    assert_eq!(parse_shard_prover_permits(None), 4);
}

#[test]
fn defaults_when_empty() {
    assert_eq!(parse_shard_prover_permits(Some("")), 4);
}

#[test]
fn defaults_when_zero() {
    assert_eq!(parse_shard_prover_permits(Some("0")), 4);
}

#[test]
fn defaults_when_not_a_number() {
    assert_eq!(parse_shard_prover_permits(Some("not-a-number")), 4);
}

#[test]
fn defaults_when_negative() {
    assert_eq!(parse_shard_prover_permits(Some("-1")), 4);
}

#[test]
fn uses_valid_positive_value() {
    assert_eq!(parse_shard_prover_permits(Some("16")), 16);
}

#[test]
fn uses_value_of_one() {
    assert_eq!(parse_shard_prover_permits(Some("1")), 1);
}
