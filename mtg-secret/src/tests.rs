//! Two peers talking to each other in-process.

use mtg_tl_types as tl;

use crate::chat::{Event, Outgoing, Output, SecretChat};
use crate::chats::{ChatsEvent, SecretChats};
use crate::dh::tests::{p14, params};
use crate::schema::{Action, DecryptedMessage, LAYER, Layer, Media, Message, SendAction, Service};
use crate::{Error, Key, cipher};

const ID: i32 = 5;

fn key() -> Key {
    Key::new([9; 256])
}

fn pair() -> (SecretChat, SecretChat) {
    let a = SecretChat::new(ID, 11, 2, true, key(), params(), 100);
    let b = SecretChat::new(ID, 11, 1, false, key(), params(), 100);
    (a, b)
}

fn wire(o: &Outgoing) -> tl::enums::EncryptedMessage {
    match o {
        Outgoing::Message(r) => encrypted(r.random_id, &r.data, true),
        Outgoing::File(r) => encrypted(r.random_id, &r.data, true),
        Outgoing::Service(r) => encrypted(r.random_id, &r.data, false),
    }
}

fn encrypted(random_id: i64, bytes: &[u8], message: bool) -> tl::enums::EncryptedMessage {
    if message {
        tl::enums::EncryptedMessage::Message(tl::types::EncryptedMessage {
            random_id,
            chat_id: ID,
            date: 1000,
            bytes: bytes.to_vec(),
            file: tl::enums::EncryptedFile::Empty,
        })
    } else {
        tl::enums::EncryptedMessage::Service(tl::types::EncryptedMessageService {
            random_id,
            chat_id: ID,
            date: 1000,
            bytes: bytes.to_vec(),
        })
    }
}

fn text(s: &str) -> Message {
    Message {
        message: s.into(),
        ..Message::default()
    }
}

fn texts(o: &Output) -> Vec<String> {
    o.events
        .iter()
        .filter_map(|e| match e {
            Event::Message { message, .. } => Some(message.message.clone()),
            _ => None,
        })
        .collect()
}

fn deliver(to: &mut SecretChat, o: &Outgoing) -> Output {
    to.receive(1, &wire(o)).unwrap()
}

/// Delivers `to_a` to `a` and `to_b` to `b`, and what comes back, until both are quiet.
fn settle(
    a: &mut SecretChat,
    b: &mut SecretChat,
    mut to_a: Vec<Outgoing>,
    mut to_b: Vec<Outgoing>,
) {
    while !to_a.is_empty() || !to_b.is_empty() {
        let mut next_a = Vec::new();
        for o in to_b.drain(..) {
            next_a.extend(deliver(b, &o).send);
        }
        let mut next_b = Vec::new();
        for o in to_a.drain(..) {
            next_b.extend(deliver(a, &o).send);
        }
        to_a = next_a;
        to_b = next_b;
    }
}

/// An envelope with chosen sequence numbers, encrypted as chat `from_originator` would.
fn forged(
    from_originator: bool,
    out_seq_no: i32,
    in_seq_no: i32,
    action: Action,
) -> tl::enums::EncryptedMessage {
    let layer = Layer {
        random_bytes: vec![1; 16],
        layer: LAYER,
        in_seq_no,
        out_seq_no,
        message: DecryptedMessage::Service(Service {
            random_id: 77,
            action,
        }),
    };
    let data = cipher::encrypt(&key(), from_originator, &layer.encode().unwrap());
    encrypted(77, &data, false)
}

