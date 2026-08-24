use std::sync::Arc;

use autostudio_api::discovery::{DiscoveryFile, DiscoveryRecord};
use autostudio_api::{PROTOCOL_VERSION, router};
use autostudio_core::project::ProjectService;
use autostudio_desktop::core_client::CoreClient;

#[tokio::test]
async fn desktop_connects_and_manages_the_current_project_without_exposing_the_token() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store =
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("night-drive.autostudio"))
            .expect("open project store");
    let token = "ship-zero-session-token-with-at-least-32-bytes";
    let app = router(Arc::new(ProjectService::new(Arc::new(store))), token);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let endpoint = format!(
        "http://{}",
        listener.local_addr().expect("listener address")
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Core server");
    });

    let discovery_path = temp.path().join("runtime/core.json");
    DiscoveryFile::new(&discovery_path)
        .publish(&DiscoveryRecord::new(
            "core-instance-1",
            std::process::id(),
            &endpoint,
            token,
        ))
        .expect("publish discovery record");

    let client = CoreClient::new(&discovery_path);
    let status = client.status().await.expect("connect to Core");
    assert_eq!(status.core_instance_id, "core-instance-1");
    assert_eq!(status.protocol_version, PROTOCOL_VERSION);

    let created = client
        .create_project("Night Drive")
        .await
        .expect("create project");
    assert_eq!(created.name, "Night Drive");
    assert_eq!(created.revision, 0);

    let reopened = client.open_project().await.expect("reopen project");
    assert_eq!(reopened, created);

    server.abort();
}
