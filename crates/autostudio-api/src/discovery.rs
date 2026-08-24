use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::constants::{MINIMUM_SESSION_TOKEN_BYTES, PROTOCOL_VERSION};
pub use crate::error::DiscoveryError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

pub struct DiscoveryFile {
    path: PathBuf,
}

impl DiscoveryFile {
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Atomically publishes a private Core discovery record.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] when the parent directory, temporary file,
    /// serialization, permission update, or atomic rename fails.
    pub fn publish(&self, record: &DiscoveryRecord) -> Result<(), DiscoveryError> {
        record.validate()?;
        let parent = self.path.parent().ok_or(DiscoveryError::MissingParent)?;
        fs::create_dir_all(parent)?;

        self.with_exclusive_lock(|| self.publish_locked(record))
    }

    fn publish_locked(&self, record: &DiscoveryRecord) -> Result<(), DiscoveryError> {
        let temporary_path = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut temporary = options.open(&temporary_path)?;
        let bytes = serde_json::to_vec(record)?;
        temporary.write_all(&bytes)?;
        temporary.sync_all()?;
        set_private_permissions(&temporary_path)?;
        drop(temporary);
        fs::rename(&temporary_path, &self.path)?;
        set_private_permissions(&self.path)?;
        Ok(())
    }

    /// Reads and validates the current Core discovery record.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] when the file is inaccessible, too broadly
    /// permissioned, malformed, or violates the discovery contract.
    pub fn read(&self) -> Result<DiscoveryRecord, DiscoveryError> {
        ensure_private_permissions(&self.path)?;
        let mut file = OpenOptions::new().read(true).open(&self.path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let record: DiscoveryRecord = serde_json::from_slice(&bytes)?;
        record.validate()?;
        Ok(record)
    }

    /// Removes the discovery record only when it still belongs to the expected Core.
    ///
    /// Publication and removal share an exclusive lock, preventing an older Core from
    /// deleting a record that a newer Core published concurrently.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] when locking, reading, or removal fails.
    pub fn remove_if_owner(&self, core_instance_id: &str) -> Result<bool, DiscoveryError> {
        let parent = self.path.parent().ok_or(DiscoveryError::MissingParent)?;
        fs::create_dir_all(parent)?;
        self.with_exclusive_lock(|| {
            if !self.path.exists() {
                return Ok(false);
            }
            let record = self.read()?;
            if record.core_instance_id() != core_instance_id {
                return Ok(false);
            }
            fs::remove_file(&self.path)?;
            Ok(true)
        })
    }

    fn with_exclusive_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, DiscoveryError>,
    ) -> Result<T, DiscoveryError> {
        let lock_path = self.path.with_extension("lock");
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(&lock_path)?;
        set_private_permissions(&lock_path)?;
        lock.lock_exclusive()?;
        let result = operation();
        lock.unlock()?;
        result
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryRecord {
    core_instance_id: String,
    core_pid: u32,
    endpoint: String,
    protocol_version: String,
    session_token: String,
}

impl DiscoveryRecord {
    #[must_use]
    pub fn new(
        core_instance_id: impl Into<String>,
        core_pid: u32,
        endpoint: impl Into<String>,
        session_token: impl Into<String>,
    ) -> Self {
        Self {
            core_instance_id: core_instance_id.into(),
            core_pid,
            endpoint: endpoint.into(),
            protocol_version: PROTOCOL_VERSION.to_owned(),
            session_token: session_token.into(),
        }
    }

    #[must_use]
    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
    }

    #[must_use]
    pub const fn core_pid(&self) -> u32 {
        self.core_pid
    }

    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    #[must_use]
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    #[must_use]
    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    fn validate(&self) -> Result<(), DiscoveryError> {
        if self.core_instance_id.trim().is_empty()
            || self.core_pid == 0
            || self.endpoint.trim().is_empty()
            || self.protocol_version.trim().is_empty()
            || self.session_token.len() < MINIMUM_SESSION_TOKEN_BYTES
        {
            return Err(DiscoveryError::InvalidRecord);
        }
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), DiscoveryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), DiscoveryError> {
    Ok(())
}

#[cfg(unix)]
fn ensure_private_permissions(path: &Path) -> Result<(), DiscoveryError> {
    use std::os::unix::fs::PermissionsExt;
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(DiscoveryError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_permissions(_path: &Path) -> Result<(), DiscoveryError> {
    Ok(())
}
