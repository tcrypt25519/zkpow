use super::*;

#[test]
fn parses_network_names_and_aliases() {
    let cases = [
        ("bitcoin", Network::Mainnet),
        ("mainnet", Network::Mainnet),
        ("main", Network::Mainnet),
        ("testnet", Network::Testnet),
        ("test", Network::Testnet),
        ("regtest", Network::Regtest),
        ("reg", Network::Regtest),
        ("BiTcOiN", Network::Mainnet),
    ];

    for (alias, expected) in cases {
        assert_eq!(Network::parse(alias), Some(expected));
    }

    assert_eq!(Network::parse("unknown"), None);
    assert_eq!(Network::parse(""), None);
    assert_eq!(Network::Mainnet.name(), "mainnet");
    assert_eq!(Network::Testnet.name(), "testnet");
    assert_eq!(Network::Regtest.name(), "regtest");
}
