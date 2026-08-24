//! `SQLite` and Project Package implementation.

pub mod constants;
mod error;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use autostudio_core::project::{
    Project, ProjectBackupDraft, ProjectBackupSink, ProjectEvent, ProjectEventEnvelope,
    ProjectStore, ProjectStoreError,
};
use fs2::FileExt;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};
use uuid::Uuid;

use crate::constants::ACTOR_QUEUE_CAPACITY;
pub use crate::error::{ProjectBackupError, ProjectPackageError};

pub struct SqliteProjectStore {
    sender: Option<SyncSender<Command>>,
    worker: Option<JoinHandle<()>>,
    package_lock: File,
}

impl SqliteProjectStore {
    /// Opens or initializes the `SQLite` actor for one Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectPackageError`] when the package directory, database,
    /// migration, or actor thread cannot be initialized.
    pub fn open(package_root: &Path) -> Result<Self, ProjectPackageError> {
        Self::open_with_owner(package_root, &format!("process-{}", std::process::id()))
    }

    /// Opens a Project Package for one named Core Instance.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectPackageError::AlreadyOpen`] while another live process owns
    /// the package's OS lock. A stale owner file without a live lock is overwritten.
    pub fn open_with_owner(package_root: &Path, owner: &str) -> Result<Self, ProjectPackageError> {
        fs::create_dir_all(package_root).map_err(|source| {
            ProjectPackageError::CreateDirectory {
                path: package_root.to_path_buf(),
                source,
            }
        })?;

        let package_lock = acquire_package_lock(package_root, owner)?;

        let database_path = package_root.join("project.db");
        let connection = Connection::open(&database_path).map_err(|source| {
            ProjectPackageError::OpenDatabase {
                path: database_path,
                source,
            }
        })?;
        migrate(&connection)?;

        let (sender, receiver) = mpsc::sync_channel(ACTOR_QUEUE_CAPACITY);
        let worker = thread::Builder::new()
            .name("autostudio-project-db".to_owned())
            .spawn(move || run_actor(&connection, &receiver))
            .map_err(ProjectPackageError::StartActor)?;

        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            package_lock,
        })
    }

    fn sender(&self) -> Result<&SyncSender<Command>, ProjectStoreError> {
        self.sender.as_ref().ok_or_else(|| {
            ProjectStoreError::Unavailable("project DB actor has stopped".to_owned())
        })
    }
}

pub struct ProjectPackageBackup {
    package_root: PathBuf,
    backup_root: PathBuf,
}

impl ProjectPackageBackup {
    /// Constrains backup reads and publications to canonical package roots.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectBackupError`] when either root cannot be prepared safely.
    pub fn new(package_root: &Path, backup_root: &Path) -> Result<Self, ProjectBackupError> {
        fs::create_dir_all(package_root).map_err(ProjectBackupError::Io)?;
        fs::create_dir_all(backup_root).map_err(ProjectBackupError::Io)?;
        Ok(Self {
            package_root: package_root
                .canonicalize()
                .map_err(ProjectBackupError::Io)?,
            backup_root: backup_root.canonicalize().map_err(ProjectBackupError::Io)?,
        })
    }

    /// Publishes a standalone `SQLite` snapshot plus immutable assets and exports.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectBackupError`] for unsafe files, `SQLite` snapshot failure,
    /// validation failure, copying, syncing, or atomic publication failure.
    pub fn backup(&self, project: &Project) -> Result<ProjectBackupDraft, ProjectBackupError> {
        let backup_name = format!(
            "project-{}-r{}-{}.autostudio",
            project.id().as_str(),
            project.revision(),
            Uuid::new_v4().simple()
        );
        let destination = self.backup_root.join(&backup_name);
        let temporary = self.backup_root.join(format!(".{backup_name}.partial"));
        let result = (|| {
            fs::create_dir(&temporary).map_err(ProjectBackupError::Io)?;
            let source_database = Connection::open(self.package_root.join("project.db"))
                .map_err(ProjectBackupError::Database)?;
            let target_database = temporary.join("project.db");
            source_database
                .execute(
                    "VACUUM main INTO ?1",
                    [target_database.to_string_lossy().as_ref()],
                )
                .map_err(ProjectBackupError::Database)?;

            for directory in ["assets", "exports"] {
                let source = self.package_root.join(directory);
                if source.exists() {
                    copy_tree_without_links(&source, &temporary.join(directory))?;
                }
            }

            let backup_database =
                Connection::open(&target_database).map_err(ProjectBackupError::Database)?;
            let integrity: String = backup_database
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .map_err(ProjectBackupError::Database)?;
            if integrity != "ok" {
                return Err(ProjectBackupError::Integrity(integrity));
            }
            let restored = select_project(&backup_database)
                .map_err(|error| ProjectBackupError::Project(error.to_string()))?;
            if &restored != project {
                return Err(ProjectBackupError::Project(
                    "backup snapshot differs from requested Project revision".to_owned(),
                ));
            }
            drop(backup_database);
            sync_directory(&temporary)?;
            fs::rename(&temporary, &destination).map_err(ProjectBackupError::Io)?;
            sync_directory(&self.backup_root)?;
            Ok(())
        })();
        if result.is_err() && temporary.exists() {
            let _ = fs::remove_dir_all(&temporary);
        }
        result?;
        Ok(ProjectBackupDraft { backup_name })
    }
}

