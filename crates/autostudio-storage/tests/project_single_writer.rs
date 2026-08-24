#[test]
fn a_second_core_cannot_open_the_same_project_package_for_writing() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("night-drive.autostudio");
    let first = autostudio_storage::SqliteProjectStore::open_with_owner(&package, "core-one")
        .expect("first Project Session");

    let Err(error) = autostudio_storage::SqliteProjectStore::open_with_owner(&package, "core-two")
    else {
        panic!("second writer must be rejected");
    };
    assert!(matches!(
        error,
        autostudio_storage::ProjectPackageError::AlreadyOpen { owner }
            if owner.as_deref() == Some("core-one")
    ));

    drop(first);
    autostudio_storage::SqliteProjectStore::open_with_owner(&package, "core-two")
        .expect("OS lock is released when the first Core stops");
}
