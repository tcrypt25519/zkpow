use crate::config::parse_shard_prover_permits;

#[test]
fn parses_shard_prover_permits() {
    for (input, expected) in [
        (None, 4),
        (Some(""), 4),
        (Some("0"), 4),
        (Some("not-a-number"), 4),
        (Some("-1"), 4),
        (Some("16"), 16),
        (Some("1"), 1),
    ] {
        assert_eq!(parse_shard_prover_permits(input), expected, "{input:?}");
    }
}