impl ProjectBackupSink for ProjectPackageBackup {
    fn backup(&self, project: &Project) -> Result<ProjectBackupDraft, String> {
        self.backup(project).map_err(|error| error.to_string())
    }
}

fn copy_tree_without_links(source: &Path, destination: &Path) -> Result<(), ProjectBackupError> {
    let metadata = fs::symlink_metadata(source).map_err(ProjectBackupError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ProjectBackupError::UnsafeEntry(source.to_path_buf()));
    }
    fs::create_dir(destination).map_err(ProjectBackupError::Io)?;
    for entry in fs::read_dir(source).map_err(ProjectBackupError::Io)? {
        let entry = entry.map_err(ProjectBackupError::Io)?;
        let metadata = entry.metadata().map_err(ProjectBackupError::Io)?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(ProjectBackupError::Io)?
            .is_symlink()
        {
            return Err(ProjectBackupError::UnsafeEntry(source_path));
        }
        if metadata.is_dir() {
            copy_tree_without_links(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path).map_err(ProjectBackupError::Io)?;
            File::open(&destination_path)
                .and_then(|file| file.sync_all())
                .map_err(ProjectBackupError::Io)?;
        } else {
            return Err(ProjectBackupError::UnsafeEntry(source_path));
        }
    }
    sync_directory(destination)
}

fn sync_directory(path: &Path) -> Result<(), ProjectBackupError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(ProjectBackupError::Io)
}

impl ProjectStore for SqliteProjectStore {
    fn create(&self, project: &Project) -> Result<(), ProjectStoreError> {
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        self.sender()?
            .send(Command::Create {
                project: project.clone(),
                response: response_sender,
            })
            .map_err(|_| actor_stopped())?;
        response_receiver.recv().map_err(|_| actor_stopped())?
    }

    fn open(&self) -> Result<Project, ProjectStoreError> {
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        self.sender()?
            .send(Command::Open {
                response: response_sender,
            })
            .map_err(|_| actor_stopped())?;
        response_receiver.recv().map_err(|_| actor_stopped())?
    }

    fn commit(
        &self,
        expected_revision: u64,
        project: &Project,
        event: &ProjectEvent,
    ) -> Result<(), ProjectStoreError> {
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        self.sender()?
            .send(Command::Commit {
                expected_revision,
                project: project.clone(),
                event: Box::new(event.clone()),
                response: response_sender,
            })
            .map_err(|_| actor_stopped())?;
        response_receiver.recv().map_err(|_| actor_stopped())?
    }

    fn events_after(
        &self,
        after_sequence: u64,
    ) -> Result<Vec<ProjectEventEnvelope>, ProjectStoreError> {
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        self.sender()?
            .send(Command::EventsAfter {
                after_sequence,
                response: response_sender,
            })
            .map_err(|_| actor_stopped())?;
        response_receiver.recv().map_err(|_| actor_stopped())?
    }
}

impl Drop for SqliteProjectStore {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = FileExt::unlock(&self.package_lock);
    }
}

enum Command {
    Create {
        project: Project,
        response: SyncSender<Result<(), ProjectStoreError>>,
    },
    Open {
        response: SyncSender<Result<Project, ProjectStoreError>>,
    },
    Commit {
        expected_revision: u64,
        project: Project,
        event: Box<ProjectEvent>,
        response: SyncSender<Result<(), ProjectStoreError>>,
    },
    EventsAfter {
        after_sequence: u64,
        response: SyncSender<Result<Vec<ProjectEventEnvelope>, ProjectStoreError>>,
    },
}

fn run_actor(connection: &Connection, receiver: &Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Create { project, response } => {
                let _ = response.send(insert_project(connection, &project));
            }
            Command::Open { response } => {
                let _ = response.send(select_project(connection));
            }
            Command::Commit {
                expected_revision,
                project,
                event,
                response,
            } => {
                let _ = response.send(commit_project(
                    connection,
                    expected_revision,
                    &project,
                    &event,
                ));
            }
            Command::EventsAfter {
                after_sequence,
                response,
            } => {
                let _ = response.send(select_events(connection, after_sequence));
            }
        }
    }
}

