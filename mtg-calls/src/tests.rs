use mtg_secret::dh::DhParams;
use mtg_tl_types as tl;
use tl::enums::PhoneCall as P;

use crate::call::*;

const P14: &str = "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aacaa68ffffffffffffffff";

fn config() -> tl::enums::messages::DhConfig {
    tl::enums::messages::DhConfig::Config(tl::types::messages::DhConfig {
        g: 2,
        p: (0..P14.len() / 2)
            .map(|i| u8::from_str_radix(&P14[2 * i..2 * i + 2], 16).unwrap())
            .collect(),
        version: 1,
        random: vec![5; 256],
    })
}

#[test]
fn incoming_calls_need_the_dh_config() {
    let mut c = Calls::new(protocol(92, vec!["2.7.7".into()]));
    assert!(!c.has_dh_config());
    assert!(
        c.handle_update(&update(requested(7, vec![0; 32])))
            .events
            .is_empty()
    );
    c.set_dh_config(&config()).unwrap();
    assert!(c.has_dh_config());
    assert_eq!(
        c.handle_update(&update(requested(7, vec![0; 32])))
            .events
            .len(),
        1
    );
}

fn calls() -> Calls {
    let mut c = Calls::new(protocol(92, vec!["2.7.7".into()]));
    c.set_dh_config(&config()).unwrap();
    c
}

fn waiting(id: i64, receive_date: Option<i32>) -> P {
    P::Waiting(tl::types::PhoneCallWaiting {
        video: false,
        id,
        access_hash: 9,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        protocol: calls().our_protocol(),
        receive_date,
    })
}

fn result(call: P) -> tl::enums::phone::PhoneCall {
    tl::enums::phone::PhoneCall::Call(tl::types::phone::PhoneCall {
        phone_call: call,
        users: vec![],
    })
}

fn requested(id: i64, g_a_hash: Vec<u8>) -> P {
    P::Requested(tl::types::PhoneCallRequested {
        video: true,
        id,
        access_hash: 9,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        g_a_hash,
        protocol: calls().our_protocol(),
    })
}

fn accepted(id: i64, g_b: Vec<u8>) -> P {
    P::Accepted(tl::types::PhoneCallAccepted {
        video: false,
        id,
        access_hash: 9,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        g_b,
        protocol: calls().our_protocol(),
    })
}

fn full(id: i64, g: Vec<u8>, fingerprint: i64) -> P {
    P::Call(tl::types::PhoneCall {
        p2p_allowed: true,
        video: false,
        conference_supported: false,
        id,
        access_hash: 9,
        date: 1,
        admin_id: 1,
        participant_id: 2,
        g_a_or_b: g,
        key_fingerprint: fingerprint,
        protocol: calls().our_protocol(),
        connections: vec![
            tl::enums::PhoneConnection::Connection(tl::types::PhoneConnection {
                tcp: true,
                id: 1,
                ip: "1.2.3.4".into(),
                ipv6: String::new(),
                port: 599,
                peer_tag: vec![1; 16],
            }),
            tl::enums::PhoneConnection::Webrtc(tl::types::PhoneConnectionWebrtc {
                turn: true,
                stun: false,
                id: 2,
                ip: "5.6.7.8".into(),
                ipv6: String::new(),
                port: 3478,
                username: "u".into(),
                password: "p".into(),
            }),
        ],
        start_date: 100,
        custom_parameters: Some(tl::enums::DataJson::Json(tl::types::DataJson {
            data: "{}".into(),
        })),
    })
}

fn update(call: P) -> tl::enums::Update {
    tl::enums::Update::PhoneCall(tl::types::UpdatePhoneCall { phone_call: call })
}

fn peer() -> tl::enums::InputUser {
    tl::enums::InputUser::UserSelf
}

fn connected(out: &Output) -> &Connected {
    match &out.events[..] {
        [Event::Connected(c)] => c,
        e => panic!("{e:?}"),
    }
}

