extern crate std;

use std::string::ToString;

use crate::{BlockHash, BlockTimestamp, CompactTarget, Header};

fn sample() -> Header {
    Header {
        version:        0x1122_3344,
        prev_blockhash: BlockHash::new([0x55; 32]),
        merkle_root:    [0x66; 32],
        timestamp:      BlockTimestamp::new(0x7788_99aa),
        compact_target: CompactTarget::new(0x1d00_ffff),
        nonce:          0xdead_beef,
    }
}

/// `to_bytes_const` respells the layout that `as_bytes` gets from `repr(C)`. If they ever drift,
/// every network's genesis hash silently changes.
#[test]
fn header_serializations_agree() {
    let header = sample();
    assert_eq!(header.to_bytes_const().as_slice(), header.as_bytes());
    assert_eq!(header.as_bytes().len(), Header::SIZE);
}

#[test]
fn header_round_trips_through_parse() {
    let header = sample();
    assert_eq!(Header::parse(header.as_bytes()).unwrap(), &header);
}

/// Mainnet block 1, whose hash is `00000000839a8e...` in display order.
#[test]
fn block_hash_is_sha256d_of_the_wire_bytes() {
    let header = Header {
        version:        1,
        prev_blockhash: BlockHash::new([
            0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72, 0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f, 0x93, 0x1e,
            0x83, 0x65, 0xe1, 0x5a, 0x08, 0x9c, 0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]),
        merkle_root:    [
            0x98, 0x20, 0x51, 0xfd, 0x1e, 0x4b, 0xa7, 0x44, 0xbb, 0xbe, 0x68, 0x0e, 0x1f, 0xee, 0x14, 0x67, 0x7b, 0xa1,
            0xa3, 0xc3, 0x54, 0x0b, 0xf7, 0xb1, 0xcd, 0xb6, 0x06, 0xe8, 0x57, 0x23, 0x3e, 0x0e,
        ],
        timestamp:      BlockTimestamp::new(1231469665),
        compact_target: CompactTarget::new(0x1d00ffff),
        nonce:          2573394689,
    };

    assert_eq!(
        BlockHash::from(&header).to_string(),
        "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048"
    );
}
