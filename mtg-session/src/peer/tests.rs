use super::*;

#[test]
fn kind_is_total_even_for_ids_outside_the_documented_ranges() {
    // the `EMPTY_CHAT_ID` hole and ids a hostile server can make up must not panic
    for id in [
        EMPTY_CHAT_ID,
        0,
        i64::MIN,
        i64::MAX,
        -2000000000000,
        -4000000000001,
    ] {
        let _ = PeerId(id).kind();
    }
    assert_eq!(PeerId(1).kind(), PeerKind::User);
    assert_eq!(PeerId(-1).kind(), PeerKind::Chat);
    assert_eq!(PeerId(-1000000000001).kind(), PeerKind::Channel);
    assert_eq!(PeerId::self_user().kind(), PeerKind::User);
}