/// Runs a call up to the callee's `accept`; returns (caller, callee, request, accept).
fn up_to_accept() -> (
    Calls,
    Calls,
    tl::functions::phone::RequestCall,
    tl::functions::phone::AcceptCall,
) {
    let (mut a, mut b) = (calls(), calls());
    let req = a.request(peer(), false).unwrap();
    let (id, out) = a
        .on_request_result(req.random_id, &result(waiting(7, None)))
        .unwrap();
    assert_eq!((id, out), (7, Output::default()));
    let out = b.handle_update(&update(requested(7, req.g_a_hash.clone())));
    assert_eq!(
        out.events,
        [Event::Incoming {
            call_id: 7,
            peer_id: 1,
            video: true
        }]
    );
    assert!(b.received(7).is_ok());
    let acc = b.accept(7).unwrap();
    (a, b, req, acc)
}

#[test]
fn handshake_gives_equal_keys() {
    let (mut a, mut b, _, acc) = up_to_accept();
    assert_eq!(
        a.handle_update(&update(waiting(7, Some(3)))).events,
        [Event::Ringing { call_id: 7 }]
    );
    assert!(a.handle_update(&update(waiting(7, None))).events.is_empty());
    let out = a.handle_update(&update(accepted(7, acc.g_b.clone())));
    let [Request::Confirm(confirm)] = &out.send[..] else {
        panic!("{out:?}")
    };
    let out_b = b.handle_update(&update(full(
        7,
        confirm.g_a.clone(),
        confirm.key_fingerprint,
    )));
    let kb = connected(&out_b).clone();
    let out_a = a.handle_update(&update(full(7, acc.g_b, confirm.key_fingerprint)));
    let ka = connected(&out_a).clone();
    assert_eq!(ka.key, kb.key);
    assert_eq!(ka.emoji, kb.emoji);
    assert!(ka.outgoing && !kb.outgoing);
    assert_eq!(ka.endpoints.len(), 2);
    assert!(matches!(
        ka.endpoints[0],
        Endpoint::Reflector {
            tcp: true,
            port: 599,
            ..
        }
    ));
    assert!(matches!(
        ka.endpoints[1],
        Endpoint::Webrtc {
            turn: true,
            port: 3478,
            ..
        }
    ));
    assert_eq!(ka.custom_parameters.as_deref(), Some("{}"));
    assert_eq!(ka.start_date, 100);
    assert!(ka.p2p_allowed);
    assert_eq!(a.access_hash(7), Some(9));
    // a second phoneCall changes nothing
    assert!(
        a.handle_update(&update(full(7, vec![], 0)))
            .events
            .is_empty()
    );
}

#[test]
fn callee_rejects_wrong_hash_and_fingerprint() {
    let (mut a, mut b, _, acc) = up_to_accept();
    let out = a.handle_update(&update(accepted(7, acc.g_b)));
    let [Request::Confirm(confirm)] = &out.send[..] else {
        panic!()
    };
    // g_a that does not match g_a_hash
    let mut other = confirm.g_a.clone();
    other[10] ^= 1;
    let out = b.handle_update(&update(full(7, other, confirm.key_fingerprint)));
    assert!(matches!(
        &out.events[..],
        [Event::Failed { call_id: 7, .. }]
    ));
    assert!(matches!(&out.send[..], [Request::Discard(d)] if d.video));
    assert!(b.access_hash(7).is_none());

    let (mut a, mut b, _, acc) = up_to_accept();
    let out = a.handle_update(&update(accepted(7, acc.g_b)));
    let [Request::Confirm(confirm)] = &out.send[..] else {
        panic!()
    };
    let out = b.handle_update(&update(full(
        7,
        confirm.g_a.clone(),
        confirm.key_fingerprint ^ 1,
    )));
    assert!(matches!(&out.events[..], [Event::Failed { .. }]));
}

