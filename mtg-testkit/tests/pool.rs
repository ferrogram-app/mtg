//! The fake server end to end with the real sender pool.

use mtg_mtsender::{InvocationError, SenderPool};
use mtg_testkit::{FakeServer, Fault};
use mtg_tl_types as tl;

async fn pool(server: &FakeServer) -> mtg_mtsender::SenderPoolFatHandle {
    let pool = SenderPool::new(server.session(), 1);
    tokio::spawn(pool.runner.run());
    pool.handle
}

#[tokio::test(flavor = "multi_thread")]
async fn sender_pool_talks_to_the_fake_server_without_a_handshake() {
    let server = FakeServer::start().await;
    let handle = pool(&server).await;
    let config = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        handle.invoke_in_dc(2, &tl::functions::help::GetConfig {}),
    )
    .await
    .expect("no answer")
    .unwrap();
    let tl::enums::Config::Config(config) = config;
    assert_eq!(config.this_dc, 2);
    // the pool's initConnection wraps a GetConfig of its own, then ours
    assert_eq!(server.requests(), vec!["GetConfig", "GetConfig"]);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn unmocked_requests_and_errors_become_rpc_errors() {
    let server = FakeServer::start().await;
    server.on(|_: tl::functions::help::GetAppConfig| Err((420, "FLOOD_WAIT_0".into())));
    let handle = pool(&server).await;

    let err = handle
        .invoke_in_dc(2, &tl::functions::help::GetAppConfig { hash: 0 })
        .await
        .unwrap_err();
    assert!(matches!(&err, InvocationError::Rpc(e) if e.code == 420 && e.name == "FLOOD_WAIT"));

    let err = handle
        .invoke_in_dc(2, &tl::functions::help::GetNearestDc {})
        .await
        .unwrap_err();
    assert!(
        matches!(&err, InvocationError::Rpc(e) if e.code == 400 && e.name.starts_with("NOT_MOCKED"))
    );
    handle.quit();
}

fn app_config() -> tl::functions::help::GetAppConfig {
    tl::functions::help::GetAppConfig { hash: 0 }
}

async fn call(
    handle: &mtg_mtsender::SenderPoolFatHandle,
) -> Result<tl::enums::help::AppConfig, InvocationError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        handle.invoke_in_dc(2, &app_config()),
    )
    .await
    .expect("no answer")
}

