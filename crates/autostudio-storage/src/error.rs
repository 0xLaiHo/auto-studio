use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectBackupError {
    #[error("backup filesystem operation failed: {0}")]
    Io(std::io::Error),
    #[error("backup SQLite snapshot failed: {0}")]
    Database(rusqlite::Error),
    #[error("backup contains an unsafe non-regular entry at {0}")]
    UnsafeEntry(PathBuf),
    #[error("backup SQLite integrity check failed: {0}")]
    Integrity(String),
    #[error("backup Project validation failed: {0}")]
    Project(String),
}

#[derive(Debug, Error)]
pub enum ProjectPackageError {
    #[error("failed to create Project Package directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Project Package is already open by {owner:?}")]
    AlreadyOpen { owner: Option<String> },
    #[error("failed to acquire Project Package lock {path}: {source}")]
    LockFile { path: PathBuf, source: io::Error },
    #[error("failed to open Project database {path}: {source}")]
    OpenDatabase {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("failed to migrate Project database: {0}")]
    Migrate(rusqlite::Error),
    #[error("failed to start Project DB actor: {0}")]
    StartActor(std::io::Error),
}
