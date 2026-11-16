use std::net::SocketAddr;
use std::sync::Arc;

use crate::storages::{MemorySession, erase};
use crate::types::{ChannelState, PeerAuth, PeerId, PeerInfo, PeerRef, UpdateState};
use crate::updates::UpdatesLike;
use crate::{Session, SessionData};

fn bytes(parts: &[u32]) -> Vec<u8> {
    parts.iter().flat_map(|p| p.to_le_bytes()).collect()
}

/// Every method of the trait through one implementation (used for the plain and erased ones).
async fn exercise<S: Session + ?Sized>(s: &S)
where
    S::Error: std::fmt::Debug,
{
    s.set_home_dc_id(4).await.unwrap();
    assert_eq!(s.home_dc_id().unwrap(), 4);

    let mut dc = s.dc_option(2).unwrap().unwrap();
    dc.auth_key = Some([7; 256]);
    s.set_dc_option(&dc).await.unwrap();
    assert_eq!(s.dc_option(2).unwrap().unwrap().auth_key, Some([7; 256]));
    assert!(s.dc_option(99).unwrap().is_none());

    let a: SocketAddr = "1.1.1.1:443".parse().unwrap();
    s.set_dc_addresses(2, vec![a]).await.unwrap();
    assert_eq!(s.dc_addresses(2).unwrap(), vec![a]);

    let id = PeerId::user_unchecked(5);
    assert!(s.peer(id).await.unwrap().is_none());
    assert!(s.peer_ref(id).await.unwrap().is_none());
    let info = |h| PeerInfo::User {
        id: 5,
        auth: h,
        bot: None,
        is_self: None,
    };
    s.cache_peers(vec![info(None), PeerInfo::Chat { id: 6 }])
        .await
        .unwrap();
    // a peer without a hash has no usable reference
    assert!(s.peer_ref(id).await.unwrap().is_none());
    s.cache_peer(info(Some(PeerAuth::from_hash(9))))
        .await
        .unwrap();
    assert_eq!(
        s.peer_ref(id).await.unwrap(),
        Some(PeerRef {
            id,
            auth: PeerAuth::from_hash(9)
        })
    );
    assert!(s.peer(PeerId::chat_unchecked(6)).await.unwrap().is_some());

    s.set_update_state(UpdateState::Primary {
        pts: 1,
        date: 2,
        seq: 3,
    })
    .await
    .unwrap();
    s.set_update_state(UpdateState::Secondary { qts: 4 })
        .await
        .unwrap();
    s.set_update_state(UpdateState::Channel { id: 8, pts: 5 })
        .await
        .unwrap();
    s.set_update_state(UpdateState::Channel { id: 8, pts: 6 })
        .await
        .unwrap();
    let st = s.updates_state().await.unwrap();
    assert_eq!((st.pts, st.date, st.seq, st.qts), (1, 2, 3, 4));
    assert_eq!(st.channels, vec![ChannelState { id: 8, pts: 6 }]);
    s.set_update_state(UpdateState::All(Default::default()))
        .await
        .unwrap();
    assert_eq!(s.updates_state().await.unwrap().pts, 0);
}

#[tokio::test]
async fn memory_session_implements_the_trait() {
    exercise(&MemorySession::default()).await;
}

#[tokio::test]
async fn an_erased_session_forwards_everything() {
    let erased = erase(Arc::new(MemorySession::default()));
    exercise(&*erased).await;
}

#[tokio::test]
async fn the_default_trait_methods_keep_the_first_error_free_path() {
    // a storage that only implements the required methods: defaults answer "nothing known"
    struct Bare(MemorySession);
    impl Session for Bare {
        type Error = <MemorySession as Session>::Error;
        fn home_dc_id(&self) -> Result<i32, Self::Error> {
            self.0.home_dc_id()
        }
        fn set_home_dc_id(&self, d: i32) -> crate::BoxFuture<'_, Result<(), Self::Error>> {
            self.0.set_home_dc_id(d)
        }
        fn dc_option(&self, d: i32) -> Result<Option<crate::types::DcOption>, Self::Error> {
            self.0.dc_option(d)
        }
        fn set_dc_option(
            &self,
            o: &crate::types::DcOption,
        ) -> crate::BoxFuture<'_, Result<(), Self::Error>> {
            self.0.set_dc_option(o)
        }
        fn peer(&self, p: PeerId) -> crate::BoxFuture<'_, Result<Option<PeerInfo>, Self::Error>> {
            self.0.peer(p)
        }
        fn cache_peer(&self, p: PeerInfo) -> crate::BoxFuture<'_, Result<(), Self::Error>> {
            self.0.cache_peer(p)
        }
        fn updates_state(
            &self,
        ) -> crate::BoxFuture<'_, Result<crate::types::UpdatesState, Self::Error>> {
            self.0.updates_state()
        }
        fn set_update_state(
            &self,
            u: UpdateState,
        ) -> crate::BoxFuture<'_, Result<(), Self::Error>> {
            self.0.set_update_state(u)
        }
    }
    let s = Bare(MemorySession::default());
    assert!(s.dc_addresses(2).unwrap().is_empty());
    s.set_dc_addresses(2, vec!["1.1.1.1:443".parse().unwrap()])
        .await
        .unwrap();
    assert!(s.dc_addresses(2).unwrap().is_empty());
}

#[tokio::test]
async fn session_data_imports_into_a_storage() {
    let mut data = SessionData {
        home_dc: 5,
        ..Default::default()
    };
    data.peer_infos
        .insert(PeerId::chat_unchecked(3), PeerInfo::Chat { id: 3 });
    data.updates_state.pts = 11;
    let s = MemorySession::default();
    data.import_to(&s).await.unwrap();
    assert_eq!(s.home_dc_id().unwrap(), 5);
    assert!(s.peer(PeerId::chat_unchecked(3)).await.unwrap().is_some());
    assert_eq!(s.updates_state().await.unwrap().pts, 11);
    assert_eq!(s.dc_option(1).unwrap().unwrap().id, 1);
}

#[test]
fn updates_like_from_bytes_knows_the_wrapped_results() {
    let too_long = 0xe317af7eu32;
    assert!(matches!(
        UpdatesLike::from_bytes(&bytes(&[too_long])),
        Ok(UpdatesLike::Updates(_))
    ));
    assert!(matches!(
        UpdatesLike::from_bytes(&bytes(&[0x84d19185, 1, 1])),
        Ok(UpdatesLike::AffectedMessages(_))
    ));
    // invitedUsers: updates, empty vector of missing invitees
    assert!(matches!(
        UpdatesLike::from_bytes(&bytes(&[0x7f5defa6, too_long, 0x1cb5c415, 0])),
        Ok(UpdatesLike::InvitedUsers(_))
    ));
    assert!(matches!(
        UpdatesLike::from_bytes(&bytes(&[0x445663a7, too_long])),
        Ok(UpdatesLike::ChatInviteJoinResult(_))
    ));
    // the web-view variant is not an update; junk fails
    assert!(UpdatesLike::from_bytes(&bytes(&[0x61ca29d3, 0, 0, 0, 0, 0x1cb5c415, 0])).is_err());
    assert!(UpdatesLike::from_bytes(&[1, 2, 3]).is_err());
}