#[test]
fn bad_g_a_and_bad_g_b_fail() {
    // g_a = 1 with a matching hash is out of range
    let (_, mut b, _, _) = up_to_accept();
    let g_a = vec![1u8];
    let hash = mtg_crypto::sha256_parts(&[&g_a]).to_vec();
    let mut b2 = calls();
    b2.handle_update(&update(requested(8, hash)));
    b2.accept(8).unwrap();
    let out = b2.handle_update(&update(full(8, g_a, 0)));
    assert!(matches!(
        &out.events[..],
        [Event::Failed { call_id: 8, .. }]
    ));
    let _ = &mut b;

    let (mut a, _, _, _) = up_to_accept();
    let out = a.handle_update(&update(accepted(7, vec![1])));
    assert!(matches!(&out.events[..], [Event::Failed { .. }]));
    assert!(matches!(&out.send[..], [Request::Discard(_)]));
}

#[test]
fn caller_rejects_wrong_fingerprint() {
    let (mut a, _, _, acc) = up_to_accept();
    let out = a.handle_update(&update(accepted(7, acc.g_b.clone())));
    let [Request::Confirm(confirm)] = &out.send[..] else {
        panic!()
    };
    let out = a.handle_update(&update(full(7, acc.g_b, confirm.key_fingerprint ^ 1)));
    assert!(matches!(&out.events[..], [Event::Failed { .. }]));
}

#[test]
fn discard_ends_the_call() {
    let (mut a, _, _, _) = up_to_accept();
    let d = P::Discarded(tl::types::PhoneCallDiscarded {
        need_rating: true,
        need_debug: false,
        video: false,
        id: 7,
        reason: Some(tl::enums::PhoneCallDiscardReason::Busy),
        duration: Some(12),
    });
    let out = a.handle_update(&update(d.clone()));
    assert_eq!(
        out.events,
        [Event::Ended {
            call_id: 7,
            reason: Some(tl::enums::PhoneCallDiscardReason::Busy),
            duration: Some(12),
            need_rating: true,
            need_debug: false
        }]
    );
    assert!(a.handle_update(&update(d)).events.is_empty());
}

#[test]
fn signalling_goes_both_ways_for_live_calls_only() {
    let (mut a, _, _, _) = up_to_accept();
    let sig = |id| {
        tl::enums::Update::PhoneCallSignalingData(tl::types::UpdatePhoneCallSignalingData {
            phone_call_id: id,
            data: vec![1, 2],
        })
    };
    assert_eq!(
        a.handle_update(&sig(7)).events,
        [Event::Signaling {
            call_id: 7,
            data: vec![1, 2]
        }]
    );
    assert!(a.handle_update(&sig(8)).events.is_empty());
    let s = a.signal(7, vec![3]).unwrap();
    assert_eq!(s.data, vec![3]);
    assert!(a.signal(8, vec![]).is_err());
}

#[test]
fn own_discard_forgets_the_call() {
    let (_, mut b, _, _) = up_to_accept();
    let d = b
        .discard(7, 5, tl::enums::PhoneCallDiscardReason::Hangup, 3)
        .unwrap();
    assert_eq!((d.duration, d.connection_id, d.video), (5, 3, true));
    assert!(
        b.discard(7, 0, tl::enums::PhoneCallDiscardReason::Hangup, 0)
            .is_err()
    );
    assert!(b.received(7).is_err());
}

#[test]
fn early_updates_are_replayed() {
    let (mut a, mut b) = (calls(), calls());
    let req = a.request(peer(), false).unwrap();
    // the callee answers before the requestCall result reaches us
    b.handle_update(&update(requested(7, req.g_a_hash.clone())));
    let acc = b.accept(7).unwrap();
    assert!(
        a.handle_update(&update(accepted(7, acc.g_b)))
            .send
            .is_empty()
    );
    assert!(
        a.handle_update(&update(waiting(7, Some(1))))
            .events
            .is_empty()
    );
    let (id, out) = a
        .on_request_result(req.random_id, &result(waiting(7, None)))
        .unwrap();
    assert_eq!(id, 7);
    assert!(matches!(&out.send[..], [Request::Confirm(_)]));
    assert_eq!(out.events, [Event::Ringing { call_id: 7 }]);
    // a discard drops what is held for the call
    let mut c = calls();
    let r = c.request(peer(), false).unwrap();
    c.handle_update(&update(accepted(8, vec![2])));
    c.handle_update(&update(P::Discarded(tl::types::PhoneCallDiscarded {
        need_rating: false,
        need_debug: false,
        video: false,
        id: 8,
        reason: None,
        duration: None,
    })));
    let (_, out) = c
        .on_request_result(r.random_id, &result(waiting(8, None)))
        .unwrap();
    assert_eq!(out, Output::default());
}

