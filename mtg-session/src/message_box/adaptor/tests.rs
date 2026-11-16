use mtg_tl_types::Deserializable;
use mtg_tl_types::enums::Update as U;

use super::*;

fn peer_user(id: i64) -> tl::enums::Peer {
    tl::types::PeerUser { user_id: id }.into()
}
fn peer_channel(id: i64) -> tl::enums::Peer {
    tl::types::PeerChannel { channel_id: id }.into()
}
fn msg(peer: Option<tl::enums::Peer>) -> tl::enums::Message {
    tl::types::MessageEmpty {
        id: 1,
        peer_id: peer,
    }
    .into()
}
fn invite() -> tl::enums::ExportedChatInvite {
    let mut b = 0xa22cbd96u32.to_le_bytes().to_vec();
    b.extend([0; 4]); // flags
    b.extend([0; 4]); // link ""
    b.extend([0; 12]); // admin_id, date
    tl::enums::ExportedChatInvite::from_bytes(&b).unwrap()
}
fn boost() -> tl::enums::Boost {
    let mut b = 0x4b3e14d6u32.to_le_bytes().to_vec();
    b.extend([0; 4]); // flags
    b.extend([0; 4]); // id ""
    b.extend([0; 8]); // date, expires
    tl::enums::Boost::from_bytes(&b).unwrap()
}
fn webpage() -> tl::enums::WebPage {
    tl::types::WebPageEmpty { id: 0, url: None }.into()
}
fn pts(update: &U) -> Option<(Key, i32, i32)> {
    PtsInfo::from_update(update).map(|i| (i.key, i.pts, i.count))
}

