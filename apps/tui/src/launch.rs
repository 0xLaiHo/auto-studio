use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::client::TuiClient;
use crate::constants::{
    CORE_BINARY_NAME, CORE_LOOPBACK_BIND, CORE_START_POLL_INTERVAL, CORE_START_TIMEOUT,
    DEFAULT_CORE_LOG_FILE, DEFAULT_DISCOVERY_FILE, DEFAULT_HOME_DIRECTORY,
    DEFAULT_LLM_CONNECTION_FILE, DEFAULT_PROJECT_DIRECTORY, DEFAULT_RUNTIME_DIRECTORY,
    ENV_AUTOSTUDIO_HOME, ENV_BIND, ENV_CORE_BINARY, ENV_DISCOVERY_FILE, ENV_LLM_CONNECTION_FILE,
    ENV_PARENT_HEARTBEAT, ENV_PROJECT_PACKAGE, HEARTBEAT_INTERVAL,
};
use crate::error::TuiError;

pub struct CoreSession {
    client: TuiClient,
    _managed_core: Option<ManagedCore>,
}

impl CoreSession {
    pub async fn connect_or_launch() -> Result<Self, TuiError> {
        let paths = LaunchPaths::resolve()?;
        paths.prepare()?;
        let client = TuiClient::new(paths.discovery.clone());
        if client.health().await.is_ok() {
            return Ok(Self {
                client,
                _managed_core: None,
            });
        }

        let mut managed_core = ManagedCore::launch(&paths)?;
        let deadline = tokio::time::Instant::now() + CORE_START_TIMEOUT;
        loop {
            if client.health().await.is_ok() {
                return Ok(Self {
                    client,
                    _managed_core: Some(managed_core),
                });
            }
            if let Some(status) = managed_core.try_wait()? {
                if client.health().await.is_ok() {
                    return Ok(Self {
                        client,
                        _managed_core: None,
                    });
                }
                return Err(TuiError::CoreExited {
                    status: status.code(),
                    log: paths.core_log.clone(),
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(TuiError::CoreStartTimeout {
                    log: paths.core_log.clone(),
                });
            }
            tokio::time::sleep(CORE_START_POLL_INTERVAL).await;
        }
    }

    #[must_use]
    pub const fn client(&self) -> &TuiClient {
        &self.client
    }
}

struct LaunchPaths {
    project_package: PathBuf,
    discovery: PathBuf,
    runtime_root: PathBuf,
    llm_connection: PathBuf,
    core_log: PathBuf,
}

impl LaunchPaths {
    fn resolve() -> Result<Self, TuiError> {
        let home = app_home()?;
        let project_package = env::var_os(ENV_PROJECT_PACKAGE)
            .map_or_else(|| home.join(DEFAULT_PROJECT_DIRECTORY), PathBuf::from);
        let discovery = env::var_os(ENV_DISCOVERY_FILE)
            .map_or_else(|| home.join(DEFAULT_DISCOVERY_FILE), PathBuf::from);
        let runtime_root = discovery
            .parent()
            .map_or_else(|| home.join(DEFAULT_RUNTIME_DIRECTORY), Path::to_path_buf);
        let llm_connection = env::var_os(ENV_LLM_CONNECTION_FILE)
            .map_or_else(|| home.join(DEFAULT_LLM_CONNECTION_FILE), PathBuf::from);
        let core_log = home.join(DEFAULT_CORE_LOG_FILE);
        Ok(Self {
            project_package,
            discovery,
            runtime_root,
            llm_connection,
            core_log,
        })
    }

    #[cfg(test)]
    fn from_home(home: &Path) -> Self {
        Self {
            project_package: home.join(DEFAULT_PROJECT_DIRECTORY),
            discovery: home.join(DEFAULT_DISCOVERY_FILE),
            runtime_root: home.join(DEFAULT_RUNTIME_DIRECTORY),
            llm_connection: home.join(DEFAULT_LLM_CONNECTION_FILE),
            core_log: home.join(DEFAULT_CORE_LOG_FILE),
        }
    }

    fn prepare(&self) -> Result<(), TuiError> {
        fs::create_dir_all(&self.project_package)?;
        fs::create_dir_all(&self.runtime_root)?;
        if let Some(parent) = self.llm_connection.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

fn app_home() -> Result<PathBuf, TuiError> {
    if let Some(path) = env::var_os(ENV_AUTOSTUDIO_HOME) {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join(DEFAULT_HOME_DIRECTORY));
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = env::var_os("HOME") {
        return Ok(PathBuf::from(path)
            .join("Library/Application Support")
            .join(DEFAULT_HOME_DIRECTORY));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(path).join(DEFAULT_HOME_DIRECTORY));
        }
        if let Some(path) = env::var_os("HOME") {
            return Ok(PathBuf::from(path)
                .join(".local/share")
                .join(DEFAULT_HOME_DIRECTORY));
        }
    }
    Err(TuiError::HomeUnavailable)
}

struct ManagedCore {
    child: Mutex<Child>,
    _heartbeat: HeartbeatLease,
}

impl ManagedCore {
    fn launch(paths: &LaunchPaths) -> Result<Self, TuiError> {
        let heartbeat_path = paths
            .runtime_root
            .join(format!("tui-heartbeat-{}", std::process::id()));
        let heartbeat = HeartbeatLease::start(heartbeat_path.clone())?;
        let binary = core_binary()?;
        let core_log = private_log_file(&paths.core_log)?;
        let child = Command::new(&binary)
            .env(ENV_PROJECT_PACKAGE, &paths.project_package)
            .env(ENV_DISCOVERY_FILE, &paths.discovery)
            .env(ENV_LLM_CONNECTION_FILE, &paths.llm_connection)
            .env(ENV_BIND, CORE_LOOPBACK_BIND)
            .env(ENV_PARENT_HEARTBEAT, heartbeat_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(core_log))
            .spawn()
            .map_err(|source| TuiError::CoreSpawn {
                binary: PathBuf::from(&binary),
                source,
            })?;
        Ok(Self {
            child: Mutex::new(child),
            _heartbeat: heartbeat,
        })
    }

    fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>, TuiError> {
        self.child
            .lock()
            .map_err(|_| std::io::Error::other("Core child lock is poisoned"))?
            .try_wait()
            .map_err(Into::into)
    }
}

impl Drop for ManagedCore {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock()
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn core_binary() -> Result<OsString, TuiError> {
    if let Some(binary) = env::var_os(ENV_CORE_BINARY) {
        return Ok(binary);
    }
    let executable = env::current_exe()?;
    let parent = executable
        .parent()
        .ok_or(TuiError::CoreBinaryWithoutParent)?;
    let sibling = parent.join(CORE_BINARY_NAME);
    if sibling.is_file() {
        Ok(sibling.into_os_string())
    } else {
        Ok(OsString::from(CORE_BINARY_NAME))
    }
}

struct HeartbeatLease {
    path: PathBuf,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl HeartbeatLease {
    fn start(path: PathBuf) -> Result<Self, TuiError> {
        touch_heartbeat(&path)?;
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let worker_path = path.clone();
        let worker = thread::Builder::new()
            .name("autostudio-tui-heartbeat".to_owned())
            .spawn(move || {
                while worker_running.load(Ordering::Acquire) {
                    let _ = touch_heartbeat(&worker_path);
                    thread::sleep(HEARTBEAT_INTERVAL);
                }
            })?;
        Ok(Self {
            path,
            running,
            worker: Some(worker),
        })
    }
}

impl Drop for HeartbeatLease {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

fn touch_heartbeat(path: &Path) -> Result<(), TuiError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    writeln!(file, "{}", std::process::id())?;
    file.flush()?;
    Ok(())
}

fn private_log_file(path: &Path) -> Result<std::fs::File, TuiError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::LaunchPaths;

    #[test]
    fn default_home_layout_keeps_credentials_outside_the_project_package() {
        let root = tempdir().expect("temporary home");
        let paths = LaunchPaths::from_home(root.path());
        assert!(!paths.llm_connection.starts_with(&paths.project_package));
        assert!(paths.discovery.starts_with(root.path()));
        assert!(paths.core_log.starts_with(&paths.runtime_root));
        paths.prepare().expect("prepared launch paths");
        assert!(paths.project_package.is_dir());
        assert!(paths.runtime_root.is_dir());
    }
}
