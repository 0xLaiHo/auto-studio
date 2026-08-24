use crate::constants::{
    CORE_LOOPBACK_BIND, ENV_BIND, ENV_CORE_BINARY, ENV_DISCOVERY_FILE, ENV_PARENT_HEARTBEAT,
    ENV_PROJECT_PACKAGE, HEARTBEAT_INTERVAL,
};
pub use crate::error::ManagedCoreError;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub struct ManagedCore {
    child: Mutex<Child>,
    heartbeat: HeartbeatLease,
}

impl ManagedCore {
    /// Starts a private Core child process and a crash-detectable parent heartbeat.
    ///
    /// # Errors
    ///
    /// Returns [`ManagedCoreError`] when paths cannot be prepared, the Core binary
    /// cannot be resolved or spawned, or the heartbeat cannot be created.
    pub fn launch(project_package: &Path, discovery_path: &Path) -> Result<Self, ManagedCoreError> {
        fs::create_dir_all(project_package).map_err(ManagedCoreError::Io)?;
        let runtime_root = discovery_path
            .parent()
            .ok_or(ManagedCoreError::DiscoveryWithoutParent)?;
        fs::create_dir_all(runtime_root).map_err(ManagedCoreError::Io)?;
        let heartbeat_path = runtime_root.join(format!("desktop-heartbeat-{}", std::process::id()));
        let heartbeat = HeartbeatLease::start(heartbeat_path.clone())?;
        let binary = core_binary()?;
        let child = Command::new(&binary)
            .env(ENV_PROJECT_PACKAGE, project_package)
            .env(ENV_DISCOVERY_FILE, discovery_path)
            .env(ENV_BIND, CORE_LOOPBACK_BIND)
            .env(ENV_PARENT_HEARTBEAT, heartbeat_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| ManagedCoreError::Spawn { binary, source })?;
        Ok(Self {
            child: Mutex::new(child),
            heartbeat,
        })
    }
}

impl Drop for ManagedCore {
    fn drop(&mut self) {
        self.heartbeat.stop();
        if let Ok(mut child) = self.child.lock()
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn core_binary() -> Result<PathBuf, ManagedCoreError> {
    if let Some(path) = std::env::var_os(ENV_CORE_BINARY).map(PathBuf::from) {
        return validate_core_binary(path);
    }
    let executable = std::env::current_exe().map_err(ManagedCoreError::Io)?;
    let parent = executable
        .parent()
        .ok_or(ManagedCoreError::BinaryWithoutParent)?;
    let name = if cfg!(windows) {
        "core-daemon.exe"
    } else {
        "core-daemon"
    };
    validate_core_binary(parent.join(name))
}

fn validate_core_binary(path: PathBuf) -> Result<PathBuf, ManagedCoreError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(ManagedCoreError::BinaryNotFound(path))
    }
}

struct HeartbeatLease {
    path: PathBuf,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl HeartbeatLease {
    fn start(path: PathBuf) -> Result<Self, ManagedCoreError> {
        touch_heartbeat(&path)?;
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let worker_path = path.clone();
        let worker = thread::Builder::new()
            .name("autostudio-desktop-heartbeat".to_owned())
            .spawn(move || {
                while worker_running.load(Ordering::Acquire) {
                    let _ = touch_heartbeat(&worker_path);
                    thread::sleep(HEARTBEAT_INTERVAL);
                }
            })
            .map_err(ManagedCoreError::Io)?;
        Ok(Self {
            path,
            running,
            worker: Some(worker),
        })
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

fn touch_heartbeat(path: &Path) -> Result<(), ManagedCoreError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(ManagedCoreError::Io)?;
    writeln!(file, "{}", std::process::id()).map_err(ManagedCoreError::Io)?;
    file.flush().map_err(ManagedCoreError::Io)
}
