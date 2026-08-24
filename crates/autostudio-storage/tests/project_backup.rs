use std::sync::Arc;

use autostudio_core::project::ProjectService;
use autostudio_storage::{ProjectPackageBackup, SqliteProjectStore};

#[test]
fn live_project_is_copied_to_an_atomic_reopenable_backup() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("source.autostudio");
    let backups = temp.path().join("backups");
    let store = Arc::new(SqliteProjectStore::open(&package).expect("source store"));
    let projects = ProjectService::new(store);
    let source = projects.create_project("Backup Source").expect("project");
    std::fs::create_dir_all(package.join("assets")).expect("assets");
    std::fs::write(
        package.join("assets/unreferenced-safe.bin"),
        b"safe extra asset",
    )
    .expect("asset");
    let sink = ProjectPackageBackup::new(&package, &backups).expect("backup sink");

    let backup = projects.backup_project(0, &sink).expect("backup Project");
    let backup_json = serde_json::to_value(backup).expect("backup JSON");
    let backup_name = backup_json["backupName"].as_str().expect("backup name");
    let backup_package = backups.join(backup_name);
    assert!(backup_package.join("project.db").is_file());
    assert_eq!(
        std::fs::read(backup_package.join("assets/unreferenced-safe.bin")).expect("copied asset"),
        b"safe extra asset"
    );

    let reopened = ProjectService::new(Arc::new(
        SqliteProjectStore::open(&backup_package).expect("backup store"),
    ))
    .open_project()
    .expect("reopen backup");
    assert_eq!(reopened, source);
    assert_eq!(projects.open_project().expect("source unchanged"), source);
}

#[cfg(unix)]
#[test]
fn backup_rejects_symlinks_in_project_assets() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("source.autostudio");
    let backups = temp.path().join("backups");
    let store = Arc::new(SqliteProjectStore::open(&package).expect("source store"));
    let projects = ProjectService::new(store);
    projects.create_project("Backup Source").expect("project");
    std::fs::create_dir_all(package.join("assets")).expect("assets");
    let outside = temp.path().join("outside.bin");
    std::fs::write(&outside, b"outside").expect("outside file");
    symlink(&outside, package.join("assets/escape.bin")).expect("asset symlink");
    let sink = ProjectPackageBackup::new(&package, &backups).expect("backup sink");

    let error = projects
        .backup_project(0, &sink)
        .expect_err("backup symlink must be rejected");
    assert!(error.to_string().contains("unsafe non-regular entry"));
    assert_eq!(
        std::fs::read_dir(&backups).expect("backup root").count(),
        0,
        "failed backup must remove partial publication"
    );
}
