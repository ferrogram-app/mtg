use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use mtg_session::types::DcOption;

use super::*;
use crate::RpcError;

fn rpc(code: i32, name: &str) -> InvocationError {
    InvocationError::Rpc(RpcError {
        code,
        name: name.into(),
        value: None,
        caused_by: None,
    })
}

#[test]
fn only_a_406_auth_key_duplicated_invalidates_the_key() {
    assert!(is_auth_key_duplicated(&rpc(406, "AUTH_KEY_DUPLICATED")));
    assert!(!is_auth_key_duplicated(&rpc(406, "OTHER")));
    assert!(!is_auth_key_duplicated(&rpc(401, "AUTH_KEY_DUPLICATED")));
    assert!(!is_auth_key_duplicated(&InvocationError::InvalidDc));
}

#[tokio::test]
async fn the_runner_forgets_the_key_of_a_duplicated_session() {
    let mut session = mtg_session::SessionData::default();
    let mut dc = empty();
    dc.auth_key = Some([7; 256]);
    session.dc_options.insert(2, dc);
    let session = Arc::new(mtg_session::storages::MemorySession::from(session));
    let pool = SenderPool::new(Arc::clone(&session), 1);
    let mut runner = pool.runner;

    let (tx, rx) = oneshot::channel();
    let flow = runner
        .process_request(Request::CheckRetry {
            dc_id: 2,
            error: rpc(406, "AUTH_KEY_DUPLICATED"),
            fail_count: NonZeroU32::MIN,
            slept_so_far: Duration::ZERO,
            tx,
        })
        .await;
    assert!(flow.is_continue());
    assert!(matches!(rx.await, Ok(ControlFlow::Break(_))));
    assert_eq!(session.dc_option(2).unwrap().unwrap().auth_key, None);

    // another error leaves the key alone
    let mut dc = empty();
    dc.auth_key = Some([7; 256]);
    session.set_dc_option(&dc).await.unwrap();
    let (tx, _rx) = oneshot::channel();
    let _ = runner
        .process_request(Request::CheckRetry {
            dc_id: 2,
            error: rpc(420, "FLOOD_WAIT"),
            fail_count: NonZeroU32::MIN,
            slept_so_far: Duration::ZERO,
            tx,
        })
        .await;
    assert!(session.dc_option(2).unwrap().unwrap().auth_key.is_some());
}

#[test]
fn only_a_transport_404_means_an_unknown_key() {
    use mtg_mtproto::{mtp::DeserializeError, transport::Error};
    assert!(is_unknown_auth_key(&InvocationError::Transport(
        Error::BadStatus { status: 404 }
    )));
    assert!(is_unknown_auth_key(&InvocationError::Deserialize(
        DeserializeError::Transport(Error::BadStatus { status: 404 })
    )));
    assert!(!is_unknown_auth_key(&InvocationError::Transport(
        Error::BadStatus { status: 429 }
    )));
    assert!(!is_unknown_auth_key(&InvocationError::InvalidDc));
}

#[tokio::test]
async fn repeated_404_drops_the_key() {
    use mtg_mtproto::transport::Error;
    let mut session = mtg_session::SessionData::default();
    let mut dc = empty();
    dc.auth_key = Some([7; 256]);
    session.dc_options.insert(2, dc);
    let session = Arc::new(mtg_session::storages::MemorySession::from(session));
    let mut runner = SenderPool::new(Arc::clone(&session), 1).runner;
    let mut check = async |error: InvocationError| {
        let (tx, _rx) = oneshot::channel();
        let _ = runner
            .process_request(Request::CheckRetry {
                dc_id: 2,
                error,
                fail_count: NonZeroU32::MIN,
                slept_so_far: Duration::ZERO,
                tx,
            })
            .await;
    };
    let bad = || InvocationError::Transport(Error::BadStatus { status: 404 });
    let has_key = || session.dc_option(2).unwrap().unwrap().auth_key.is_some();
    // two in a row, then another error resets the count
    check(bad()).await;
    check(bad()).await;
    check(rpc(420, "FLOOD_WAIT")).await;
    check(bad()).await;
    check(bad()).await;
    assert!(has_key());
    check(bad()).await;
    assert!(!has_key());
}

fn empty() -> DcOption {
    DcOption {
        id: 2,
        ipv4: SocketAddrV4::new(Ipv4Addr::from_bits(0), 0),
        ipv6: SocketAddrV6::new(Ipv6Addr::from_bits(0), 0, 0, 0),
        auth_key: None,
    }
}

#[test]
fn debug_of_a_request_survives_a_body_shorter_than_a_constructor_id() {
    for body in [vec![], vec![1, 2, 3]] {
        let (tx, _rx) = oneshot::channel();
        let text = format!(
            "{:?}",
            Request::Invoke {
                dc_id: 2,
                media: false,
                body,
                tx
            }
        );
        assert!(text.contains("\"?\""), "{text}");
    }
}