fn migrate(connection: &Connection) -> Result<(), ProjectPackageError> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA busy_timeout = 5000;
             PRAGMA wal_autocheckpoint = 1000;
             CREATE TABLE IF NOT EXISTS project_metadata (
                 singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                 project_id TEXT NOT NULL,
                 name TEXT NOT NULL,
                 revision INTEGER NOT NULL CHECK (revision >= 0),
                 schema_version INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS project_snapshot (
                 singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                 snapshot_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS project_events (
                 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                 project_revision INTEGER NOT NULL CHECK (project_revision >= 0),
                 event_type TEXT NOT NULL,
                 event_json TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS project_outbox (
                 event_sequence INTEGER PRIMARY KEY REFERENCES project_events(sequence),
                 event_json TEXT NOT NULL,
                 delivered INTEGER NOT NULL DEFAULT 0 CHECK (delivered IN (0, 1))
             );",
        )
        .map_err(ProjectPackageError::Migrate)
}

fn insert_project(connection: &Connection, project: &Project) -> Result<(), ProjectStoreError> {
    let revision = i64::try_from(project.revision()).map_err(|_| {
        ProjectStoreError::Unavailable("project revision exceeds SQLite range".to_owned())
    })?;
    let snapshot = serde_json::to_string(project).map_err(|error| json_unavailable(&error))?;
    let event = ProjectEvent::created(project);
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| unavailable(&error))?;
    transaction
        .execute(
            "INSERT INTO project_metadata
             (singleton, project_id, name, revision, schema_version)
             VALUES (1, ?1, ?2, ?3, 1)",
            params![project.id().as_str(), project.name().as_str(), revision],
        )
        .map_err(|error| match error.sqlite_error_code() {
            Some(ErrorCode::ConstraintViolation) => ProjectStoreError::AlreadyExists,
            _ => unavailable(&error),
        })?;
    transaction
        .execute(
            "INSERT INTO project_snapshot (singleton, snapshot_json) VALUES (1, ?1)",
            [snapshot],
        )
        .map_err(|error| unavailable(&error))?;
    append_event(&transaction, &event)?;
    transaction.commit().map_err(|error| unavailable(&error))
}

fn select_project(connection: &Connection) -> Result<Project, ProjectStoreError> {
    let snapshot = connection
        .query_row(
            "SELECT snapshot_json FROM project_snapshot WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| unavailable(&error))?;
    if let Some(snapshot) = snapshot {
        let project: Project =
            serde_json::from_str(&snapshot).map_err(|error| json_unavailable(&error))?;
        project
            .validate_restored()
            .map_err(|error| ProjectStoreError::Unavailable(error.to_string()))?;
        let (metadata_id, metadata_name, metadata_revision) = connection
            .query_row(
                "SELECT project_id, name, revision FROM project_metadata WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(|error| unavailable(&error))?;
        let metadata_revision = sqlite_revision(metadata_revision)?;
        if project.id().as_str() != metadata_id
            || project.name().as_str() != metadata_name
            || project.revision() != metadata_revision
        {
            return Err(ProjectStoreError::Unavailable(
                "Project snapshot does not match authoritative metadata".to_owned(),
            ));
        }
        return Ok(project);
    }

    let metadata_exists = connection
        .query_row(
            "SELECT 1 FROM project_metadata WHERE singleton = 1",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| unavailable(&error))?;
    if metadata_exists.is_some() {
        Err(ProjectStoreError::Unavailable(
            "Project snapshot is missing while authoritative metadata exists".to_owned(),
        ))
    } else {
        Err(ProjectStoreError::NotFound)
    }
}

fn commit_project(
    connection: &Connection,
    expected_revision: u64,
    project: &Project,
    event: &ProjectEvent,
) -> Result<(), ProjectStoreError> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| unavailable(&error))?;
    let actual_revision = transaction
        .query_row(
            "SELECT revision FROM project_metadata WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| unavailable(&error))?
        .ok_or(ProjectStoreError::NotFound)
        .and_then(sqlite_revision)?;
    if actual_revision != expected_revision {
        return Err(ProjectStoreError::RevisionConflict {
            expected: expected_revision,
            actual: actual_revision,
        });
    }

    let revision = i64::try_from(project.revision()).map_err(|_| {
        ProjectStoreError::Unavailable("project revision exceeds SQLite range".to_owned())
    })?;
    let expected_revision_sql = i64::try_from(expected_revision).map_err(|_| {
        ProjectStoreError::Unavailable("expected revision exceeds SQLite range".to_owned())
    })?;
    let snapshot = serde_json::to_string(project).map_err(|error| json_unavailable(&error))?;
    let changed = transaction
        .execute(
            "UPDATE project_metadata
             SET name = ?1, revision = ?2, schema_version = 1
             WHERE singleton = 1 AND revision = ?3",
            params![project.name().as_str(), revision, expected_revision_sql],
        )
        .map_err(|error| unavailable(&error))?;
    if changed != 1 {
        let actual = select_revision(&transaction)?;
        return Err(ProjectStoreError::RevisionConflict {
            expected: expected_revision,
            actual,
        });
    }
    transaction
        .execute(
            "INSERT INTO project_snapshot (singleton, snapshot_json) VALUES (1, ?1)
             ON CONFLICT(singleton) DO UPDATE SET snapshot_json = excluded.snapshot_json",
            [snapshot],
        )
        .map_err(|error| unavailable(&error))?;
    append_event(&transaction, event)?;
    transaction.commit().map_err(|error| unavailable(&error))
}

fn append_event(connection: &Connection, event: &ProjectEvent) -> Result<(), ProjectStoreError> {
    let event_json = serde_json::to_string(event).map_err(|error| json_unavailable(&error))?;
    let revision = i64::try_from(event.project_revision()).map_err(|_| {
        ProjectStoreError::Unavailable("event revision exceeds SQLite range".to_owned())
    })?;
    connection
        .execute(
            "INSERT INTO project_events (project_revision, event_type, event_json)
             VALUES (?1, ?2, ?3)",
            params![revision, event.kind_name(), event_json],
        )
        .map_err(|error| unavailable(&error))?;
    let sequence = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO project_outbox (event_sequence, event_json) VALUES (?1, ?2)",
            params![sequence, event_json],
        )
        .map_err(|error| unavailable(&error))?;
    Ok(())
}

