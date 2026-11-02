use std::{error::Error, net::TcpListener};

use strum::IntoEnumIterator;

use super::*;

#[test]
fn command_names_round_trip_through_wire_fields() {
    for command in Command::iter() {
        assert_eq!(Command::from_wire(&command.as_bytes()), command);
    }
}

#[test]
fn unrecognized_wire_names_map_to_unknown() {
    for (wire, expected) in [
        ([b'x'; 12], Command::Unknown),
        ([0; 12], Command::Unknown),
        (*b"version\0\0\0\0x", Command::Version),
    ] {
        assert_eq!(Command::from_wire(&wire), expected, "{wire:?}");
    }
}

fn connection_with_frame(frame: &[u8]) -> PeerConnection {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    server.write_all(frame).unwrap();
    PeerConnection {
        stream,
        network_magic: msg::MAGIC[Network::Mainnet as usize],
    }
}

fn headers_frame(payload: &[u8], checksum: [u8; 4]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&msg::MAGIC[Network::Mainnet as usize]);
    frame.extend_from_slice(&Command::Headers.as_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&checksum);
    frame.extend_from_slice(payload);
    frame
}

#[test]
fn invalid_network_magic_keeps_received_and_expected_bytes() {
    let mut connection = connection_with_frame(&[0; 4]);
    assert!(matches!(
        connection.read_headers(),
        Err(ConnectionError::InvalidMagic { received: [0, 0, 0, 0], expected })
            if expected == msg::MAGIC[Network::Mainnet as usize]
    ));
}

#[test]
fn checksum_mismatch_keeps_command_and_both_checksums() {
    let frame = headers_frame(&[0], [0; 4]);
    let mut connection = connection_with_frame(&frame);
    assert!(matches!(
        connection.read_headers(),
        Err(ConnectionError::ChecksumMismatch { command: Command::Headers, received: [0, 0, 0, 0], expected })
            if expected == create_checksum(&[0])
    ));
}

#[test]
fn truncated_frame_preserves_transport_error_source() {
    let mut connection = connection_with_frame(&[0xF9]);
    let error = connection.read_headers().unwrap_err();
    assert!(matches!(
        &error,
        ConnectionError::Io(source) if source.kind() == std::io::ErrorKind::UnexpectedEof
    ));
    assert!(error.source().unwrap().downcast_ref::<std::io::Error>().is_some());
}

#[test]
fn malformed_headers_preserve_bitcoin_codec_error_source() {
    // A non-minimal CompactSize encoding for a zero header count.
    let payload = [0xFD, 0, 0];
    let frame = headers_frame(&payload, create_checksum(&payload));
    let mut connection = connection_with_frame(&frame);
    let error = connection.read_headers().unwrap_err();
    assert!(matches!(&error, ConnectionError::Codec(encode::Error::NonMinimalVarInt)));
    assert!(error.source().unwrap().downcast_ref::<encode::Error>().is_some());
}

#[test]
fn truncated_version_preserves_bitcoin_io_error_source() {
    let error = VersionMessage::decode(&[]).unwrap_err();
    assert!(matches!(&error, ConnectionError::Codec(encode::Error::Io(_))));
    assert!(error
        .source()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<bitcoin::io::Error>()
        .is_some());
}