#[test]
fn early_queue_is_capped() {
    let mut c = calls();
    let r = c.request(peer(), false).unwrap();
    for id in 0..20 {
        c.handle_update(&update(accepted(100 + id, vec![2])));
    }
    // the oldest were dropped, the newest kept
    let (_, out) = c
        .on_request_result(r.random_id, &result(waiting(100, None)))
        .unwrap();
    assert!(out.send.is_empty());
    assert_eq!(c.early_len(), 8);
}

#[test]
fn ignored_updates() {
    let mut c = calls();
    assert_eq!(
        c.handle_update(&tl::enums::Update::Config),
        Output::default()
    );
    // nothing asked: nothing held
    assert!(
        c.handle_update(&update(waiting(1, Some(1))))
            .events
            .is_empty()
    );
    assert!(
        c.handle_update(&update(accepted(1, vec![2])))
            .events
            .is_empty()
    );
    assert!(
        c.handle_update(&update(full(1, vec![2], 0)))
            .events
            .is_empty()
    );
    assert_eq!(c.early_len(), 0);
    assert!(
        c.handle_update(&update(P::Empty(tl::types::PhoneCallEmpty { id: 1 })))
            .events
            .is_empty()
    );
    // a duplicate request does not ring twice; the same id of a call we placed is ignored
    let (_, mut b, req, _) = up_to_accept();
    assert!(
        b.handle_update(&update(requested(7, req.g_a_hash)))
            .events
            .is_empty()
    );
    // accepted/full in a state that does not expect them
    let (mut a, _, _, acc) = up_to_accept();
    assert!(
        a.handle_update(&update(accepted(7, acc.g_b.clone())))
            .events
            .is_empty()
    );
    assert!(
        a.handle_update(&update(accepted(7, acc.g_b)))
            .send
            .is_empty()
    );
}

#[test]
fn errors() {
    let mut c = Calls::new(protocol(92, vec![]));
    assert!(c.request(peer(), false).is_err());
    assert!(c.accept(1).is_err());
    assert_eq!(c.dh_config_request().version, 0);
    // an incoming call before the DH config is ignored
    assert!(
        c.handle_update(&update(requested(1, vec![0; 32])))
            .events
            .is_empty()
    );
    c.set_dh_config(&config()).unwrap();
    assert_eq!(c.dh_config_request().version, 1);
    assert!(c.accept(1).is_err());
    let not_waiting = result(P::Empty(tl::types::PhoneCallEmpty { id: 1 }));
    assert!(c.on_request_result(1, &not_waiting).is_err());
    assert!(c.on_request_result(1, &result(waiting(1, None))).is_err());
    // accepting twice
    let (_, mut b, _, _) = up_to_accept();
    assert!(b.accept(7).is_err());
    // a bad prime
    let bad = tl::enums::messages::DhConfig::Config(tl::types::messages::DhConfig {
        g: 2,
        p: vec![7; 256],
        version: 2,
        random: vec![0; 256],
    });
    assert!(c.set_dh_config(&bad).is_err());
    let _ = DhParams::new;
}

#[test]
fn builders() {
    let p = protocol(92, vec!["a".into()]);
    assert!(p.udp_p2p && p.udp_reflector);
    assert_eq!((p.min_layer, p.max_layer), (MIN_LAYER, 92));
    let r = Calls::rate(1, 2, 5, "ok");
    assert_eq!((r.rating, r.comment.as_str()), (5, "ok"));
    let d = Calls::debug(1, 2, "{}");
    assert!(matches!(d.debug, tl::enums::DataJson::Json(j) if j.data == "{}"));
}
