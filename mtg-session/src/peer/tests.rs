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

use mtg_tl_types as tl;
use mtg_tl_types::Deserializable;

fn user(flags: u32, id: i64, hash: Option<i64>) -> tl::types::User {
    let mut b = Vec::new();
    b.extend(flags.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(id.to_le_bytes());
    if let Some(h) = hash {
        b.extend(h.to_le_bytes());
    }
    if flags & BOT != 0 {
        b.extend(0i32.to_le_bytes()); // bot_info_version
    }
    tl::types::User::from_bytes(&b).unwrap()
}

fn channel(flags: u32, id: i64, hash: Option<i64>) -> tl::types::Channel {
    let mut b = Vec::new();
    b.extend(flags.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(id.to_le_bytes());
    if let Some(h) = hash {
        b.extend(h.to_le_bytes());
    }
    b.extend([0, 0, 0, 0]); // title ""
    b.extend(0x37c1011cu32.to_le_bytes()); // chatPhotoEmpty
    b.extend(0i32.to_le_bytes());
    tl::types::Channel::from_bytes(&b).unwrap()
}

const SELF: u32 = 1 << 10;
const BOT: u32 = 1 << 14;
const HASH: u32 = 1 << 0;
const C_MIN: u32 = 1 << 12;
const C_HASH: u32 = 1 << 13;

#[test]
fn peer_ids_convert_to_and_from_tl_peers() {
    let ids = [
        PeerId::user_unchecked(5),
        PeerId::chat_unchecked(6),
        PeerId::channel_unchecked(7),
    ];
    for id in ids {
        let peer = tl::enums::Peer::from(id);
        assert_eq!(PeerId::from(&peer), id);
        assert_eq!(PeerId::from(peer), id);
        assert_eq!(tl::enums::Peer::from(&id), tl::enums::Peer::from(id));
    }
    assert_eq!(
        PeerId::from(tl::types::PeerUser { user_id: 5 }),
        PeerId::user_unchecked(5)
    );
    assert_eq!(
        PeerId::from(tl::types::PeerChat { chat_id: 6 }),
        PeerId::chat_unchecked(6)
    );
    assert_eq!(
        PeerId::from(tl::types::PeerChannel { channel_id: 7 }),
        PeerId::channel_unchecked(7)
    );
    assert_eq!(PeerId::user_unchecked(5).to_string(), "5");
    assert_eq!(PeerId::self_user().to_string(), "<self-user>");
}

#[test]
fn peer_info_from_users() {
    let full = PeerInfo::from(user(HASH | SELF | BOT, 1, Some(9)));
    assert_eq!(
        full,
        PeerInfo::User {
            id: 1,
            auth: Some(PeerAuth::from_hash(9)),
            bot: Some(true),
            is_self: Some(true)
        }
    );
    assert_eq!(full.id(), PeerId::user_unchecked(1));
    assert_eq!(full.auth(), Some(PeerAuth::from_hash(9)));
    // min users carry a hash that is not usable
    let min = PeerInfo::from(&user(HASH | (1 << 20), 1, Some(9)));
    assert_eq!(min.auth(), None);
    let empty = PeerInfo::from(tl::types::UserEmpty { id: 2 });
    assert_eq!(
        empty,
        PeerInfo::User {
            id: 2,
            auth: None,
            bot: None,
            is_self: None
        }
    );
    let en = tl::enums::User::Empty(tl::types::UserEmpty { id: 2 });
    assert_eq!(PeerInfo::from(&en), empty);
    assert_eq!(PeerInfo::from(en), empty);
    let en = tl::enums::User::User(user(0, 3, None));
    assert_eq!(PeerInfo::from(en).id(), PeerId::user_unchecked(3));
}

#[test]
fn peer_info_from_chats_and_channels() {
    let chat = PeerInfo::Chat { id: 4 };
    assert_eq!(chat.auth(), Some(PeerAuth::default()));
    assert_eq!(PeerInfo::from(tl::types::ChatEmpty { id: 4 }), chat);
    assert_eq!(PeerInfo::from(&tl::types::ChatEmpty { id: 4 }), chat);
    let forbidden = tl::types::ChatForbidden {
        id: 4,
        title: String::new(),
    };
    assert_eq!(PeerInfo::from(&forbidden), chat);
    assert_eq!(PeerInfo::from(forbidden.clone()), chat);
    assert_eq!(PeerInfo::from(tl::enums::Chat::Forbidden(forbidden)), chat);
    assert_eq!(
        PeerInfo::from(tl::enums::Chat::Empty(tl::types::ChatEmpty { id: 4 })),
        chat
    );

    let ch = PeerInfo::from(channel((1 << 5) | C_HASH, 8, Some(3)));
    assert_eq!(
        ch,
        PeerInfo::Channel {
            id: 8,
            auth: Some(PeerAuth::from_hash(3)),
            kind: Some(ChannelKind::Broadcast)
        }
    );
    assert_eq!(ch.id(), PeerId::channel_unchecked(8));
    let kinds = [
        ((1 << 8), Some(ChannelKind::Megagroup)),
        ((1 << 26) | (1 << 5), Some(ChannelKind::Gigagroup)),
        (0, None),
    ];
    for (flags, kind) in kinds {
        let c = channel(flags, 8, None);
        assert_eq!(ChannelKind::try_from(&c).ok(), kind);
        assert_eq!(ChannelKind::try_from(c.clone()).ok(), kind);
        assert_eq!(PeerInfo::from(c).auth(), None);
    }
    // min channel hides its hash
    assert_eq!(
        PeerInfo::from(channel(C_MIN | C_HASH, 8, Some(3))).auth(),
        None
    );
    let en = tl::enums::Chat::Channel(channel(1 << 5, 8, None));
    assert_eq!(PeerInfo::from(&en).id(), PeerId::channel_unchecked(8));
    assert_eq!(PeerInfo::from(en).id(), PeerId::channel_unchecked(8));
}

fn channel_forbidden(flags: u32, id: i64) -> tl::types::ChannelForbidden {
    tl::types::ChannelForbidden {
        broadcast: flags & 1 != 0,
        megagroup: flags & 2 != 0,
        monoforum: false,
        id,
        access_hash: 11,
        title: String::new(),
        until_date: None,
    }
}

#[test]
fn forbidden_channels_and_communities() {
    for (flags, kind) in [
        (1, Some(ChannelKind::Broadcast)),
        (2, Some(ChannelKind::Megagroup)),
        (0, None),
    ] {
        let c = channel_forbidden(flags, 8);
        assert_eq!(ChannelKind::try_from(&c).ok(), kind);
        assert_eq!(ChannelKind::try_from(c.clone()).ok(), kind);
        let info = PeerInfo::from(&c);
        assert_eq!(info, PeerInfo::from(c.clone()));
        assert_eq!(info.auth(), Some(PeerAuth::from_hash(11)));
        assert_eq!(
            PeerInfo::from(tl::enums::Chat::ChannelForbidden(c.clone())),
            info
        );
        let r = PeerRef::from(&c);
        assert_eq!(r, PeerRef::from(c));
        assert_eq!(r.auth.hash(), 11);
    }
    let cf = |h| tl::types::CommunityForbidden {
        id: 12,
        access_hash: h,
        title: String::new(),
    };
    let info = PeerInfo::from(cf(Some(5)));
    assert_eq!(
        info,
        PeerInfo::Channel {
            id: 12,
            auth: Some(PeerAuth::from_hash(5)),
            kind: Some(ChannelKind::Community)
        }
    );
    assert_eq!(PeerInfo::from(&cf(None)).auth(), None);
    assert_eq!(
        PeerInfo::from(tl::enums::Chat::CommunityForbidden(cf(None))).id(),
        PeerId::channel_unchecked(12)
    );
    assert_eq!(PeerRef::from(&cf(Some(5))).auth.hash(), 5);
    assert_eq!(PeerRef::from(cf(None)).auth, PeerAuth::default());
    let en = tl::enums::Chat::CommunityForbidden(cf(Some(5)));
    assert_eq!(PeerRef::from(&en), PeerRef::from(en.clone()));
}

#[test]
fn extend_info_merges_matching_peers_only() {
    let mut u = PeerInfo::User {
        id: 1,
        auth: None,
        bot: None,
        is_self: Some(false),
    };
    let new = PeerInfo::User {
        id: 1,
        auth: Some(PeerAuth::from_hash(2)),
        bot: Some(true),
        is_self: None,
    };
    assert!(u.extend_info(&new));
    assert_eq!(
        u,
        PeerInfo::User {
            id: 1,
            auth: Some(PeerAuth::from_hash(2)),
            bot: Some(true),
            is_self: Some(false)
        }
    );
    assert!(!u.extend_info(&PeerInfo::User {
        id: 2,
        auth: None,
        bot: None,
        is_self: None
    }));
    assert!(!u.extend_info(&PeerInfo::Chat { id: 1 }));
    let mut c = PeerInfo::Chat { id: 1 };
    assert!(c.extend_info(&PeerInfo::Chat { id: 1 }));
    assert!(!c.extend_info(&PeerInfo::Chat { id: 2 }));
    let mut ch = PeerInfo::Channel {
        id: 1,
        auth: None,
        kind: None,
    };
    let new = PeerInfo::Channel {
        id: 1,
        auth: Some(PeerAuth::from_hash(4)),
        kind: Some(ChannelKind::Megagroup),
    };
    assert!(ch.extend_info(&new));
    assert_eq!(ch, new);
    assert!(!ch.extend_info(&PeerInfo::Channel {
        id: 3,
        auth: None,
        kind: None
    }));
}

#[test]
fn peer_refs_from_tl_values_and_back() {
    use tl::enums::InputPeer as IP;
    let h = 77;
    let cases: Vec<(IP, PeerRef)> = vec![
        (IP::PeerSelf, PeerId::self_user().to_ambient_ref()),
        (
            IP::User(tl::types::InputPeerUser {
                user_id: 1,
                access_hash: h,
            }),
            PeerRef {
                id: PeerId::user_unchecked(1),
                auth: PeerAuth::from_hash(h),
            },
        ),
        (
            IP::Chat(tl::types::InputPeerChat { chat_id: 2 }),
            PeerRef {
                id: PeerId::chat_unchecked(2),
                auth: PeerAuth::default(),
            },
        ),
        (
            IP::Channel(tl::types::InputPeerChannel {
                channel_id: 3,
                access_hash: h,
            }),
            PeerRef {
                id: PeerId::channel_unchecked(3),
                auth: PeerAuth::from_hash(h),
            },
        ),
    ];
    for (input, peer) in cases {
        assert_eq!(PeerRef::from(&input), peer);
        assert_eq!(PeerRef::from(input.clone()), peer);
        assert_eq!(IP::from(&peer), input);
        assert_eq!(IP::from(peer), input);
    }
    // from-message variants and the empty input
    let peer = IP::Empty;
    let _ = peer; // `Empty` is covered by the match arm below
    let msg_user = tl::types::InputPeerUserFromMessage {
        peer: tl::enums::InputPeer::Empty,
        msg_id: 1,
        user_id: 5,
    };
    let r = PeerRef::from(&msg_user);
    assert_eq!(r.id, PeerId::user_unchecked(5));
    assert_eq!(PeerRef::from(msg_user).auth, PeerAuth::default());
    let msg_ch = tl::types::InputPeerChannelFromMessage {
        peer: tl::enums::InputPeer::Empty,
        msg_id: 1,
        channel_id: 6,
    };
    assert_eq!(PeerRef::from(&msg_ch).id, PeerId::channel_unchecked(6));
    assert_eq!(PeerRef::from(msg_ch).auth, PeerAuth::default());
    assert_eq!(
        PeerRef::from(tl::types::InputPeerSelf {}),
        PeerId::self_user().to_ambient_ref()
    );
    assert_eq!(
        PeerRef::from(&tl::types::InputPeerSelf {}),
        PeerId::self_user().to_ambient_ref()
    );
    assert_eq!(
        PeerRef::from(tl::types::InputPeerUser {
            user_id: 1,
            access_hash: 2
        })
        .auth
        .hash(),
        2
    );
    assert_eq!(
        PeerRef::from(tl::types::InputPeerChat { chat_id: 1 }).id,
        PeerId::chat_unchecked(1)
    );
    assert_eq!(
        PeerRef::from(tl::types::InputPeerChannel {
            channel_id: 1,
            access_hash: 2
        })
        .auth
        .hash(),
        2
    );
}

#[test]
fn peer_refs_from_entities() {
    let u = user(HASH, 1, Some(9));
    let r = PeerRef::from(&u);
    assert_eq!(r, PeerRef::from(u.clone()));
    assert_eq!(r.auth.hash(), 9);
    assert_eq!(PeerRef::from(&user(0, 1, None)).auth, PeerAuth::default());
    let en = tl::enums::User::User(u);
    assert_eq!(PeerRef::from(&en), r);
    assert_eq!(PeerRef::from(en), r);
    let e = tl::types::UserEmpty { id: 4 };
    assert_eq!(PeerRef::from(&e), PeerRef::from(e.clone()));
    let en = tl::enums::User::Empty(e);
    assert_eq!(PeerRef::from(&en).id, PeerId::user_unchecked(4));
    assert_eq!(PeerRef::from(en).id, PeerId::user_unchecked(4));

    let ce = tl::types::ChatEmpty { id: 5 };
    assert_eq!(PeerRef::from(&ce), PeerRef::from(ce.clone()));
    let cf = tl::types::ChatForbidden {
        id: 5,
        title: String::new(),
    };
    assert_eq!(PeerRef::from(&cf), PeerRef::from(cf.clone()));
    for en in [tl::enums::Chat::Empty(ce), tl::enums::Chat::Forbidden(cf)] {
        assert_eq!(PeerRef::from(&en).id, PeerId::chat_unchecked(5));
        assert_eq!(PeerRef::from(en).auth, PeerAuth::default());
    }
    let ch = channel(C_HASH, 6, Some(3));
    assert_eq!(PeerRef::from(&ch), PeerRef::from(ch.clone()));
    assert_eq!(PeerRef::from(&ch).auth.hash(), 3);
    assert_eq!(
        PeerRef::from(&channel(0, 6, None)).auth,
        PeerAuth::default()
    );
    let en = tl::enums::Chat::Channel(ch);
    assert_eq!(PeerRef::from(&en).id, PeerId::channel_unchecked(6));
    assert_eq!(PeerRef::from(en).id, PeerId::channel_unchecked(6));
    let en = tl::enums::Chat::ChannelForbidden(channel_forbidden(0, 6));
    assert_eq!(PeerRef::from(&en).auth.hash(), 11);
}

#[test]
fn derived_inputs_follow_the_peer_kind() {
    let user = PeerRef {
        id: PeerId::user_unchecked(1),
        auth: PeerAuth::from_hash(2),
    };
    let chat = PeerRef {
        id: PeerId::chat_unchecked(3),
        auth: PeerAuth::default(),
    };
    let chan = PeerRef {
        id: PeerId::channel_unchecked(4),
        auth: PeerAuth::from_hash(5),
    };
    let me = PeerId::self_user().to_ambient_ref();
    assert!(
        matches!(tl::enums::InputUser::from(user), tl::enums::InputUser::User(u) if u.user_id == 1 && u.access_hash == 2)
    );
    assert!(matches!(
        tl::enums::InputUser::from(&me),
        tl::enums::InputUser::UserSelf
    ));
    assert!(matches!(
        tl::enums::InputUser::from(chat),
        tl::enums::InputUser::Empty
    ));
    assert!(matches!(
        tl::enums::InputUser::from(chan),
        tl::enums::InputUser::Empty
    ));
    assert_eq!(i64::from(chat), 3);
    assert_eq!(i64::from(user), EMPTY_CHAT_ID);
    assert_eq!(i64::from(chan), EMPTY_CHAT_ID);
    assert!(
        matches!(tl::enums::InputChannel::from(chan), tl::enums::InputChannel::Channel(c) if c.channel_id == 4 && c.access_hash == 5)
    );
    assert!(matches!(
        tl::enums::InputChannel::from(user),
        tl::enums::InputChannel::Empty
    ));
    assert!(matches!(
        tl::enums::InputChannel::from(chat),
        tl::enums::InputChannel::Empty
    ));
}

fn chat(id: i64) -> tl::types::Chat {
    let mut b = 0u32.to_le_bytes().to_vec();
    b.extend(id.to_le_bytes());
    b.extend([0, 0, 0, 0]); // title
    b.extend(0x37c1011cu32.to_le_bytes()); // chatPhotoEmpty
    b.extend([0; 12]); // participants_count, date, version
    tl::types::Chat::from_bytes(&b).unwrap()
}

fn community(flags: u32, id: i64) -> tl::types::Community {
    let mut b = flags.to_le_bytes().to_vec();
    b.extend(0u32.to_le_bytes());
    b.extend(id.to_le_bytes());
    if flags & C_HASH != 0 {
        b.extend(3i64.to_le_bytes());
    }
    b.extend([0, 0, 0, 0]); // title
    b.extend(0x37c1011cu32.to_le_bytes());
    b.extend(0i32.to_le_bytes()); // date
    tl::types::Community::from_bytes(&b).unwrap()
}

#[test]
fn chats_and_communities_convert() {
    let c = chat(5);
    assert_eq!(PeerInfo::from(&c), PeerInfo::Chat { id: 5 });
    assert_eq!(PeerInfo::from(c.clone()), PeerInfo::Chat { id: 5 });
    assert_eq!(PeerRef::from(&c), PeerRef::from(c.clone()));
    let en = tl::enums::Chat::Chat(c);
    assert_eq!(PeerInfo::from(&en), PeerInfo::Chat { id: 5 });
    assert_eq!(PeerRef::from(&en).id, PeerId::chat_unchecked(5));

    let with = community(C_HASH, 6);
    let info = PeerInfo::from(&with);
    assert_eq!(info, PeerInfo::from(with.clone()));
    assert_eq!(info.auth(), Some(PeerAuth::from_hash(3)));
    assert_eq!(PeerRef::from(&with), PeerRef::from(with.clone()));
    assert_eq!(PeerRef::from(&with).auth.hash(), 3);
    assert_eq!(PeerInfo::from(&community(C_HASH | C_MIN, 6)).auth(), None);
    assert_eq!(PeerRef::from(&community(0, 6)).auth, PeerAuth::default());
    let en = tl::enums::Chat::Community(with);
    assert_eq!(PeerInfo::from(&en), info);
    assert_eq!(PeerRef::from(&en).id, PeerId::channel_unchecked(6));
}

#[test]
fn peer_refs_from_message_inputs_and_the_empty_input() {
    let u = tl::enums::InputPeer::UserFromMessage(Box::new(tl::types::InputPeerUserFromMessage {
        peer: tl::enums::InputPeer::Empty,
        msg_id: 1,
        user_id: 5,
    }));
    assert_eq!(PeerRef::from(&u).id, PeerId::user_unchecked(5));
    let c = tl::enums::InputPeer::ChannelFromMessage(Box::new(
        tl::types::InputPeerChannelFromMessage {
            peer: tl::enums::InputPeer::Empty,
            msg_id: 1,
            channel_id: 6,
        },
    ));
    assert_eq!(PeerRef::from(&c).id, PeerId::channel_unchecked(6));
}

#[test]
#[should_panic(expected = "InputPeer::Empty")]
fn an_empty_input_peer_has_no_peer_ref() {
    let _ = PeerRef::from(&tl::enums::InputPeer::Empty);
}

#[test]
fn checked_peer_id_constructors_respect_the_ranges() {
    assert_eq!(PeerId::user(5), Some(PeerId::user_unchecked(5)));
    assert_eq!(PeerId::user(0), None);
    assert_eq!(PeerId::chat(5), Some(PeerId::chat_unchecked(5)));
    assert_eq!(PeerId::chat(0), None);
    assert_eq!(PeerId::channel(5), Some(PeerId::channel_unchecked(5)));
    assert_eq!(
        PeerId::channel(1002147483649),
        Some(PeerId::channel_unchecked(1002147483649))
    );
    assert_eq!(PeerId::channel(0), None);
    for ok in [5, -5, -1000000000005, -3000000000000] {
        assert_eq!(
            PeerId::from_bot_api_dialog_id(ok).and_then(|p| p.bot_api_dialog_id()),
            Some(ok)
        );
    }
    for bad in [0, -1000000000000, -5000000000000, 0x10000000000] {
        assert_eq!(PeerId::from_bot_api_dialog_id(bad), None);
    }
}