#[test]
fn updates_with_a_common_pts() {
    let common = |u: U| assert_eq!(pts(&u), Some((Key::Common, 5, 2)), "{u:?}");
    common(
        tl::types::UpdateNewMessage {
            message: msg(Some(peer_user(1))),
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateNewMessage {
            message: msg(None),
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateDeleteMessages {
            messages: vec![],
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateReadHistoryInbox {
            folder_id: None,
            peer: peer_user(1),
            top_msg_id: None,
            max_id: 0,
            still_unread_count: 0,
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateReadHistoryOutbox {
            peer: peer_user(1),
            max_id: 0,
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateWebPage {
            webpage: webpage(),
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateReadMessagesContents {
            messages: vec![],
            pts: 5,
            pts_count: 2,
            date: None,
        }
        .into(),
    );
    common(
        tl::types::UpdateEditMessage {
            message: msg(None),
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdateFolderPeers {
            folder_peers: vec![],
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
    common(
        tl::types::UpdatePinnedMessages {
            pinned: true,
            peer: peer_user(1),
            messages: vec![],
            pts: 5,
            pts_count: 2,
        }
        .into(),
    );
}

#[test]
fn channel_scoped_updates_are_not_common() {
    let ch = peer_channel(9);
    assert_eq!(
        pts(&tl::types::UpdateNewMessage {
            message: msg(Some(ch.clone())),
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
    assert_eq!(
        pts(&tl::types::UpdateEditMessage {
            message: msg(Some(ch.clone())),
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
    assert_eq!(
        pts(&tl::types::UpdateReadHistoryInbox {
            folder_id: None,
            peer: ch.clone(),
            top_msg_id: None,
            max_id: 0,
            still_unread_count: 0,
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
    assert_eq!(
        pts(&tl::types::UpdateReadHistoryOutbox {
            peer: ch.clone(),
            max_id: 0,
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
    assert_eq!(
        pts(&tl::types::UpdatePinnedMessages {
            pinned: false,
            peer: ch.clone(),
            messages: vec![],
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
}

#[test]
fn updates_with_a_channel_pts() {
    let chan = |u: U, count| assert_eq!(pts(&u), Some((Key::Channel(9), 5, count)), "{u:?}");
    chan(
        tl::types::UpdateChannelTooLong {
            channel_id: 9,
            pts: Some(5),
        }
        .into(),
        0,
    );
    chan(
        tl::types::UpdateNewChannelMessage {
            message: msg(Some(peer_channel(9))),
            pts: 5,
            pts_count: 2,
        }
        .into(),
        2,
    );
    chan(
        tl::types::UpdateEditChannelMessage {
            message: msg(Some(peer_channel(9))),
            pts: 5,
            pts_count: 2,
        }
        .into(),
        2,
    );
    chan(
        tl::types::UpdateReadChannelInbox {
            folder_id: None,
            channel_id: 9,
            max_id: 0,
            still_unread_count: 0,
            pts: 5,
        }
        .into(),
        0,
    );
    chan(
        tl::types::UpdateDeleteChannelMessages {
            channel_id: 9,
            messages: vec![],
            pts: 5,
            pts_count: 2,
        }
        .into(),
        2,
    );
    chan(
        tl::types::UpdateChannelWebPage {
            channel_id: 9,
            webpage: webpage(),
            pts: 5,
            pts_count: 2,
        }
        .into(),
        2,
    );
    chan(
        tl::types::UpdatePinnedChannelMessages {
            pinned: true,
            channel_id: 9,
            messages: vec![],
            pts: 5,
            pts_count: 2,
        }
        .into(),
        2,
    );
    // no pts, or a message without a channel: nothing to track
    assert_eq!(
        pts(&tl::types::UpdateChannelTooLong {
            channel_id: 9,
            pts: None
        }
        .into()),
        None
    );
    assert_eq!(
        pts(&tl::types::UpdateNewChannelMessage {
            message: msg(None),
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
    assert_eq!(
        pts(&tl::types::UpdateEditChannelMessage {
            message: msg(Some(peer_user(1))),
            pts: 5,
            pts_count: 2
        }
        .into()),
        None
    );
}

#[test]
fn updates_with_a_secondary_qts() {
    let sec = |u: U, count| assert_eq!(pts(&u), Some((Key::Secondary, 7, count)), "{u:?}");
    let enc = tl::enums::EncryptedMessage::Message(tl::types::EncryptedMessage {
        random_id: 0,
        chat_id: 0,
        date: 0,
        bytes: vec![],
        file: tl::enums::EncryptedFile::Empty,
    });
    sec(
        tl::types::UpdateNewEncryptedMessage {
            message: enc,
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateChatParticipant {
            chat_id: 1,
            date: 0,
            actor_id: 1,
            user_id: 1,
            prev_participant: None,
            new_participant: None,
            invite: None,
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateChannelParticipant {
            via_chatlist: false,
            channel_id: 1,
            date: 0,
            actor_id: 1,
            user_id: 1,
            prev_participant: None,
            new_participant: None,
            invite: None,
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotStopped {
            user_id: 1,
            date: 0,
            stopped: true,
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotChatInviteRequester {
            peer: peer_user(1),
            date: 0,
            user_id: 1,
            about: String::new(),
            invite: invite(),
            qts: 7,
            query_id: None,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotChatBoost {
            peer: peer_user(1),
            boost: boost(),
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotMessageReaction {
            peer: peer_user(1),
            msg_id: 1,
            date: 0,
            actor: peer_user(1),
            old_reactions: vec![],
            new_reactions: vec![],
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotMessageReactions {
            peer: peer_user(1),
            msg_id: 1,
            date: 0,
            reactions: vec![],
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotNewBusinessMessage {
            connection_id: String::new(),
            message: msg(None),
            reply_to_message: None,
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotEditBusinessMessage {
            connection_id: String::new(),
            message: msg(None),
            reply_to_message: None,
            qts: 7,
        }
        .into(),
        1,
    );
    sec(
        tl::types::UpdateBotDeleteBusinessMessage {
            connection_id: String::new(),
            peer: peer_user(1),
            messages: vec![1, 2, 3],
            qts: 7,
        }
        .into(),
        3,
    );
    sec(
        tl::types::UpdateBotPurchasedPaidMedia {
            user_id: 1,
            payload: String::new(),
            qts: 7,
        }
        .into(),
        1,
    );
}

#[test]
fn updates_without_a_pts_and_the_unset_marker() {
    assert_eq!(pts(&U::Config), None);
    assert_eq!(pts(&U::SavedGifs), None);
    assert_eq!(
        pts(&tl::types::UpdateUserStatus {
            user_id: 1,
            status: tl::enums::UserStatus::Empty
        }
        .into()),
        None
    );
    // a pts equal to the "no pts" marker is filtered out
    assert_eq!(
        pts(&tl::types::UpdateDeleteMessages {
            messages: vec![],
            pts: NO_PTS,
            pts_count: 0
        }
        .into()),
        None
    );
}

fn short_message() -> tl::types::UpdateShortMessage {
    tl::types::UpdateShortMessage {
        out: true,
        mentioned: false,
        media_unread: false,
        silent: true,
        id: 3,
        user_id: 4,
        message: "hi".into(),
        pts: 10,
        pts_count: 1,
        date: 77,
        fwd_from: None,
        via_bot_id: None,
        reply_to: None,
        entities: None,
        ttl_period: None,
    }
}

fn only_update(c: &tl::types::UpdatesCombined) -> &tl::enums::Update {
    assert_eq!(c.updates.len(), 1);
    &c.updates[0]
}

#[test]
fn short_updates_become_combined_updates() {
    let c = adapt(UpdatesLike::Updates(
        tl::enums::Updates::UpdateShortMessage(short_message()),
    ))
    .unwrap();
    assert_eq!((c.date, c.seq, c.seq_start), (77, NO_SEQ, NO_SEQ));
    let U::NewMessage(u) = only_update(&c) else {
        panic!()
    };
    assert_eq!((u.pts, u.pts_count), (10, 1));
    let tl::enums::Message::Message(m) = &u.message else {
        panic!()
    };
    assert_eq!(
        (m.id, m.message.as_str(), m.out, m.silent),
        (3, "hi", true, true)
    );
    assert_eq!(m.peer_id, peer_user(4));
    assert!(m.from_id.is_none());

    let chat = tl::types::UpdateShortChatMessage {
        out: false,
        mentioned: true,
        media_unread: false,
        silent: false,
        id: 3,
        from_id: 4,
        chat_id: 5,
        message: "yo".into(),
        pts: 11,
        pts_count: 1,
        date: 78,
        fwd_from: None,
        via_bot_id: None,
        reply_to: None,
        entities: None,
        ttl_period: None,
    };
    let c = adapt(UpdatesLike::Updates(
        tl::enums::Updates::UpdateShortChatMessage(chat),
    ))
    .unwrap();
    let U::NewMessage(u) = only_update(&c) else {
        panic!()
    };
    let tl::enums::Message::Message(m) = &u.message else {
        panic!()
    };
    assert_eq!(m.from_id, Some(peer_user(4)));
    assert_eq!(m.peer_id, tl::types::PeerChat { chat_id: 5 }.into());
    assert!(m.mentioned);

    let sent = tl::types::UpdateShortSentMessage {
        out: true,
        id: 9,
        pts: 12,
        pts_count: 1,
        date: 79,
        media: None,
        entities: None,
        ttl_period: None,
    };
    let c = adapt(UpdatesLike::Updates(
        tl::enums::Updates::UpdateShortSentMessage(sent),
    ))
    .unwrap();
    let U::NewMessage(u) = only_update(&c) else {
        panic!()
    };
    assert!(matches!(&u.message, tl::enums::Message::Empty(m) if m.id == 9));
}

#[test]
fn other_updates_shapes() {
    use tl::enums::Updates as E;
    assert!(adapt(UpdatesLike::Updates(E::TooLong)).is_err());
    assert!(adapt(UpdatesLike::ConnectionClosed).is_err());
    assert!(adapt(UpdatesLike::MalformedUpdates).is_err());

    let short = tl::types::UpdateShort {
        update: U::Config,
        date: 5,
    };
    let c = adapt(UpdatesLike::Updates(E::UpdateShort(short))).unwrap();
    assert_eq!((c.date, c.seq), (5, NO_SEQ));

    let combined = tl::types::UpdatesCombined {
        updates: vec![U::Config],
        users: vec![],
        chats: vec![],
        date: 1,
        seq_start: 2,
        seq: 3,
    };
    let c = adapt(UpdatesLike::Updates(E::Combined(combined))).unwrap();
    assert_eq!((c.seq_start, c.seq), (2, 3));

    let plain = tl::types::Updates {
        updates: vec![U::Config],
        users: vec![],
        chats: vec![],
        date: 1,
        seq: 4,
    };
    let c = adapt(UpdatesLike::Updates(E::Updates(plain.clone()))).unwrap();
    assert_eq!((c.seq_start, c.seq), (4, 4));

    let invited = tl::types::messages::InvitedUsers {
        updates: E::Updates(plain.clone()),
        missing_invitees: vec![],
    };
    assert_eq!(adapt(UpdatesLike::InvitedUsers(invited)).unwrap().seq, 4);
    let joined = tl::types::messages::ChatInviteJoinResultOk {
        updates: E::Updates(plain),
    };
    assert_eq!(
        adapt(UpdatesLike::ChatInviteJoinResult(joined))
            .unwrap()
            .seq,
        4
    );

    let affected = tl::types::messages::AffectedMessages {
        pts: 20,
        pts_count: 3,
    };
    let c = adapt(UpdatesLike::AffectedMessages(affected.clone())).unwrap();
    assert_eq!(pts(only_update(&c)), Some((Key::Common, 20, 3)));
    let c = adapt(UpdatesLike::AffectedChannelMessages {
        affected,
        channel_id: 9,
        message_ids: vec![1],
    })
    .unwrap();
    assert_eq!(pts(only_update(&c)), Some((Key::Channel(9), 20, 3)));
}

fn send_request() -> tl::functions::messages::SendMessage {
    tl::functions::messages::SendMessage {
        no_webpage: false,
        silent: true,
        background: false,
        clear_draft: false,
        noforwards: true,
        update_stickersets_order: false,
        invert_media: true,
        allow_paid_floodskip: false,
        peer: tl::types::InputPeerChannel {
            channel_id: 9,
            access_hash: 1,
        }
        .into(),
        reply_to: None,
        message: "text".into(),
        random_id: 0,
        reply_markup: None,
        entities: None,
        schedule_date: None,
        schedule_repeat_period: None,
        send_as: None,
        quick_reply_shortcut: None,
        effect: None,
        allow_paid_stars: None,
        suggested_post: None,
        rich_message: None,
    }
}

fn short_sent() -> tl::types::UpdateShortSentMessage {
    tl::types::UpdateShortSentMessage {
        out: true,
        id: 9,
        pts: 12,
        pts_count: 1,
        date: 79,
        media: None,
        entities: None,
        ttl_period: None,
    }
}

fn sent_message(request: tl::functions::messages::SendMessage) -> tl::types::Message {
    let c = adapt(UpdatesLike::ShortSentMessage {
        request,
        update: short_sent(),
    })
    .unwrap();
    let U::NewMessage(u) = only_update(&c) else {
        panic!()
    };
    let tl::enums::Message::Message(m) = &u.message else {
        panic!()
    };
    m.clone()
}

#[test]
fn a_sent_message_is_rebuilt_from_its_request() {
    let m = sent_message(send_request());
    assert_eq!(
        (
            m.id,
            m.message.as_str(),
            m.silent,
            m.noforwards,
            m.invert_media
        ),
        (9, "text", true, true, true)
    );
    assert_eq!(m.peer_id, peer_channel(9));
    assert!(m.from_id.is_none() && m.reply_to.is_none() && m.quick_reply_shortcut_id.is_none());

    let mut req = send_request();
    req.send_as = Some(tl::types::InputPeerChat { chat_id: 2 }.into());
    req.quick_reply_shortcut = Some(tl::types::InputQuickReplyShortcutId { shortcut_id: 6 }.into());
    req.reply_to = Some(
        tl::types::InputReplyToMessage {
            reply_to_msg_id: 5,
            top_msg_id: Some(4),
            reply_to_peer_id: Some(
                tl::types::InputPeerUser {
                    user_id: 8,
                    access_hash: 0,
                }
                .into(),
            ),
            quote_text: Some("q".into()),
            quote_entities: None,
            quote_offset: Some(1),
            monoforum_peer_id: None,
            todo_item_id: None,
            poll_option: None,
            quote_rich: None,
        }
        .into(),
    );
    let m = sent_message(req);
    assert_eq!(m.from_id, Some(tl::types::PeerChat { chat_id: 2 }.into()));
    assert_eq!(m.quick_reply_shortcut_id, Some(6));
    let Some(tl::enums::MessageReplyHeader::Header(h)) = m.reply_to else {
        panic!()
    };
    assert_eq!(
        (h.reply_to_msg_id, h.reply_to_top_id, h.quote),
        (Some(5), Some(4), true)
    );
    assert_eq!(h.reply_to_peer_id, Some(peer_user(8)));

    let mut req = send_request();
    req.quick_reply_shortcut = Some(
        tl::types::InputQuickReplyShortcut {
            shortcut: "x".into(),
        }
        .into(),
    );
    req.reply_to = Some(
        tl::types::InputReplyToStory {
            peer: tl::enums::InputPeer::PeerSelf,
            story_id: 3,
        }
        .into(),
    );
    let m = sent_message(req);
    assert_eq!(m.quick_reply_shortcut_id, None);
    assert!(
        matches!(m.reply_to, Some(tl::enums::MessageReplyHeader::MessageReplyStoryHeader(h)) if h.story_id == 3)
    );

    let mut req = send_request();
    req.reply_to = Some(
        tl::types::InputReplyToMonoForum {
            monoforum_peer_id: tl::enums::InputPeer::Empty,
        }
        .into(),
    );
    assert!(sent_message(req).reply_to.is_none());
}

#[test]
fn input_peers_map_to_peers() {
    use tl::enums::InputPeer as I;
    assert_eq!(peer_from_input_peer(&I::Empty), peer_user(0));
    assert_eq!(peer_from_input_peer(&I::PeerSelf), peer_user(0));
    assert_eq!(
        peer_from_input_peer(&tl::types::InputPeerChat { chat_id: 2 }.into()),
        tl::types::PeerChat { chat_id: 2 }.into()
    );
    assert_eq!(
        peer_from_input_peer(
            &tl::types::InputPeerUser {
                user_id: 3,
                access_hash: 0
            }
            .into()
        ),
        peer_user(3)
    );
    assert_eq!(
        peer_from_input_peer(
            &tl::types::InputPeerChannel {
                channel_id: 4,
                access_hash: 0
            }
            .into()
        ),
        peer_channel(4)
    );
    let u: I = tl::types::InputPeerUserFromMessage {
        peer: I::Empty,
        msg_id: 1,
        user_id: 5,
    }
    .into();
    assert_eq!(peer_from_input_peer(&u), peer_user(5));
    let c: I = tl::types::InputPeerChannelFromMessage {
        peer: I::Empty,
        msg_id: 1,
        channel_id: 6,
    }
    .into();
    assert_eq!(peer_from_input_peer(&c), peer_channel(6));
}

fn service(peer: tl::enums::Peer) -> tl::enums::Message {
    use tl::Serializable;
    let mut b = 0x7a800e0au32.to_le_bytes().to_vec();
    b.extend(0u32.to_le_bytes()); // flags
    b.extend(1i32.to_le_bytes()); // id
    b.extend(peer.to_bytes());
    b.extend(0i32.to_le_bytes()); // date
    b.extend(0xb6aef7b0u32.to_le_bytes()); // messageActionEmpty
    tl::enums::Message::from_bytes(&b).unwrap()
}

#[test]
fn message_peer_and_channel_id() {
    assert_eq!(message_peer(&msg(Some(peer_user(1)))), Some(peer_user(1)));
    assert_eq!(message_peer(&msg(None)), None);
    assert_eq!(message_peer(&service(peer_user(1))), Some(peer_user(1)));
    assert_eq!(message_channel_id(&msg(Some(peer_channel(9)))), Some(9));
    assert_eq!(message_channel_id(&msg(Some(peer_user(9)))), None);
    assert_eq!(message_channel_id(&msg(None)), None);
    assert_eq!(message_channel_id(&service(peer_channel(9))), Some(9));
    assert_eq!(message_channel_id(&service(peer_user(9))), None);
    let m: tl::enums::Message = {
        let mut m = sent_message(send_request());
        m.peer_id = peer_channel(7);
        m.into()
    };
    assert_eq!(message_channel_id(&m), Some(7));
    let mut m = sent_message(send_request());
    m.peer_id = peer_user(7);
    assert_eq!(message_channel_id(&m.clone().into()), None);
    assert_eq!(message_peer(&m.into()), Some(peer_user(7)));
}

#[test]
fn channel_differences_are_adapted() {
    use tl::enums::updates::ChannelDifference as D;
    let empty = D::Empty(tl::types::updates::ChannelDifferenceEmpty {
        r#final: true,
        pts: 5,
        timeout: Some(3),
    });
    let d = adapt_channel_difference(empty).unwrap();
    assert_eq!((d.r#final, d.pts, d.timeout), (true, 5, Some(3)));
    assert!(d.new_messages.is_empty());

    let full = tl::types::updates::ChannelDifference {
        r#final: false,
        pts: 6,
        timeout: None,
        new_messages: vec![msg(None)],
        other_updates: vec![],
        chats: vec![],
        users: vec![],
    };
    assert_eq!(
        adapt_channel_difference(D::Difference(full))
            .unwrap()
            .new_messages
            .len(),
        1
    );
}
