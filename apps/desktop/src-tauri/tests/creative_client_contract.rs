use std::sync::Arc;

use autostudio_api::discovery::{DiscoveryFile, DiscoveryRecord};
use autostudio_api::router_with_runtime_media_and_backup;
use autostudio_core::project::ProjectService;
use autostudio_desktop::core_client::{CoreClient, CreativeBriefInput};
use autostudio_media::ProjectMedia;
use autostudio_provider::{
    AgentPlanner, DeterministicGenerationAdapter, DeterministicInferenceAdapter,
    GenerationCoordinator, LocalCreativeRuntime,
};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn desktop_client_completes_the_fake_creative_workflow_through_core_only() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("desktop-creative.autostudio");
    let staging = temp.path().join("staging");
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("project store"));
    let projects = Arc::new(ProjectService::new(store));
    let media = Arc::new(ProjectMedia::new(&package, &staging).expect("media"));
    let runtime = LocalCreativeRuntime::new(
        AgentPlanner::new(projects.clone(), Arc::new(DeterministicInferenceAdapter)),
        GenerationCoordinator::new(
            projects.clone(),
            Arc::new(DeterministicGenerationAdapter::new(&staging).expect("fake provider")),
            media.clone(),
        ),
    );
    let token = "desktop-creative-session-token-at-least-32-bytes";
    let backups = temp.path().join("backups");
    let backup = Arc::new(
        autostudio_storage::ProjectPackageBackup::new(&package, &backups).expect("backup"),
    );
    let app = router_with_runtime_media_and_backup(
        projects,
        token,
        Arc::new(runtime),
        media.clone(),
        media,
        backup,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Core server");
    });
    let discovery_path = temp.path().join("runtime/core.json");
    DiscoveryFile::new(&discovery_path)
        .publish(&DiscoveryRecord::new(
            "desktop-creative-core",
            std::process::id(),
            endpoint,
            token,
        ))
        .expect("discovery");
    let client = CoreClient::new(discovery_path);

    let created = client.create_project("Night Drive").await.expect("project");
    let briefed = client
        .set_brief(
            created.revision,
            CreativeBriefInput {
                summary: "Nocturnal synthwave cue".to_owned(),
                purpose: Some("film opening".to_owned()),
                style: vec!["synthwave".to_owned()],
                mood: vec!["tense".to_owned()],
                instrumentation: vec!["analog synth".to_owned()],
                target_duration_seconds: Some(1),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .await
        .expect("brief");
    let planned = client.plan_agent_run(briefed.revision).await.expect("plan");
    let run = &planned.agent_runs[0];
    let approved = client
        .approve_agent_run(&run.id, planned.revision, "USD", 100, &run.plan.input_hash)
        .await
        .expect("approval");
    let generated = client
        .execute_agent_run(&run.id, approved.revision)
        .await
        .expect("generation");
    assert_eq!(generated.candidates.len(), 2);
    let preview = client
        .preview_asset(&generated.candidates[0].asset.id)
        .await
        .expect("Preview Playback");
    assert_eq!(&preview[..4], b"RIFF");
    let selected = client
        .select_candidate(&generated.candidates[0].id, generated.revision, 0)
        .await
        .expect("Selection");
    assert!(selected.selection.is_some());
    assert_eq!(selected.timeline.clips.len(), 1);
    let handed_off = client
        .export_handoff(selected.revision)
        .await
        .expect("DAW Handoff");
    assert_eq!(handed_off.exports.len(), 1);
    assert!(
        package
            .join(&handed_off.exports[0].relative_path)
            .join("manifest.json")
            .is_file()
    );
    let backed_up = client
        .backup_project(handed_off.revision)
        .await
        .expect("Project backup");
    assert_eq!(backed_up.source_project_revision, handed_off.revision);
    assert!(
        backups
            .join(backed_up.backup_name)
            .join("project.db")
            .is_file()
    );

    server.abort();
}