fn protocol(r: Result<Output, Error>) -> &'static str {
    match r {
        Err(Error::Protocol(why)) => why,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

#[test]
fn messages_go_both_ways() {
    let (mut a, mut b) = pair();
    let mut m = text("hi");
    m.media = Some(Media::GeoPoint {
        lat: 1.5,
        long: -2.5,
    });
    m.reply_to_random_id = Some(4);
    let o = deliver(&mut b, &a.send_message(m.clone()).unwrap());
    match &o.events[..] {
        [Event::Message { message, date, .. }] => {
            assert_eq!(message.message, "hi");
            assert_eq!(message.media, m.media);
            assert_eq!(message.reply_to_random_id, Some(4));
            assert_ne!(message.random_id, 0);
            assert_eq!(*date, 1000);
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(
        texts(&deliver(&mut a, &b.send_message(text("yo")).unwrap())),
        ["yo"]
    );
    assert_eq!(
        texts(&deliver(&mut b, &a.send_message(text("again")).unwrap())),
        ["again"]
    );
}

#[test]
fn a_file_message_carries_the_file() {
    let (mut a, mut b) = pair();
    let input = tl::enums::InputEncryptedFile::Empty;
    let o = a.send_file(text("pic"), input.clone()).unwrap();
    assert!(matches!(&o, Outgoing::File(r) if r.file == input));
    assert_eq!(texts(&deliver(&mut b, &o)), ["pic"]);
}

#[test]
fn duplicates_are_dropped() {
    let (mut a, mut b) = pair();
    let o = a.send_message(text("once")).unwrap();
    assert_eq!(texts(&deliver(&mut b, &o)).len(), 1);
    assert_eq!(deliver(&mut b, &o), Output::default());
}

#[test]
fn a_gap_is_held_and_resent_once() {
    let (mut a, mut b) = pair();
    let m: Vec<_> = (0..4)
        .map(|i| a.send_message(text(&format!("m{i}"))).unwrap())
        .collect();
    assert_eq!(texts(&deliver(&mut b, &m[0])), ["m0"]);
    // m1 is lost: m2 is held and asks for a resend of exactly m1
    let o = deliver(&mut b, &m[2]);
    assert!(o.events.is_empty());
    let [req] = &o.send[..] else { panic!("{o:?}") };
    // a second held message does not ask again
    let o3 = deliver(&mut b, &m[3]);
    assert!(o3.events.is_empty() && o3.send.is_empty());
    // the same held message again is ignored
    assert_eq!(deliver(&mut b, &m[3]), Output::default());
    // a handles the request by resending m1 only
    let o = deliver(&mut a, req);
    let [resent] = &o.send[..] else {
        panic!("{o:?}")
    };
    let o = deliver(&mut b, resent);
    assert_eq!(texts(&o), ["m1", "m2", "m3"]);
    // a held duplicate is ignored too
    assert_eq!(deliver(&mut b, &m[3]), Output::default());
}

#[test]
fn a_resend_arriving_out_of_order_is_executed_once() {
    let (mut a, mut b) = pair();
    let b0 = b.send_message(text("b0")).unwrap(); // b's wire number 0, lost on the way to a
    let m0 = a.send_message(text("m0")).unwrap(); // lost on the way to b
    let ask = a
        .send_action(Action::Resend {
            start_seq_no: 0,
            end_seq_no: 0,
        })
        .unwrap();
    // the request overtakes m0: b answers it at once (b0 again) and asks for m0
    let o = deliver(&mut b, &ask);
    assert!(o.events.is_empty());
    assert_eq!(o.send.len(), 2);
    assert_eq!(texts(&deliver(&mut a, &o.send[0])), ["b0"]);
    let _ = b0;
    // m0 arrives: it is applied, the held request is not executed a second time
    let o = deliver(&mut b, &m0);
    assert_eq!(texts(&o), ["m0"]);
    assert!(o.send.is_empty());
}

#[test]
fn parity_and_range_violations_end_the_chat() {
    let (_, mut b) = pair();
    // from the originator: out must be odd... b expects x_in = 1
    let wrong = forged(true, 0, 0, Action::Noop);
    assert!(protocol(b.receive(1, &wrong)).contains("parity"));
    let wrong = forged(true, 1, 1, Action::Noop); // in_seq_no must have x_out = 0 parity for b
    assert!(protocol(b.receive(1, &wrong)).contains("parity"));
    let neg = forged(true, -1, 0, Action::Noop);
    assert!(protocol(b.receive(1, &neg)).contains("parity"));
    // b never sent anything, so the peer cannot have received anything of b's
    let range = forged(true, 1, 2, Action::Noop);
    assert!(protocol(b.receive(1, &range)).contains("in_seq_no"));
}

#[test]
fn in_seq_no_may_not_go_back() {
    let (mut a, mut b) = pair();
    let m1 = b.send_message(text("1")).unwrap();
    let m2 = b.send_message(text("2")).unwrap();
    // a answers after both: in_seq_no = 2
    deliver(&mut a, &m1);
    deliver(&mut a, &m2);
    let late = a.send_message(text("late")).unwrap();
    deliver(&mut b, &late);
    // an envelope claiming that only 1 was received comes after
    let back = forged(true, 3, 2, Action::Noop);
    // x_in(b)=1: out 3 = raw 1 (a's second message); in 2 = raw 1 < peer_in 2 -> error
    assert!(protocol(b.receive(1, &back)).contains("in_seq_no"));
}

#[test]
fn too_many_held_messages_end_the_chat() {
    let (mut a, mut b) = pair();
    let m: Vec<_> = (0..7)
        .map(|i| a.send_message(text(&format!("{i}"))).unwrap())
        .collect();
    for o in &m[1..5] {
        deliver(&mut b, o);
    }
    assert!(protocol(b.receive(1, &wire(&m[5]))).contains("too many"));
}

#[test]
fn resend_beyond_the_log_or_out_of_shape_ends_the_chat() {
    let (mut a, mut b) = pair();
    // b asks a for wire 3..=3 which a never sent (a's wire numbers are odd: 1, 3, ...)
    let ask = b
        .send_action(Action::Resend {
            start_seq_no: 3,
            end_seq_no: 3,
        })
        .unwrap();
    assert!(protocol(a.receive(1, &wire(&ask))).contains("no longer"));
    let (mut a, mut b) = pair();
    for (s, e) in [(5, 1), (0, 0), (-1, -1)] {
        let ask = b
            .send_action(Action::Resend {
                start_seq_no: s,
                end_seq_no: e,
            })
            .unwrap();
        assert!(
            protocol(a.receive(1, &wire(&ask))).contains("range"),
            "{s} {e}"
        );
    }
}

#[test]
fn the_resend_log_is_bounded() {
    let (mut a, mut b) = pair();
    let first = a.send_message(text("0")).unwrap();
    for _ in 0..300 {
        a.send_message(text("x")).unwrap();
    }
    let _ = first;
    // b never acknowledged: asking for the very first message fails now
    let ask = b
        .send_action(Action::Resend {
            start_seq_no: 1,
            end_seq_no: 1,
        })
        .unwrap();
    assert!(protocol(a.receive(1, &wire(&ask))).contains("no longer"));
}

#[test]
fn acknowledged_messages_leave_the_log() {
    let (mut a, mut b) = pair();
    let m = a.send_message(text("x")).unwrap();
    deliver(&mut b, &m);
    let reply = b.send_message(text("y")).unwrap();
    deliver(&mut a, &reply); // carries in_seq_no = 1: the log of a is trimmed
    let ask = b
        .send_action(Action::Resend {
            start_seq_no: 1,
            end_seq_no: 1,
        })
        .unwrap();
    assert!(protocol(a.receive(1, &wire(&ask))).contains("no longer"));
}

#[test]
fn layers_are_negotiated() {
    let (mut a, mut b) = pair();
    assert_eq!(a.send_layer(), 73);
    let o = deliver(&mut b, &a.notify_layer().unwrap());
    assert_eq!(o.events, [Event::LayerChanged(LAYER)]);
    assert_eq!(b.send_layer(), LAYER);
    // an envelope of a newer layer than known raises it too
    let m = deliver(&mut a, &b.send_message(text("x")).unwrap());
    assert!(m.events.contains(&Event::LayerChanged(LAYER)));
    assert_eq!(a.send_layer(), LAYER);
    // never above ours
    let ahead = forged(true, 1, 0, Action::NotifyLayer { layer: 999 });
    let mut c = SecretChat::new(ID, 11, 1, false, key(), params(), 0);
    c.receive(1, &ahead).unwrap();
    assert_eq!(c.send_layer(), LAYER);
}

#[test]
fn layers_below_46_are_refused() {
    let layer = Layer {
        random_bytes: vec![1; 16],
        layer: 17,
        in_seq_no: 0,
        out_seq_no: 1,
        message: DecryptedMessage::Service(Service {
            random_id: 1,
            action: Action::Noop,
        }),
    };
    assert!(matches!(layer.encode(), Err(Error::UnsupportedLayer(17))));
    let mut ok = layer.clone();
    ok.layer = 46;
    let data = cipher::encrypt(&key(), true, &ok.encode().unwrap());
    let (_, mut b) = pair();
    assert!(b.receive(1, &encrypted(1, &data, false)).is_ok());
    // the same with the layer byte patched down
    let mut raw = ok.encode().unwrap();
    let pos = raw
        .windows(4)
        .position(|w| w == 46i32.to_le_bytes())
        .unwrap();
    raw[pos..pos + 4].copy_from_slice(&17i32.to_le_bytes());
    let data = cipher::encrypt(&key(), true, &raw);
    let (_, mut b) = pair();
    assert!(matches!(
        b.receive(1, &encrypted(1, &data, false)),
        Err(Error::UnsupportedLayer(17))
    ));
}

#[test]
fn bad_envelopes_are_errors_not_chat_killers() {
    let (mut a, mut b) = pair();
    let o = a.send_message(text("x")).unwrap();
    let Outgoing::Message(mut r) = o else {
        unreachable!()
    };
    // an unknown key
    let mut data = r.data.clone();
    data[0] ^= 1;
    assert!(matches!(
        b.receive(1, &encrypted(1, &data, true)),
        Err(Error::UnknownKey)
    ));
    // a flipped payload byte
    let last = r.data.len() - 1;
    r.data[last] ^= 1;
    assert!(matches!(
        b.receive(1, &encrypted(1, &r.data, true)),
        Err(Error::BadMsgKey)
    ));
    assert!(matches!(
        b.receive(1, &encrypted(1, &[1, 2], true)),
        Err(Error::TooShort)
    ));
    // not a layer
    let data = cipher::encrypt(&key(), true, &[0, 0, 0, 0]);
    assert!(matches!(
        b.receive(1, &encrypted(1, &data, true)),
        Err(Error::Deserialize(_))
    ));
    // short random_bytes
    let layer = Layer {
        random_bytes: vec![1; 16],
        layer: LAYER,
        in_seq_no: 0,
        out_seq_no: 1,
        message: DecryptedMessage::Service(Service {
            random_id: 1,
            action: Action::Noop,
        }),
    };
    let mut raw = layer.encode().unwrap();
    raw[4] = 3; // length byte of random_bytes: 3 < 15 (the padding is garbage, never read)
    let data = cipher::encrypt(&key(), true, &raw);
    assert!(matches!(
        b.receive(1, &encrypted(1, &data, true)),
        Err(Error::BadRandomBytes)
    ));
}

#[test]
fn service_actions_reach_the_application() {
    let (mut a, mut b) = pair();
    let actions = [
        Action::SetMessageTtl { ttl_seconds: 30 },
        Action::ReadMessages {
            random_ids: vec![1, 2],
        },
        Action::DeleteMessages {
            random_ids: vec![3],
        },
        Action::ScreenshotMessages {
            random_ids: vec![4],
        },
        Action::FlushHistory,
        Action::Typing {
            action: SendAction::RecordAudio,
        },
    ];
    for action in actions {
        let o = deliver(&mut b, &a.send_action(action.clone()).unwrap());
        assert_eq!(o.events, [Event::Action(action)]);
    }
    // a noop and a layer repeat are swallowed
    assert!(
        deliver(&mut b, &a.send_action(Action::Noop).unwrap())
            .events
            .is_empty()
    );
}

#[test]
fn builders_name_the_chat() {
    let (a, _) = pair();
    let peer = tl::enums::InputEncryptedChat::Chat(tl::types::InputEncryptedChat {
        chat_id: ID,
        access_hash: 11,
    });
    assert_eq!(a.typing(true).peer, peer);
    assert!(a.typing(true).typing);
    assert_eq!(a.read_history(42).max_date, 42);
    assert_eq!(a.read_history(42).peer, peer);
    let d = a.discard(true);
    assert!(d.delete_history);
    assert_eq!(d.chat_id, ID);
    assert_eq!((a.id(), a.peer_id(), a.originator()), (ID, 2, true));
    assert_eq!(a.key().fingerprint(), key().fingerprint());
}

#[test]
fn state_survives_a_restart() {
    let (mut a, mut b) = pair();
    deliver(&mut b, &a.send_message(text("1")).unwrap());
    deliver(&mut a, &b.send_message(text("2")).unwrap());
    let mut a2 = SecretChat::restore(a.state(), params());
    assert_eq!(a2.state(), a.state());
    assert_eq!(
        texts(&deliver(&mut b, &a2.send_message(text("3")).unwrap())),
        ["3"]
    );
    assert_eq!(
        texts(&deliver(&mut a2, &b.send_message(text("4")).unwrap())),
        ["4"]
    );
}

#[test]
fn a_rekey_is_due_after_100_messages_or_a_week() {
    let (mut a, _) = pair();
    assert!(!a.needs_rekey(100 + 8 * 24 * 3600)); // unused keys are not replaced
    a.send_message(text("x")).unwrap();
    assert!(!a.needs_rekey(100 + 7 * 24 * 3600));
    assert!(a.needs_rekey(101 + 7 * 24 * 3600));
    let (mut a, _) = pair();
    for _ in 0..100 {
        a.send_message(text("x")).unwrap();
    }
    assert!(!a.needs_rekey(100));
    a.send_message(text("x")).unwrap();
    assert!(a.needs_rekey(100));
    a.request_key().unwrap();
    assert!(!a.needs_rekey(100)); // already running
    assert!(matches!(a.request_key(), Err(Error::WrongState(_))));
}

#[test]
fn a_rekey_switches_both_sides() {
    let (mut a, mut b) = pair();
    let old = a.key().fingerprint();
    let request = a.request_key().unwrap();
    let o = deliver(&mut b, &request);
    let [accept] = &o.send[..] else {
        panic!("{o:?}")
    };
    let o = deliver(&mut a, accept);
    let [commit] = &o.send[..] else {
        panic!("{o:?}")
    };
    let new = a.key().fingerprint();
    assert_ne!(new, old);
    assert_eq!(o.events, [Event::KeyChanged { fingerprint: new }]);
    // a already talks under the new key; b reads it (future key) but holds it until the commit
    let early = a.send_message(text("new key")).unwrap();
    let held = deliver(&mut b, &early);
    assert!(held.events.is_empty());
    assert_eq!(b.key().fingerprint(), old);
    let o = deliver(&mut b, commit);
    assert_eq!(o.events[0], Event::KeyChanged { fingerprint: new });
    assert_eq!(texts(&o), ["new key"]);
    assert_eq!(b.key().bytes(), a.key().bytes());
    // b's resend request is answered with a commit b has already seen
    settle(&mut a, &mut b, held.send, vec![]);
    let back = b.send_message(text("reply")).unwrap();
    assert_eq!(texts(&deliver(&mut a, &back)), ["reply"]);
    let after = deliver(&mut b, &a.send_message(text("later")).unwrap());
    assert_eq!(texts(&after), ["later"]);
}

#[test]
fn the_old_key_is_dropped_after_the_new_one_is_used() {
    let (mut a, mut b) = pair();
    let old_msg = {
        let mut copy = SecretChat::restore(a.state(), params());
        copy.send_message(text("under the old key")).unwrap()
    };
    let request = a.request_key().unwrap();
    let accept = deliver(&mut b, &request).send;
    let commit = deliver(&mut a, &accept[0]).send;
    deliver(&mut b, &commit[0]);
    // a message under the new key arrives: the previous key goes
    deliver(&mut b, &a.send_message(text("new")).unwrap());
    let Outgoing::Message(r) = &old_msg else {
        unreachable!()
    };
    assert!(matches!(
        b.receive(1, &encrypted(r.random_id, &r.data, true)),
        Err(Error::UnknownKey)
    ));
}

#[test]
fn concurrent_rekeys_end_with_one_new_key() {
    for _ in 0..8 {
        let (mut a, mut b) = pair();
        let old = a.key().fingerprint();
        let ra = a.request_key().unwrap();
        let rb = b.request_key().unwrap();
        let to_b = deliver(&mut a, &rb).send;
        let to_a = deliver(&mut b, &ra).send;
        settle(&mut a, &mut b, to_a, to_b);
        assert_eq!(a.key().bytes(), b.key().bytes());
        assert_ne!(a.key().fingerprint(), old);
        // and the chat still works
        assert_eq!(
            texts(&deliver(&mut b, &a.send_message(text("ok")).unwrap())),
            ["ok"]
        );
    }
}

#[test]
fn rekey_aborts() {
    // we abort before the peer's accept arrives
    let (mut a, mut b) = pair();
    let request = a.request_key().unwrap();
    let accept = deliver(&mut b, &request).send;
    let abort = a.abort_rekey().unwrap().unwrap();
    assert!(a.abort_rekey().unwrap().is_none());
    let o = deliver(&mut b, &abort);
    assert!(matches!(&o.events[..], [Event::RekeyAborted { .. }]));
    // the late accept is ignored
    assert_eq!(deliver(&mut a, &accept[0]), Output::default());
    // an abort for another exchange, or with nothing running, does nothing
    let stray = b.send_action(Action::AbortKey { exchange_id: 1 }).unwrap();
    assert_eq!(deliver(&mut a, &stray), Output::default());
    a.request_key().unwrap();
    let stray = b.send_action(Action::AbortKey { exchange_id: 1 }).unwrap();
    assert_eq!(deliver(&mut a, &stray), Output::default());
}

#[test]
fn a_bad_accept_aborts_the_exchange() {
    let (mut a, mut b) = pair();
    let request = a.request_key().unwrap();
    let Action::RequestKey { exchange_id, .. } = last_action(&mut b, &request) else {
        panic!()
    };
    // wrong fingerprint
    let bogus = b
        .send_action(Action::AcceptKey {
            exchange_id,
            g_b: vec![2],
            key_fingerprint: 5,
        })
        .unwrap();
    let o = deliver(&mut a, &bogus);
    assert!(matches!(&o.events[..], [Event::RekeyAborted { .. }]));
    assert!(matches!(&o.send[..], [Outgoing::Service(_)]));
    // an accept for another id is ignored
    let (mut a, mut b) = pair();
    a.request_key().unwrap();
    let other = b
        .send_action(Action::AcceptKey {
            exchange_id: 1,
            g_b: vec![2],
            key_fingerprint: 5,
        })
        .unwrap();
    assert_eq!(deliver(&mut a, &other), Output::default());
}

fn last_action(to: &mut SecretChat, o: &Outgoing) -> Action {
    // peek into the request by decrypting it as `to` would
    let Outgoing::Service(r) = o else { panic!() };
    let from_originator = !to.originator();
    let plain = cipher::decrypt(to.key(), from_originator, &r.data).unwrap();
    match Layer::decode(&plain).unwrap().message {
        DecryptedMessage::Service(s) => s.action,
        _ => panic!(),
    }
}

#[test]
fn a_bad_request_is_answered_with_an_abort() {
    let (mut a, mut b) = pair();
    let bad = a
        .send_action(Action::RequestKey {
            exchange_id: 3,
            g_a: vec![1],
        })
        .unwrap();
    let o = deliver(&mut b, &bad);
    let [reply] = &o.send[..] else {
        panic!("{o:?}")
    };
    assert!(matches!(
        last_action(&mut a, reply),
        Action::AbortKey { exchange_id: 3 }
    ));
    // a second request while one is accepted is refused
    let good = a.request_key().unwrap();
    deliver(&mut b, &good);
    let again = a
        .send_action(Action::RequestKey {
            exchange_id: 4,
            g_a: vec![1],
        })
        .unwrap();
    let o = deliver(&mut b, &again);
    assert!(matches!(
        last_action(&mut a, &o.send[0]),
        Action::AbortKey { exchange_id: 4 }
    ));
}

#[test]
fn a_commit_that_does_not_match_changes_nothing() {
    let (mut a, mut b) = pair();
    let request = a.request_key().unwrap();
    deliver(&mut b, &request);
    let wrong = a
        .send_action(Action::CommitKey {
            exchange_id: 1,
            key_fingerprint: 1,
        })
        .unwrap();
    assert_eq!(deliver(&mut b, &wrong), Output::default());
    assert_eq!(b.key().fingerprint(), key().fingerprint());
}

// ---- the manager

fn config() -> tl::enums::messages::DhConfig {
    tl::enums::messages::DhConfig::Config(tl::types::messages::DhConfig {
        g: 2,
        p: p14(),
        version: 1,
        random: vec![7; 256],
    })
}

fn chat_update(chat: tl::enums::EncryptedChat) -> tl::enums::Update {
    tl::enums::Update::Encryption(tl::types::UpdateEncryption { chat, date: 1 })
}

fn message_update(o: &Outgoing, qts: i32) -> tl::enums::Update {
    tl::enums::Update::NewEncryptedMessage(tl::types::UpdateNewEncryptedMessage {
        message: wire(o),
        qts,
    })
}

/// Alice asks, Bob accepts: returns both managers with the chat ready.
fn handshake() -> (SecretChats, SecretChats) {
    let (mut alice, mut bob) = (SecretChats::new(), SecretChats::new());
    assert_eq!(alice.dh_config_request().version, 0);
    alice.set_dh_config(&config()).unwrap();
    bob.set_dh_config(&config()).unwrap();
    assert_eq!(alice.dh_config_request().version, 1);
    let req = alice.request(tl::enums::InputUser::UserSelf, 2).unwrap();
    let o = bob.handle_update(
        10,
        &chat_update(tl::enums::EncryptedChat::Requested(
            tl::types::EncryptedChatRequested {
                folder_id: None,
                id: ID,
                access_hash: 11,
                date: 1,
                admin_id: 1,
                participant_id: 2,
                g_a: req.g_a.clone(),
            },
        )),
    );
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Requested {
            chat_id: ID,
            peer_id: 1
        }]
    ));
    let accept = bob.accept(ID, 10).unwrap();
    let waiting = tl::enums::EncryptedChat::Waiting(tl::types::EncryptedChatWaiting {
        id: ID,
        access_hash: 11,
        date: 1,
        admin_id: 1,
        participant_id: 2,
    });
    assert_eq!(
        alice.on_request_result(req.random_id, &waiting).unwrap(),
        ID
    );
    let chat = tl::enums::EncryptedChat::Chat(tl::types::EncryptedChat {
        id: ID,
        access_hash: 11,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        g_a_or_b: accept.g_b,
        key_fingerprint: accept.key_fingerprint,
    });
    let o = alice.handle_update(10, &chat_update(chat.clone()));
    assert!(matches!(&o.events[..], [ChatsEvent::Ready { chat_id: ID }]));
    // the confirmation for bob changes nothing
    assert!(bob.handle_update(10, &chat_update(chat)).events.is_empty());
    (alice, bob)
}

