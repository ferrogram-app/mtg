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
