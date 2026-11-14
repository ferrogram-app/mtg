//! The fake server end to end with the real sender pool.

use mtg_mtsender::{InvocationError, SenderPool};
use mtg_testkit::FakeServer;
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
