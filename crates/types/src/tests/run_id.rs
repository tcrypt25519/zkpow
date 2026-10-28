use zkvm_test::test;

use crate::run_id::{RunId, RunIdError};

#[test]
fn valid_from_bytes() {
    let id = RunId::from_bytes(*b"12345678").unwrap();
    assert_eq!(id.as_str(), "12345678");
    assert_eq!(id.to_bytes(), *b"12345678");

    let id2 = RunId::new(*b"aBcDeF90").unwrap();
    assert_eq!(id2.as_str(), "aBcDeF90");
    assert_eq!(id2.as_bytes(), b"aBcDeF90");
}

#[test]
fn reject_invalid_characters_in_bytes() {
    let invalid_chars = [
        b'-', b'_', b'/', b'\\', b'.', b' ', b'\0', b':', b';', b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(',
        b')', 128, 255,
    ];

    for &ch in &invalid_chars {
        let mut arr = *b"valid001";
        arr[4] = ch;
        assert_eq!(
            RunId::from_bytes(arr),
            Err(RunIdError::InvalidCharacter(ch)),
            "should reject byte 0x{ch:02x}"
        );
    }
}

#[test]
fn valid_from_str() {
    let id = RunId::from_str("localdev").unwrap();
    assert_eq!(id.as_str(), "localdev");
    assert_eq!(&*id, "localdev");

    let id2 = "ABC123xy".parse::<RunId>().unwrap();
    assert_eq!(id2.as_str(), "ABC123xy");

    let id3 = RunId::try_from("00000000").unwrap();
    assert_eq!(id3.as_str(), "00000000");
}

#[test]
fn reject_invalid_length_in_str() {
    let lengths = [
        "",
        "a",
        "ab",
        "abc",
        "abcd",
        "abcde",
        "abcdef",
        "abcdefg",
        "123456789",
        "toolongrunid",
    ];
    for s in lengths {
        assert_eq!(
            RunId::from_str(s),
            Err(RunIdError::InvalidLength),
            "string {s:?} should fail with InvalidLength"
        );
    }
}

#[test]
fn reject_symbols_and_traversal_in_str() {
    let bad_strings = [
        "run-0001",
        "run_0001",
        "run/0001",
        "run\\0001",
        "run..001",
        "../run01",
        "/escape1",
        "run.0001",
        "run 0001",
    ];

    for s in bad_strings {
        assert!(
            matches!(RunId::from_str(s), Err(RunIdError::InvalidCharacter(_))),
            "string {s:?} should fail with InvalidCharacter"
        );
    }
}

#[test]
fn formatting_and_traits() {
    extern crate alloc;
    use alloc::format;

    let id = RunId::from_str("run00001").unwrap();
    assert_eq!(format!("{id}"), "run00001");
    assert_eq!(format!("{id:?}"), "RunId(run00001)");

    // Clone, Copy, Eq, Ord
    let id_copy = id;
    assert_eq!(id, id_copy);

    let id_a = RunId::from_str("aaaa1111").unwrap();
    let id_b = RunId::from_str("bbbb2222").unwrap();
    assert!(id_a < id_b);
}
