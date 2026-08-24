use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn managed_core_exits_after_the_desktop_heartbeat_disappears() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("managed.autostudio");
    let discovery = temp.path().join("runtime/core.json");
    let heartbeat = temp.path().join("runtime/desktop-heartbeat");
    fs::create_dir_all(heartbeat.parent().expect("heartbeat parent")).expect("runtime directory");
    fs::write(&heartbeat, b"test-desktop\n").expect("heartbeat");

    let mut core = Command::new(env!("CARGO_BIN_EXE_core-daemon"))
        .env("AUTOSTUDIO_PROJECT_PACKAGE", &package)
        .env("AUTOSTUDIO_DISCOVERY_FILE", &discovery)
        .env("AUTOSTUDIO_BIND", "127.0.0.1:0")
        .env("AUTOSTUDIO_PARENT_HEARTBEAT", &heartbeat)
        .env("DEEPSEEK_API_KEY", "contract-test-key-not-sent")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start Core");

    let startup_deadline = Instant::now() + Duration::from_secs(5);
    while !discovery.is_file() && Instant::now() < startup_deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(discovery.is_file(), "Core did not publish discovery");
    fs::remove_file(&heartbeat).expect("remove heartbeat");

    let shutdown_deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < shutdown_deadline {
        if core.try_wait().expect("query Core").is_some() {
            assert!(!discovery.exists(), "Core must clean its discovery record");
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = core.kill();
    let _ = core.wait();
    panic!("Core remained alive without its Desktop heartbeat");
}
