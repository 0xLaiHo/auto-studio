use std::sync::Arc;

use autostudio_core::project::ProjectService;
use autostudio_storage::SqliteProjectStore;

#[test]
fn creator_can_reopen_a_created_project_package() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("night-drive.autostudio");

    let created = {
        let store = SqliteProjectStore::open(&package).expect("open project store");
        let projects = ProjectService::new(Arc::new(store));
        projects
            .create_project("Night Drive")
            .expect("create project")
    };

    let reopened = {
        let store = SqliteProjectStore::open(&package).expect("reopen project store");
        let projects = ProjectService::new(Arc::new(store));
        projects.open_project().expect("open project")
    };

    assert_eq!(reopened, created);
    assert_eq!(reopened.name().as_str(), "Night Drive");
    assert_eq!(reopened.revision(), 0);
    assert!(package.join("project.db").is_file());
}
