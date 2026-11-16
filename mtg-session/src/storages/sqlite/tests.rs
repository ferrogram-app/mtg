// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use {ChannelKind, DcOption, KNOWN_DC_OPTIONS, PeerAuth, PeerInfo, Session, UpdateState};

use super::*;

#[test]
fn exercise_sqlite_session() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(do_exercise_sqlite_session());
}

async fn do_exercise_sqlite_session() {
    let session = SqliteSession::open(":memory:").await.unwrap();

    assert_eq!(session.home_dc_id().unwrap(), DEFAULT_DC);
    session.set_home_dc_id(DEFAULT_DC + 1).await.unwrap();
    assert_eq!(session.home_dc_id().unwrap(), DEFAULT_DC + 1);

    assert_eq!(
        session.dc_option(KNOWN_DC_OPTIONS[0].id).unwrap(),
        Some(KNOWN_DC_OPTIONS[0].clone())
    );
    let new_dc_option = DcOption {
        id: KNOWN_DC_OPTIONS
            .iter()
            .map(|dc_option| dc_option.id)
            .max()
            .unwrap()
            + 1,
        ipv4: SocketAddrV4::new(Ipv4Addr::from_bits(0), 1),
        ipv6: SocketAddrV6::new(Ipv6Addr::from_bits(0), 1, 0, 0),
        auth_key: Some([1; 256]),
    };
    assert_eq!(session.dc_option(new_dc_option.id).unwrap(), None);
    session.set_dc_option(&new_dc_option).await.unwrap();
    assert_eq!(
        session.dc_option(new_dc_option.id).unwrap(),
        Some(new_dc_option)
    );

    assert_eq!(session.peer(PeerId::self_user()).await.unwrap(), None);
    assert_eq!(session.peer(PeerId::user_unchecked(1)).await.unwrap(), None);
    let peer = PeerInfo::User {
        id: 1,
        auth: None,
        bot: Some(true),
        is_self: Some(true),
    };
    session.cache_peer(peer.clone()).await.unwrap();
    assert_eq!(
        session.peer(PeerId::self_user()).await.unwrap(),
        Some(peer.clone())
    );
    assert_eq!(
        session.peer(PeerId::user_unchecked(1)).await.unwrap(),
        Some(peer)
    );

    assert_eq!(
        session.peer(PeerId::channel_unchecked(1)).await.unwrap(),
        None
    );
    let peer = PeerInfo::Channel {
        id: 1,
        auth: Some(PeerAuth::from_hash(-1)),
        kind: Some(ChannelKind::Broadcast),
    };
    session.cache_peer(peer.clone()).await.unwrap();
    assert_eq!(
        session.peer(PeerId::channel_unchecked(1)).await.unwrap(),
        Some(peer)
    );

    let peers = vec![
        PeerInfo::Channel {
            id: 2,
            auth: Some(PeerAuth::from_hash(-2)),
            kind: Some(ChannelKind::Megagroup),
        },
        PeerInfo::Channel {
            id: 3,
            auth: Some(PeerAuth::from_hash(-3)),
            kind: None,
        },
    ];
    session.cache_peers(peers.clone()).await.unwrap();
    for peer in peers.iter() {
        assert_eq!(session.peer(peer.id()).await.unwrap().as_ref(), Some(peer));
    }

    // If the inserted peer is going to be updated with some fields changed
    // from `Some` to `None`, the old `Some` is expected to be kept.
    let new_inserted_peers = vec![
        PeerInfo::Channel {
            id: 2,
            auth: None,
            kind: Some(ChannelKind::Broadcast),
        },
        PeerInfo::Channel {
            id: 3,
            auth: Some(PeerAuth::from_hash(-30)),
            kind: None,
        },
    ];
    session.cache_peers(new_inserted_peers).await.unwrap();
    assert_eq!(
        session.peer(PeerId::channel_unchecked(2)).await.unwrap(),
        Some(PeerInfo::Channel {
            id: 2,
            auth: Some(PeerAuth::from_hash(-2)),
            kind: Some(ChannelKind::Broadcast),
        })
    );
    assert_eq!(
        session.peer(PeerId::channel_unchecked(3)).await.unwrap(),
        Some(PeerInfo::Channel {
            id: 3,
            auth: Some(PeerAuth::from_hash(-30)),
            kind: None
        })
    );

    assert_eq!(
        session.updates_state().await.unwrap(),
        UpdatesState::default()
    );
    session
        .set_update_state(UpdateState::All(UpdatesState {
            pts: 1,
            qts: 2,
            date: 3,
            seq: 4,
            channels: vec![
                ChannelState { id: 5, pts: 6 },
                ChannelState { id: 7, pts: 8 },
            ],
        }))
        .await
        .unwrap();
    session
        .set_update_state(UpdateState::Primary {
            pts: 2,
            date: 4,
            seq: 5,
        })
        .await
        .unwrap();
    session
        .set_update_state(UpdateState::Secondary { qts: 3 })
        .await
        .unwrap();
    session
        .set_update_state(UpdateState::Channel { id: 7, pts: 9 })
        .await
        .unwrap();
    assert_eq!(
        session.updates_state().await.unwrap(),
        UpdatesState {
            pts: 2,
            qts: 3,
            date: 4,
            seq: 5,
            channels: vec![
                ChannelState { id: 5, pts: 6 },
                ChannelState { id: 7, pts: 9 },
            ],
        }
    );
}

