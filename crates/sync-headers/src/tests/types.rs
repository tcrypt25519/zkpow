use super::*;

#[test]
fn parses_network_names_and_aliases() {
    let cases = [
        ("mainnet", Network::Mainnet),
        ("testnet", Network::Testnet),
        ("regtest", Network::Regtest),
    ];

    for (alias, expected) in cases {
        assert_eq!(Network::parse(alias), Some(expected));
    }

    assert_eq!(Network::parse("unknown"), None);
    assert_eq!(Network::parse(""), None);
    assert_eq!(Network::Mainnet.to_string(), "mainnet");
    assert_eq!(Network::Testnet.to_string(), "testnet");
    assert_eq!(Network::Regtest.to_string(), "regtest");
}