#[test]
fn a_chat_is_created_and_used_through_the_managers() {
    let (mut alice, mut bob) = handshake();
    assert_eq!(
        alice.chat(ID).unwrap().key().bytes(),
        bob.chat(ID).unwrap().key().bytes()
    );
    assert!(alice.chat(ID).unwrap().originator());
    let hello = alice
        .chat_mut(ID)
        .unwrap()
        .send_message(text("hello"))
        .unwrap();
    let o = bob.handle_update(11, &message_update(&hello, 7));
    assert_eq!(o.qts, Some(7));
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Chat {
            chat_id: ID,
            event: Event::Message { .. }
        }]
    ));
    assert_eq!(SecretChats::received_queue(7).max_qts, 7);
    // layers: each tells the other
    let n = bob.chat_mut(ID).unwrap().notify_layer().unwrap();
    let o = alice.handle_update(12, &message_update(&n, 8));
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Chat {
            event: Event::LayerChanged(_),
            ..
        }]
    ));
    // persistence
    let states = alice.states();
    let again = SecretChats::restore(params(), states.clone());
    assert_eq!(again.states(), states);
    assert_eq!(again.dh_config_request().version, params().version());
}

#[test]
fn other_updates_and_unknown_chats_are_ignored() {
    let (mut alice, mut bob) = handshake();
    let nothing = tl::enums::Update::Config;
    let o = alice.handle_update(0, &nothing);
    assert!(o.events.is_empty() && o.qts.is_none());
    let m = bob.chat_mut(ID).unwrap().send_message(text("x")).unwrap();
    let mut other = SecretChats::new();
    assert!(
        other
            .handle_update(0, &message_update(&m, 1))
            .events
            .is_empty()
    );
    assert!(other.chat(ID).is_none() && other.chat_mut(ID).is_none());
    assert!(
        alice
            .handle_update(
                0,
                &chat_update(tl::enums::EncryptedChat::Empty(
                    tl::types::EncryptedChatEmpty { id: 99 }
                ))
            )
            .events
            .is_empty()
    );
}

