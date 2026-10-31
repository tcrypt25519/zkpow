use super::*;

#[test]
fn displays_each_status_from_its_enum_variant() {
    let statuses = [
        (PeerStatus::Unknown, "unknown"),
        (PeerStatus::Connected, "connected"),
        (PeerStatus::Unreachable, "unreachable"),
        (PeerStatus::Disconnected, "disconnected"),
        (PeerStatus::TooSlow, "too_slow"),
    ];

    for (status, expected) in statuses {
        assert_eq!(status.to_string(), expected);
    }
}