fn select_events(
    connection: &Connection,
    after_sequence: u64,
) -> Result<Vec<ProjectEventEnvelope>, ProjectStoreError> {
    let after_sequence = i64::try_from(after_sequence).map_err(|_| {
        ProjectStoreError::Unavailable("event cursor exceeds SQLite range".to_owned())
    })?;
    let mut statement = connection
        .prepare(
            "SELECT sequence, event_json FROM project_events
             WHERE sequence > ?1 ORDER BY sequence ASC",
        )
        .map_err(|error| unavailable(&error))?;
    let rows = statement
        .query_map([after_sequence], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| unavailable(&error))?;
    let mut events = Vec::new();
    for row in rows {
        let (sequence, json) = row.map_err(|error| unavailable(&error))?;
        let sequence = u64::try_from(sequence).map_err(|_| {
            ProjectStoreError::Unavailable("stored event sequence is negative".to_owned())
        })?;
        let event = serde_json::from_str(&json).map_err(|error| json_unavailable(&error))?;
        events.push(ProjectEventEnvelope::new(sequence, event));
    }
    Ok(events)
}

fn select_revision(connection: &Connection) -> Result<u64, ProjectStoreError> {
    connection
        .query_row(
            "SELECT revision FROM project_metadata WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| unavailable(&error))
        .and_then(sqlite_revision)
}

fn sqlite_revision(revision: i64) -> Result<u64, ProjectStoreError> {
    u64::try_from(revision)
        .map_err(|_| ProjectStoreError::Unavailable("stored revision is negative".to_owned()))
}

fn actor_stopped() -> ProjectStoreError {
    ProjectStoreError::Unavailable("project DB actor stopped unexpectedly".to_owned())
}

fn unavailable(error: &rusqlite::Error) -> ProjectStoreError {
    ProjectStoreError::Unavailable(error.to_string())
}

fn json_unavailable(error: &serde_json::Error) -> ProjectStoreError {
    ProjectStoreError::Unavailable(error.to_string())
}

fn acquire_package_lock(package_root: &Path, owner: &str) -> Result<File, ProjectPackageError> {
    let path = package_root.join(".autostudio.lock");
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|source| ProjectPackageError::LockFile {
            path: path.clone(),
            source,
        })?;
    match file.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            let owner = fs::read_to_string(&path)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            return Err(ProjectPackageError::AlreadyOpen { owner });
        }
        Err(source) => {
            return Err(ProjectPackageError::LockFile { path, source });
        }
    }
    file.set_len(0)
        .and_then(|()| file.write_all(owner.as_bytes()))
        .and_then(|()| file.sync_all())
        .map_err(|source| ProjectPackageError::LockFile { path, source })?;
    Ok(file)
}