#[test]
fn typing_read_and_discard_updates() {
    let (mut alice, _) = handshake();
    let o = alice.handle_update(
        0,
        &tl::enums::Update::EncryptedChatTyping(tl::types::UpdateEncryptedChatTyping {
            chat_id: ID,
        }),
    );
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Typing { chat_id: ID }]
    ));
    let o = alice.handle_update(
        0,
        &tl::enums::Update::EncryptedMessagesRead(tl::types::UpdateEncryptedMessagesRead {
            chat_id: ID,
            max_date: 5,
            date: 6,
        }),
    );
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Read {
            chat_id: ID,
            max_date: 5,
            date: 6
        }]
    ));
    let gone = tl::enums::EncryptedChat::Discarded(tl::types::EncryptedChatDiscarded {
        history_deleted: true,
        id: ID,
    });
    let o = alice.handle_update(0, &chat_update(gone));
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Discarded {
            chat_id: ID,
            history_deleted: true
        }]
    ));
    assert!(alice.chat(ID).is_none());
}

#[test]
fn a_protocol_violation_discards_the_chat_and_a_bad_message_only_drops() {
    let (mut alice, mut bob) = handshake();
    // parity violation: wire out 0 from the originator
    let bad = forged_for(alice.chat(ID).unwrap(), true, 0, 0);
    let o = bob.handle_update(
        0,
        &tl::enums::Update::NewEncryptedMessage(tl::types::UpdateNewEncryptedMessage {
            message: bad,
            qts: 3,
        }),
    );
    assert_eq!(o.discard.len(), 1);
    assert!(matches!(
        &o.events[..],
        [
            ChatsEvent::Dropped { .. },
            ChatsEvent::Discarded { chat_id: ID, .. }
        ]
    ));
    assert!(bob.chat(ID).is_none());
    // garbage is only dropped
    let junk = tl::enums::Update::NewEncryptedMessage(tl::types::UpdateNewEncryptedMessage {
        message: encrypted(1, &[1, 2, 3], true),
        qts: 4,
    });
    let o = alice.handle_update(0, &junk);
    assert!(matches!(&o.events[..], [ChatsEvent::Dropped { .. }]));
    assert!(alice.chat(ID).is_some());
}

