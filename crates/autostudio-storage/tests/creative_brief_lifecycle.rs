use std::sync::Arc;

use autostudio_core::project::{CreativeBriefDraft, ProjectService};

#[test]
fn creator_can_update_and_reopen_a_versioned_creative_brief_with_events() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("night-drive.autostudio");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("open project store"),
    );
    let projects = ProjectService::new(store.clone());
    projects
        .create_project("Night Drive")
        .expect("create project");

    let updated = projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A nocturnal synthwave cue for a tense city drive".to_owned(),
                purpose: Some("short-film opening".to_owned()),
                style: vec!["synthwave".to_owned(), "cinematic".to_owned()],
                mood: vec!["tense".to_owned(), "propulsive".to_owned()],
                instrumentation: vec!["analog synth".to_owned(), "drum machine".to_owned()],
                target_duration_seconds: Some(90),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("update brief");

    assert_eq!(updated.revision(), 1);
    assert_eq!(
        updated.brief().expect("creative brief").summary(),
        "A nocturnal synthwave cue for a tense city drive"
    );
    assert_eq!(projects.open_project().expect("reopen project"), updated);

    let events = projects.events_after(0).expect("project events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].event().kind_name(), "project.created");
    assert_eq!(events[1].sequence(), 2);
    assert_eq!(events[1].event().kind_name(), "brief.updated");
    assert_eq!(events[1].event().project_revision(), 1);
}
