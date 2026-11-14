// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use {DcOption, KNOWN_DC_OPTIONS, PeerInfo, Session, UpdateState};

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