fn forged_for(
    chat: &SecretChat,
    from_originator: bool,
    out: i32,
    inn: i32,
) -> tl::enums::EncryptedMessage {
    let layer = Layer {
        random_bytes: vec![1; 16],
        layer: LAYER,
        in_seq_no: inn,
        out_seq_no: out,
        message: DecryptedMessage::Service(Service {
            random_id: 1,
            action: Action::Noop,
        }),
    };
    let data = cipher::encrypt(chat.key(), from_originator, &layer.encode().unwrap());
    encrypted(1, &data, false)
}

#[test]
fn a_gap_in_the_manager_asks_for_a_resend() {
    let (mut alice, mut bob) = handshake();
    let _lost = alice.chat_mut(ID).unwrap().send_message(text("0")).unwrap();
    let second = alice.chat_mut(ID).unwrap().send_message(text("1")).unwrap();
    let o = bob.handle_update(0, &message_update(&second, 1));
    assert_eq!(o.send.len(), 1);
    assert!(o.events.is_empty());
}

#[test]
fn manager_errors() {
    let mut chats = SecretChats::new();
    assert!(matches!(
        chats.request(tl::enums::InputUser::UserSelf, 1),
        Err(Error::WrongState(_))
    ));
    assert!(matches!(chats.accept(1, 0), Err(Error::WrongState(_))));
    // a config without p/g we can use
    let bad = tl::enums::messages::DhConfig::Config(tl::types::messages::DhConfig {
        g: 2,
        p: vec![1, 2, 3],
        version: 1,
        random: vec![],
    });
    assert!(matches!(chats.set_dh_config(&bad), Err(Error::BadDhParams)));
    let unchanged =
        tl::enums::messages::DhConfig::NotModified(tl::types::messages::DhConfigNotModified {
            random: vec![1],
        });
    assert!(matches!(
        chats.set_dh_config(&unchanged),
        Err(Error::WrongState(_))
    ));
    chats.set_dh_config(&config()).unwrap();
    chats.set_dh_config(&unchanged).unwrap();
    assert!(matches!(chats.accept(1, 0), Err(Error::NoSuchChat(1))));
    let req = chats.request(tl::enums::InputUser::UserSelf, 1).unwrap();
    // an answer that is not "waiting", and one for an unknown request
    let empty = tl::enums::EncryptedChat::Empty(tl::types::EncryptedChatEmpty { id: 1 });
    assert!(chats.on_request_result(req.random_id, &empty).is_err());
    let waiting = tl::enums::EncryptedChat::Waiting(tl::types::EncryptedChatWaiting {
        id: 1,
        access_hash: 1,
        date: 1,
        admin_id: 1,
        participant_id: 2,
    });
    assert!(
        chats
            .on_request_result(req.random_id ^ 1, &waiting)
            .is_err()
    );
    // the peer answers with a value that is not a valid key
    chats.on_request_result(req.random_id, &waiting).unwrap();
    let chat = tl::enums::EncryptedChat::Chat(tl::types::EncryptedChat {
        id: 1,
        access_hash: 1,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        g_a_or_b: vec![1],
        key_fingerprint: 0,
    });
    let o = chats.handle_update(0, &chat_update(chat));
    assert_eq!(o.discard.len(), 1);
    assert!(matches!(
        &o.events[..],
        [ChatsEvent::Discarded { chat_id: 1, .. }]
    ));
    // a request with a bad g_a cannot be accepted, but can be declined
    let bad = tl::enums::EncryptedChat::Requested(tl::types::EncryptedChatRequested {
        folder_id: None,
        id: 2,
        access_hash: 1,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        g_a: vec![1],
    });
    chats.handle_update(0, &chat_update(bad));
    assert!(matches!(chats.accept(2, 0), Err(Error::BadPublicValue)));
    let d = chats.discard(2, true);
    assert!((d.chat_id, d.delete_history) == (2, true));
    assert!(matches!(chats.accept(2, 0), Err(Error::NoSuchChat(2))));
}

