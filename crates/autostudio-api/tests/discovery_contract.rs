use std::fs;

use autostudio_api::discovery::{DiscoveryFile, DiscoveryRecord};

#[test]
fn desktop_can_read_a_private_core_discovery_record() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("runtime").join("core.json");
    let discovery = DiscoveryFile::new(&path);
    let record = DiscoveryRecord::new(
        "core-instance-123",
        4242,
        "http://127.0.0.1:47321",
        "ship-zero-test-session-token-123456",
    );

    discovery.publish(&record).expect("publish discovery");
    let loaded = discovery.read().expect("read discovery");

    assert_eq!(loaded.core_instance_id(), "core-instance-123");
    assert_eq!(loaded.core_pid(), 4242);
    assert_eq!(loaded.endpoint(), "http://127.0.0.1:47321");
    assert_eq!(
        loaded.session_token(),
        "ship-zero-test-session-token-123456"
    );
    assert_eq!(loaded.protocol_version(), "0.3.0");

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("discovery bytes")).expect("discovery JSON");
    assert_eq!(json["coreInstanceId"], "core-instance-123");
    assert_eq!(json["protocolVersion"], "0.3.0");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path)
            .expect("discovery metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn stopping_core_only_removes_its_own_discovery_record() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("core.json");
    let discovery = DiscoveryFile::new(&path);
    let record = DiscoveryRecord::new(
        "new-core-instance",
        4242,
        "http://127.0.0.1:47321",
        "ship-zero-test-session-token-123456",
    );
    discovery.publish(&record).expect("publish discovery");

    assert!(
        !discovery
            .remove_if_owner("old-core-instance")
            .expect("keep newer record")
    );
    assert!(path.exists());
    assert!(
        discovery
            .remove_if_owner("new-core-instance")
            .expect("remove owned record")
    );
    assert!(!path.exists());
}