#[tokio::test]
async fn dc_addresses_are_ordered_and_persisted() {
    let path = std::env::temp_dir().join(format!("mtg-dc-addr-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let list: Vec<std::net::SocketAddr> =
        vec!["1.2.3.4:443".parse().unwrap(), "[::1]:80".parse().unwrap()];
    {
        let session = SqliteSession::open(path.to_str().unwrap()).await.unwrap();
        assert!(session.dc_addresses(2).unwrap().is_empty());
        session.set_dc_addresses(2, list.clone()).await.unwrap();
        session
            .set_dc_addresses(2, list.iter().rev().cloned().collect())
            .await
            .unwrap();
        assert_eq!(session.dc_addresses(2).unwrap(), vec![list[1], list[0]]);
    }
    let session = SqliteSession::open(path.to_str().unwrap()).await.unwrap();
    assert_eq!(session.dc_addresses(2).unwrap(), vec![list[1], list[0]]);
    assert!(session.dc_addresses(3).unwrap().is_empty());
    drop(session);
    let _ = std::fs::remove_file(&path);
}

fn temp_db(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("mtg-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[tokio::test]
async fn peers_keep_their_subtype_and_are_merged_on_update() {
    let s = SqliteSession::open(":memory:").await.unwrap();
    let user = |id, bot, is_self| PeerInfo::User {
        id,
        auth: Some(PeerAuth::from_hash(id)),
        bot: Some(bot),
        is_self: Some(is_self),
    };
    let channel = |id, kind| PeerInfo::Channel {
        id,
        auth: Some(PeerAuth::from_hash(id)),
        kind,
    };
    let peers = vec![
        user(1, false, false),
        user(2, true, false),
        user(3, false, true),
        user(4, true, true),
        PeerInfo::Chat { id: 5 },
        channel(6, Some(ChannelKind::Megagroup)),
        channel(7, Some(ChannelKind::Broadcast)),
        channel(8, Some(ChannelKind::Gigagroup)),
        channel(9, Some(ChannelKind::Community)),
        channel(10, None),
    ];
    // the bulk path and the single path must agree
    s.cache_peers(peers.clone()).await.unwrap();
    for p in &peers {
        let got = s.peer(p.id()).await.unwrap().unwrap();
        match p {
            // a community is stored but read back without a kind
            PeerInfo::Channel {
                kind: Some(ChannelKind::Community),
                ..
            } => {
                assert!(matches!(got, PeerInfo::Channel { kind: None, .. }))
            }
            // a user without flags has no subtype stored
            PeerInfo::User { id: 1, .. } => assert_eq!(
                got,
                PeerInfo::User {
                    id: 1,
                    auth: Some(PeerAuth::from_hash(1)),
                    bot: None,
                    is_self: None
                }
            ),
            _ => assert_eq!(&got, p),
        }
    }
    let single = SqliteSession::open(":memory:").await.unwrap();
    for p in peers.clone() {
        single.cache_peer(p).await.unwrap();
    }
    for p in &peers {
        assert_eq!(
            single.peer(p.id()).await.unwrap(),
            s.peer(p.id()).await.unwrap()
        );
    }
    assert_eq!(
        single
            .peer(PeerId::self_user())
            .await
            .unwrap()
            .unwrap()
            .id(),
        PeerId::user_unchecked(3)
    );

    // known facts are not lost when a later update knows less
    let weak = PeerInfo::User {
        id: 2,
        auth: None,
        bot: None,
        is_self: None,
    };
    s.cache_peers(vec![weak.clone()]).await.unwrap();
    single.cache_peer(weak).await.unwrap();
    for x in [&s, &single] {
        assert_eq!(
            x.peer(PeerId::user_unchecked(2)).await.unwrap(),
            Some(user(2, true, false))
        );
    }
}

#[tokio::test]
async fn update_state_is_inserted_then_updated() {
    for first_primary in [true, false] {
        let s = SqliteSession::open(":memory:").await.unwrap();
        if first_primary {
            s.set_update_state(UpdateState::Primary {
                pts: 1,
                date: 2,
                seq: 3,
            })
            .await
            .unwrap();
            s.set_update_state(UpdateState::Primary {
                pts: 4,
                date: 5,
                seq: 6,
            })
            .await
            .unwrap();
            s.set_update_state(UpdateState::Secondary { qts: 7 })
                .await
                .unwrap();
        } else {
            s.set_update_state(UpdateState::Secondary { qts: 7 })
                .await
                .unwrap();
            s.set_update_state(UpdateState::Secondary { qts: 8 })
                .await
                .unwrap();
            s.set_update_state(UpdateState::Primary {
                pts: 4,
                date: 5,
                seq: 6,
            })
            .await
            .unwrap();
        }
        let st = s.updates_state().await.unwrap();
        assert_eq!((st.pts, st.date, st.seq), (4, 5, 6));
        assert_eq!(st.qts, if first_primary { 7 } else { 8 });
    }
}

#[tokio::test]
async fn bad_rows_in_the_database_are_errors_when_opening() {
    let path = temp_db("bad-rows");
    let p = path.to_str().unwrap();
    let s = SqliteSession::open(p).await.unwrap();
    {
        let db = s.database.lock().await;
        db.0.execute(
            "INSERT INTO dc_option VALUES (1, '1.2.3.4:443', '[::1]:443', x'0102')",
            (),
        )
        .await
        .unwrap();
    }
    drop(s);
    let err = SqliteSession::open(p).await.err().unwrap();
    assert!(matches!(err, SqliteSessionError::InvalidAuthKeyLength(2)));
    assert_eq!(
        err.to_string(),
        "invalid auth_key length: expected 256, got 2"
    );

    {
        let conn = libsql::Builder::new_local(p)
            .build()
            .await
            .unwrap()
            .connect()
            .unwrap();
        conn.execute("UPDATE dc_option SET auth_key = NULL", ())
            .await
            .unwrap();
        conn.execute("UPDATE dc_option SET ipv4 = 'nope' WHERE dc_id = 1", ())
            .await
            .unwrap();
    }
    let err = SqliteSession::open(p).await.err().unwrap();
    assert!(matches!(err, SqliteSessionError::AddrParse(_)));
    assert_eq!(err.to_string(), "invalid socket address syntax");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_poisoned_cache_is_an_error_not_a_panic() {
    let s = std::sync::Arc::new(SqliteSession::open(":memory:").await.unwrap());
    let t = s.clone();
    let _ = std::thread::spawn(move || {
        let _g = t.cache.lock().unwrap();
        panic!("poison");
    })
    .join();
    assert!(matches!(s.home_dc_id(), Err(SqliteSessionError::Poisoned)));
    assert!(s.dc_option(1).is_err());
    assert!(s.dc_addresses(1).is_err());
    assert!(s.set_home_dc_id(1).await.is_err());
    assert!(s.set_dc_option(&KNOWN_DC_OPTIONS[0]).await.is_err());
    assert!(s.set_dc_addresses(1, vec![]).await.is_err());
    assert_eq!(
        SqliteSessionError::Poisoned.to_string(),
        "session lock is poisoned"
    );
}