#[test]
fn the_larger_exchange_id_wins_a_collision() {
    let (mut a, mut b) = pair();
    a.request_key().unwrap();
    // b's id is the smallest possible: a ignores it and keeps its own exchange
    let theirs = b
        .send_action(Action::RequestKey {
            exchange_id: i64::MIN,
            g_a: vec![1],
        })
        .unwrap();
    assert_eq!(deliver(&mut a, &theirs), Output::default());
    // b's id is the largest: a gives up its own and answers b (here: a bad g_a is aborted)
    let theirs = b
        .send_action(Action::RequestKey {
            exchange_id: i64::MAX,
            g_a: vec![1],
        })
        .unwrap();
    let o = deliver(&mut a, &theirs);
    assert!(matches!(&o.events[..], [Event::RekeyAborted { .. }]));
    assert_eq!(o.send.len(), 1);
}

#[test]
fn a_waiting_chat_update_changes_nothing() {
    let (mut alice, _) = handshake();
    let waiting = tl::enums::EncryptedChat::Waiting(tl::types::EncryptedChatWaiting {
        id: 1,
        access_hash: 1,
        date: 1,
        admin_id: 1,
        participant_id: 2,
    });
    assert!(
        alice
            .handle_update(0, &chat_update(waiting))
            .events
            .is_empty()
    );
}

#[test]
fn errors_have_messages() {
    let errors = [
        Error::BadDhParams,
        Error::BadPublicValue,
        Error::FingerprintMismatch,
        Error::UnknownKey,
        Error::TooShort,
        Error::BadMsgKey,
        Error::BadLength,
        Error::BadRandomBytes,
        Error::UnsupportedLayer(3),
        Error::Deserialize(mtg_tl_types::deserialize::Error::UnexpectedEof),
        Error::Protocol("x"),
        Error::WrongState("y"),
        Error::NoSuchChat(4),
    ];
    for e in errors {
        assert!(e.to_string().starts_with("secret chat: "), "{e:?}");
    }
}