fn mock_app_config(server: &FakeServer) {
    server.on(|_: tl::functions::help::GetAppConfig| Ok(tl::enums::help::AppConfig::NotModified));
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_server_salt_is_followed_by_a_resend() {
    let server = FakeServer::start().await;
    mock_app_config(&server);
    let handle = pool(&server).await;
    call(&handle).await.unwrap(); // warm up: connection and init
    server.inject(Fault::BadServerSalt { new_salt: 42 });
    call(&handle).await.unwrap();
    assert_eq!(
        server.connections(),
        1,
        "the salt is fixed on the same connection"
    );
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_msg_notification_about_the_clock_is_followed_by_a_resend() {
    let server = FakeServer::start().await;
    mock_app_config(&server);
    let handle = pool(&server).await;
    call(&handle).await.unwrap();
    server.inject(Fault::BadMsg { error_code: 16 });
    call(&handle).await.unwrap();
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_keeps_sending_bad_msg_gets_the_request_dropped() {
    let server = FakeServer::start().await;
    mock_app_config(&server);
    let handle = pool(&server).await;
    call(&handle).await.unwrap();
    for _ in 0..20 {
        server.inject(Fault::BadMsg { error_code: 16 });
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), call(&handle)).await;
    assert!(
        matches!(result, Ok(Err(InvocationError::Dropped(_)))),
        "{result:?}"
    );
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_connection_is_reopened() {
    let server = FakeServer::start().await;
    mock_app_config(&server);
    let handle = pool(&server).await;
    call(&handle).await.unwrap();
    server.inject(Fault::Close);
    let result = call(&handle).await;
    println!(
        "after close: {result:?}, connections: {}",
        server.connections()
    );
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn garbage_from_the_server_does_not_hang_the_client() {
    let server = FakeServer::start().await;
    mock_app_config(&server);
    let handle = pool(&server).await;
    call(&handle).await.unwrap();
    server.inject(Fault::Garbage);
    let result = call(&handle).await;
    println!(
        "after garbage: {result:?}, connections: {}",
        server.connections()
    );
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn connector_falls_back_to_the_next_address_and_remembers_it() {
    use mtg_session::Session;

    let server = FakeServer::start().await;
    let session = server.session();
    // a closed port as the known address, the live server only in the stored list
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let mut dc = session.dc_option(2).unwrap().unwrap();
    let std::net::SocketAddr::V4(dead4) = dead else {
        unreachable!()
    };
    dc.ipv4 = dead4;
    session.set_dc_option(&dc).await.unwrap();
    session
        .set_dc_addresses(2, vec![dead, server.addr])
        .await
        .unwrap();

    let pool = SenderPool::new(session.clone(), 1);
    tokio::spawn(pool.runner.run());
    let handle = pool.handle;
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        handle.invoke_in_dc(2, &tl::functions::help::GetConfig {}),
    )
    .await
    .expect("no answer")
    .unwrap();

    let std::net::SocketAddr::V4(live) = server.addr else {
        unreachable!()
    };
    assert_eq!(session.dc_option(2).unwrap().unwrap().ipv4, live);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn connector_reports_the_last_error_when_every_address_is_dead() {
    use mtg_session::Session;

    let server = FakeServer::start().await;
    let session = server.session();
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let mut dc = session.dc_option(2).unwrap().unwrap();
    let std::net::SocketAddr::V4(dead4) = dead else {
        unreachable!()
    };
    dc.ipv4 = dead4;
    session.set_dc_option(&dc).await.unwrap();

    let pool = SenderPool::new(session, 1);
    tokio::spawn(pool.runner.run());
    let err = pool
        .handle
        .invoke_in_dc(2, &tl::functions::help::GetConfig {})
        .await
        .unwrap_err();
    assert!(matches!(err, InvocationError::Io(_)), "{err:?}");
    pool.handle.quit();
}

async fn through_proxy(secret_text: &str, padded: bool) {
    let secret: [u8; 16] = std::array::from_fn(|i| i as u8 * 3 + 1);
    let server = FakeServer::start_proxy(secret).await;
    run_through(server, secret, secret_text, "", padded).await;
}

async fn run_through(
    server: FakeServer,
    secret: [u8; 16],
    prefix: &str,
    suffix: &str,
    padded: bool,
) {
    mock_app_config(&server);
    let hex: String = secret.iter().map(|b| format!("{b:02x}")).collect();
    let link = format!(
        "tg://proxy?server=127.0.0.1&port={}&secret={prefix}{hex}{suffix}",
        server.addr.port()
    );
    let proxy = mtg_mtsender::MtProxy::from_link(&link).unwrap();
    assert_eq!(proxy.padded, padded);
    let session = server.session();
    // the datacenter address is dead: only the proxy can answer
    use mtg_session::Session;
    let mut dc = session.dc_option(2).unwrap().unwrap();
    dc.ipv4 = "127.0.0.1:1".parse().unwrap();
    session.set_dc_option(&dc).await.unwrap();
    let params = mtg_mtsender::ConnectionParams {
        mtproxy: Some(proxy),
        ..Default::default()
    };
    let pool = SenderPool::with_configuration(session, 1, params);
    tokio::spawn(pool.runner.run());
    let handle = pool.handle;
    // several round trips: the stream cipher and (for dd) the padding must stay in step
    for _ in 0..3 {
        call(&handle).await.unwrap();
    }
    assert_eq!(server.proxy_dcs(), vec![2]);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn mtproxy_plain_secret_end_to_end() {
    through_proxy("", false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mtproxy_padded_secret_end_to_end() {
    through_proxy("dd", true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mtproxy_fake_tls_end_to_end() {
    let secret: [u8; 16] = std::array::from_fn(|i| i as u8 * 5 + 2);
    let server = FakeServer::start_proxy_tls(secret).await;
    let domain: String = "cdn.example.org"
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    run_through(server, secret, "ee", &domain, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_tls_with_the_wrong_secret_fails_instead_of_hanging() {
    let secret: [u8; 16] = [3; 16];
    let server = FakeServer::start_proxy_tls(secret).await;
    let wrong: String = [4u8; 16].iter().map(|b| format!("{b:02x}")).collect();
    let link = format!(
        "tg://proxy?server=127.0.0.1&port={}&secret=ee{wrong}{}",
        server.addr.port(),
        "61".repeat(3)
    );
    let params = mtg_mtsender::ConnectionParams {
        mtproxy: Some(mtg_mtsender::MtProxy::from_link(&link).unwrap()),
        retry_policy: Box::new(mtg_mtsender::NoRetries),
        ..Default::default()
    };
    let pool = SenderPool::with_configuration(server.session(), 1, params);
    tokio::spawn(pool.runner.run());
    assert!(call(&pool.handle).await.is_err());
    assert!(server.proxy_dcs().is_empty());
    pool.handle.quit();
}

fn media_config(port: u16) -> tl::enums::Config {
    let tl::enums::Config::Config(mut config) = mtg_testkit::config();
    config.dc_options = vec![tl::enums::DcOption::Option(tl::types::DcOption {
        ipv6: false,
        media_only: true,
        tcpo_only: false,
        cdn: false,
        r#static: false,
        this_port_only: false,
        id: 2,
        ip_address: "127.0.0.1".into(),
        port: port as i32,
        secret: None,
    })];
    tl::enums::Config::Config(config)
}

#[tokio::test(flavor = "multi_thread")]
async fn media_requests_use_their_own_connection_once_the_config_lists_media_addresses() {
    let server = FakeServer::start().await;
    let port = server.addr.port();
    server.on(move |_: tl::functions::help::GetConfig| Ok(media_config(port)));
    server.on(|_: tl::functions::help::GetNearestDc| {
        Ok(tl::enums::NearestDc::Dc(tl::types::NearestDc {
            country: "xx".into(),
            this_dc: 2,
            nearest_dc: 2,
        }))
    });
    let handle = pool(&server).await;
    let nearest = tl::functions::help::GetNearestDc {};

    // nothing known yet: the media request goes over the ordinary connection
    handle.invoke_media_in_dc(2, &nearest).await.unwrap();
    assert_eq!(server.connections(), 1);
    // the config of that connection listed a media address: the next request opens one more
    handle.invoke_media_in_dc(2, &nearest).await.unwrap();
    assert_eq!(server.connections(), 2);
    // both kinds are reused
    handle.invoke_media_in_dc(2, &nearest).await.unwrap();
    handle.invoke_in_dc(2, &nearest).await.unwrap();
    assert_eq!(server.connections(), 2);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn dead_media_addresses_fall_back_to_the_ordinary_connection() {
    let server = FakeServer::start().await;
    // port 1: nothing listens there
    server.on(|_: tl::functions::help::GetConfig| Ok(media_config(1)));
    server.on(|_: tl::functions::help::GetNearestDc| {
        Ok(tl::enums::NearestDc::Dc(tl::types::NearestDc {
            country: "xx".into(),
            this_dc: 2,
            nearest_dc: 2,
        }))
    });
    let handle = pool(&server).await;
    let nearest = tl::functions::help::GetNearestDc {};
    handle.invoke_media_in_dc(2, &nearest).await.unwrap();
    // the media connect fails, the request still succeeds, and the dead address is not retried
    handle.invoke_media_in_dc(2, &nearest).await.unwrap();
    handle.invoke_media_in_dc(2, &nearest).await.unwrap();
    assert_eq!(server.connections(), 1);
    handle.quit();
}

async fn receiver(
    server: &FakeServer,
) -> (
    mtg_mtsender::SenderPoolFatHandle,
    mtg_mtsender::UpdatesReceiver,
    tokio::sync::mpsc::Sender<mtg_session::updates::UpdatesLike>,
) {
    use mtg_session::SessionData;
    use mtg_session::storages::{MemorySession, erase};
    use mtg_session::types::{DcOption, PeerId, PeerInfo};
    let std::net::SocketAddr::V4(ipv4) = server.addr else {
        unreachable!()
    };
    let mut data = SessionData {
        home_dc: 2,
        ..Default::default()
    };
    data.dc_options.insert(
        2,
        DcOption {
            id: 2,
            ipv4,
            ipv6: "[::1]:1".parse().unwrap(),
            auth_key: Some(mtg_testkit::KEY),
        },
    );
    // MemorySession does not alias the account to `self_user` (SQLite does), so put it there
    data.peer_infos.insert(
        PeerId::self_user(),
        PeerInfo::User {
            id: 1,
            auth: None,
            bot: Some(false),
            is_self: Some(true),
        },
    );
    // a channel whose access hash we know (100) and one we do not (200: nothing stored)
    data.peer_infos.insert(
        PeerId::channel_unchecked(100),
        PeerInfo::Channel {
            id: 100,
            auth: Some(mtg_session::types::PeerAuth::from_hash(55)),
            kind: None,
        },
    );
    let session = std::sync::Arc::new(MemorySession::from(data));
    let pool = SenderPool::new(session.clone(), 1);
    tokio::spawn(pool.runner.run());
    let (updates, feed) = tokio::sync::mpsc::channel(8);
    let receiver = mtg_mtsender::UpdatesReceiver::create(
        pool.handle.thin.clone(),
        erase(session),
        feed,
        Default::default(),
    )
    .await
    .unwrap();
    (pool.handle, receiver, updates)
}

async fn poll(receiver: &mut mtg_mtsender::UpdatesReceiver) {
    // no updates ever arrive: next() runs getState and then waits
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), receiver.next()).await;
}

fn state_count(server: &FakeServer) -> usize {
    server
        .requests()
        .iter()
        .filter(|r| **r == "GetState")
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_get_state_is_asked_again_on_the_next_call() {
    let server = FakeServer::start().await;
    server.on(|_: tl::functions::updates::GetState| Err((500, "INTERNAL".into())));
    let (handle, mut receiver, _feed) = receiver(&server).await;
    poll(&mut receiver).await;
    poll(&mut receiver).await;
    assert!(state_count(&server) >= 2, "{:?}", server.requests());
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_logged_out_account_does_not_repeat_get_state() {
    let server = FakeServer::start().await;
    server.on(|_: tl::functions::updates::GetState| Err((401, "AUTH_KEY_UNREGISTERED".into())));
    let (handle, mut receiver, _feed) = receiver(&server).await;
    poll(&mut receiver).await;
    poll(&mut receiver).await;
    assert_eq!(state_count(&server), 1, "{:?}", server.requests());
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_good_get_state_is_asked_once() {
    let server = FakeServer::start().await;
    server.on(|_: tl::functions::updates::GetState| {
        Ok(tl::enums::updates::State::State(
            tl::types::updates::State {
                pts: 1,
                qts: 0,
                date: 2,
                seq: 3,
                unread_count: 0,
            },
        ))
    });
    let (handle, mut receiver, _feed) = receiver(&server).await;
    poll(&mut receiver).await;
    poll(&mut receiver).await;
    assert_eq!(state_count(&server), 1);
    handle.quit();
}

fn state(pts: i32) -> tl::types::updates::State {
    tl::types::updates::State {
        pts,
        qts: 0,
        date: 2,
        seq: 3,
        unread_count: 0,
    }
}

fn differences(
    server: &FakeServer,
    answers: Vec<tl::enums::updates::Difference>,
) -> std::sync::Arc<std::sync::Mutex<Vec<i32>>> {
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = asked.clone();
    let answers = std::sync::Mutex::new(answers.into_iter());
    server
        .on(|_: tl::functions::updates::GetState| Ok(tl::enums::updates::State::State(state(10))));
    server.on(move |r: tl::functions::updates::GetDifference| {
        seen.lock().unwrap().push(r.pts);
        answers
            .lock()
            .unwrap()
            .next()
            .ok_or((500, "NO_MORE_ANSWERS".to_string()))
    });
    asked
}

fn too_long() -> mtg_session::updates::UpdatesLike {
    mtg_session::updates::UpdatesLike::Updates(tl::enums::Updates::TooLong)
}

fn empty_difference() -> tl::enums::updates::Difference {
    tl::enums::updates::Difference::Empty(tl::types::updates::DifferenceEmpty { date: 5, seq: 3 })
}

#[tokio::test(flavor = "multi_thread")]
async fn updates_too_long_asks_for_the_difference_from_the_known_pts() {
    let server = FakeServer::start().await;
    let asked = differences(&server, vec![empty_difference()]);
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(too_long()).await.unwrap();
    poll(&mut receiver).await;
    assert_eq!(*asked.lock().unwrap(), vec![10]);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_partial_difference_is_followed_by_another_from_its_pts() {
    let server = FakeServer::start().await;
    let slice = tl::enums::updates::Difference::Slice(tl::types::updates::DifferenceSlice {
        new_messages: Vec::new(),
        new_encrypted_messages: Vec::new(),
        other_updates: Vec::new(),
        chats: Vec::new(),
        users: Vec::new(),
        intermediate_state: tl::enums::updates::State::State(state(20)),
    });
    let asked = differences(&server, vec![slice, empty_difference()]);
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(too_long()).await.unwrap();
    // next() hands out the (empty) first batch, then goes on with the rest
    poll(&mut receiver).await;
    poll(&mut receiver).await;
    assert_eq!(*asked.lock().unwrap(), vec![10, 20]);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_difference_ends_next_with_the_error() {
    let server = FakeServer::start().await;
    let _ = differences(&server, vec![]);
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(too_long()).await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.next())
        .await
        .unwrap();
    assert!(result.is_err());
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_difference_that_fails_with_a_server_error_is_retried() {
    let server = FakeServer::start().await;
    server
        .on(|_: tl::functions::updates::GetState| Ok(tl::enums::updates::State::State(state(10))));
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    server.on(move |_: tl::functions::updates::GetDifference| {
        match seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            0 => Err((500, "INTERNAL".into())),
            1 => Err((400, "PERSISTENT_TIMESTAMP_OUTDATED".into())),
            _ => Ok(empty_difference()),
        }
    });
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(too_long()).await.unwrap();
    // 200 ms + 400 ms of waiting between the three calls
    assert!(receiver.next().await.is_ok());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_difference_that_keeps_failing_gives_up_after_the_retries() {
    let server = FakeServer::start().await;
    server
        .on(|_: tl::functions::updates::GetState| Ok(tl::enums::updates::State::State(state(10))));
    server.on(|_: tl::functions::updates::GetDifference| Err((500, "INTERNAL".into())));
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(too_long()).await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), receiver.next())
        .await
        .unwrap();
    assert!(result.is_err());
    let asked = server
        .requests()
        .iter()
        .filter(|r| **r == "GetDifference")
        .count();
    assert_eq!(asked, 4);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn other_difference_errors_are_not_retried() {
    let server = FakeServer::start().await;
    server
        .on(|_: tl::functions::updates::GetState| Ok(tl::enums::updates::State::State(state(10))));
    server.on(|_: tl::functions::updates::GetDifference| Err((400, "DATE_EMPTY".into())));
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(too_long()).await.unwrap();
    assert!(receiver.next().await.is_err());
    let asked = server
        .requests()
        .iter()
        .filter(|r| **r == "GetDifference")
        .count();
    assert_eq!(asked, 1);
    handle.quit();
}

fn cdn_config(port: u16) -> tl::enums::Config {
    let tl::enums::Config::Config(mut config) = mtg_testkit::config();
    config.dc_options = vec![tl::enums::DcOption::Option(tl::types::DcOption {
        ipv6: false,
        media_only: false,
        tcpo_only: false,
        cdn: true,
        r#static: false,
        this_port_only: false,
        id: 203,
        ip_address: "127.0.0.1".into(),
        port: port as i32,
        secret: None,
    })];
    tl::enums::Config::Config(config)
}

fn get_cdn_file() -> tl::functions::upload::GetCdnFile {
    tl::functions::upload::GetCdnFile {
        file_token: vec![1],
        offset: 0,
        limit: 4096,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cdn_dc_gets_initconnection_with_its_first_request_only() {
    use mtg_session::Session;
    use mtg_session::types::DcOption;
    let server = FakeServer::start().await;
    let port = server.addr.port();
    server.on(move |_: tl::functions::help::GetConfig| Ok(cdn_config(port)));
    server.on(|_: tl::functions::upload::GetCdnFile| {
        Ok(tl::enums::upload::CdnFile::File(
            tl::types::upload::CdnFile {
                bytes: vec![7; 4096],
            },
        ))
    });
    let session = server.session();
    let mtg_session::types::DcOption { ipv4, ipv6, .. } = session.dc_option(2).unwrap().unwrap();
    session
        .set_dc_option(&DcOption {
            id: 203,
            ipv4,
            ipv6,
            auth_key: Some(mtg_testkit::KEY),
        })
        .await
        .unwrap();
    let pool = SenderPool::new(session, 1);
    tokio::spawn(pool.runner.run());
    let handle = pool.handle;

    // the first connection brings the config that lists DC 203 as a CDN one
    handle
        .invoke_in_dc(2, &tl::functions::help::GetConfig {})
        .await
        .unwrap();
    assert_eq!((server.connections(), server.layered_requests()), (1, 1));

    let tl::enums::upload::CdnFile::File(file) =
        handle.invoke_in_dc(203, &get_cdn_file()).await.unwrap()
    else {
        panic!("not a file")
    };
    assert_eq!(file.bytes.len(), 4096);
    // a new connection, and its first request carried initConnection
    assert_eq!((server.connections(), server.layered_requests()), (2, 2));
    handle.invoke_in_dc(203, &get_cdn_file()).await.unwrap();
    assert_eq!((server.connections(), server.layered_requests()), (2, 2));
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cdn_dc_without_a_key_or_a_public_key_is_not_connected() {
    let server = FakeServer::start().await;
    let port = server.addr.port();
    server.on(move |_: tl::functions::help::GetConfig| Ok(cdn_config(port)));
    let handle = pool(&server).await;
    handle
        .invoke_in_dc(2, &tl::functions::help::GetConfig {})
        .await
        .unwrap();
    // no auth key for DC 203 and no key from getCdnConfig to trust: no handshake is attempted
    let error = handle.invoke_in_dc(203, &get_cdn_file()).await.unwrap_err();
    assert!(
        matches!(error, mtg_mtsender::InvocationError::Dropped(m) if m.contains("CDN public key")),
        "{error:?}"
    );
    assert_eq!(server.connections(), 1);
    // keys that are not PEM are skipped
    assert!(handle.set_cdn_keys(&tl::types::CdnConfig {
        public_keys: vec![tl::enums::CdnPublicKey::Key(tl::types::CdnPublicKey {
            dc_id: 203,
            public_key: "garbage".into(),
        })],
    }));
    assert!(handle.invoke_in_dc(203, &get_cdn_file()).await.is_err());
    handle.quit();
}

fn channel_too_long(channel_id: i64, pts: i32) -> mtg_session::updates::UpdatesLike {
    mtg_session::updates::UpdatesLike::Updates(tl::enums::Updates::Updates(tl::types::Updates {
        updates: vec![
            tl::types::UpdateChannelTooLong {
                channel_id,
                pts: Some(pts),
            }
            .into(),
        ],
        users: Vec::new(),
        chats: Vec::new(),
        date: 0,
        seq: 0,
    }))
}

fn channel_difference(r#final: bool, pts: i32) -> tl::enums::updates::ChannelDifference {
    tl::enums::updates::ChannelDifference::Empty(tl::types::updates::ChannelDifferenceEmpty {
        r#final,
        pts,
        timeout: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_channel_difference_that_is_not_final_is_asked_again_from_its_pts() {
    let server = FakeServer::start().await;
    server
        .on(|_: tl::functions::updates::GetState| Ok(tl::enums::updates::State::State(state(10))));
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = asked.clone();
    let answers = std::sync::Mutex::new(
        vec![channel_difference(false, 30), channel_difference(true, 40)].into_iter(),
    );
    server.on(move |r: tl::functions::updates::GetChannelDifference| {
        let tl::enums::InputChannel::Channel(c) = &r.channel else {
            panic!("not a channel")
        };
        seen.lock()
            .unwrap()
            .push((c.channel_id, c.access_hash, r.pts));
        Ok(answers
            .lock()
            .unwrap()
            .next()
            .unwrap_or_else(|| channel_difference(true, 40)))
    });
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(channel_too_long(100, 20)).await.unwrap();
    for _ in 0..4 {
        poll(&mut receiver).await;
    }
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert_eq!(asked[0].1, 55);
    // the second request continues from the pts of the first answer
    assert_eq!(asked[1].2, 30);
    handle.quit();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_channel_without_a_stored_hash_is_dropped_without_a_request() {
    let server = FakeServer::start().await;
    server
        .on(|_: tl::functions::updates::GetState| Ok(tl::enums::updates::State::State(state(10))));
    let (handle, mut receiver, feed) = receiver(&server).await;
    feed.send(channel_too_long(200, 20)).await.unwrap();
    for _ in 0..2 {
        poll(&mut receiver).await;
    }
    assert!(
        !server.requests().contains(&"GetChannelDifference"),
        "{:?}",
        server.requests()
    );
    handle.quit();
}
