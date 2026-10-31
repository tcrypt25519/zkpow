use crate::connection::Command;

#[test]
fn command_names_round_trip_through_wire_fields() {
    for command in Command::ALL {
        assert_eq!(Command::from_wire(&command.as_bytes()), command);
    }
}

#[test]
fn unrecognized_wire_names_map_to_unknown() {
    assert_eq!(Command::from_wire(&[b'x'; 12]), Command::Unknown);
    assert_eq!(Command::from_wire(&[0; 12]), Command::Unknown);
    assert_eq!(Command::from_wire(b"version\0\0\0\0x"), Command::Version);
}
