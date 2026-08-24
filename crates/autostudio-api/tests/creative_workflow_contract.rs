use std::sync::Arc;

use autostudio_api::router_with_runtime_media_and_backup;
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_media::ProjectMedia;
use autostudio_provider::{
    AgentPlanner, DeterministicGenerationAdapter, DeterministicInferenceAdapter,
    GenerationCoordinator, LocalCreativeRuntime,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn desktop_can_complete_the_fake_ship_zero_creative_workflow() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("creative-workflow.autostudio");
    let staging = temp.path().join("provider-staging");
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("project store"));
    let projects = Arc::new(ProjectService::new(store));
    projects.create_project("Night Drive").expect("project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "Nocturnal synthwave cue".to_owned(),
                purpose: None,
                style: vec!["synthwave".to_owned()],
                mood: vec!["tense".to_owned()],
                instrumentation: vec!["analog synth".to_owned()],
                target_duration_seconds: Some(1),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");
    let agent_planner =
        AgentPlanner::new(projects.clone(), Arc::new(DeterministicInferenceAdapter));
    let media = Arc::new(ProjectMedia::new(&package, &staging).expect("Project media"));
    let generation = GenerationCoordinator::new(
        projects.clone(),
        Arc::new(DeterministicGenerationAdapter::new(&staging).expect("fake generation")),
        media.clone(),
    );
    let backup_root = temp.path().join("backups");
    let backup = Arc::new(
        autostudio_storage::ProjectPackageBackup::new(&package, &backup_root)
            .expect("Project backup"),
    );
    let app = router_with_runtime_media_and_backup(
        projects,
        "test-session-token",
        Arc::new(LocalCreativeRuntime::new(agent_planner, generation)),
        media.clone(),
        media,
        backup,
    );

    let planned_response = command(&app, "/v1/agent-runs", r#"{"expectedRevision":1}"#).await;
    assert_eq!(planned_response.0, StatusCode::OK);
    let run_id = planned_response.1["agentRuns"][0]["id"]
        .as_str()
        .expect("run id");
    let input_hash = planned_response.1["agentRuns"][0]["plan"]["inputHash"]
        .as_str()
        .expect("input hash");

    let approved = command(
        &app,
        &format!("/v1/agent-runs/{run_id}/approval"),
        &format!(
            r#"{{"expectedRevision":2,"approval":{{"currency":"USD","maxMinorUnits":100,"inputHash":"{input_hash}"}}}}"#
        ),
    )
    .await;
    assert_eq!(approved.0, StatusCode::OK);
    assert_eq!(approved.1["agentRuns"][0]["status"], "ready_to_submit");

    let executed = command(
        &app,
        &format!("/v1/agent-runs/{run_id}/execute"),
        r#"{"expectedRevision":3}"#,
    )
    .await;
    assert_eq!(executed.0, StatusCode::OK);
    assert_eq!(executed.1["revision"], 6);
    assert_eq!(
        executed.1["candidates"]
            .as_array()
            .expect("Candidates")
            .len(),
        2
    );
    let candidate_id = executed.1["candidates"][0]["id"]
        .as_str()
        .expect("Candidate id");
    let asset_version_id = executed.1["candidates"][0]["asset"]["id"]
        .as_str()
        .expect("Asset Version id");

    let preview = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/assets/{asset_version_id}/preview"))
                .header("authorization", "Bearer test-session-token")
                .header("range", "bytes=0-15")
                .body(Body::empty())
                .expect("preview request"),
        )
        .await
        .expect("preview response");
    assert_eq!(preview.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(preview.headers()["accept-ranges"], "bytes");
    assert!(
        preview.headers()["content-range"]
            .to_str()
            .expect("Content-Range")
            .starts_with("bytes 0-15/")
    );
    assert_eq!(
        preview
            .into_body()
            .collect()
            .await
            .expect("preview body")
            .to_bytes()
            .len(),
        16
    );

    let invalid_range = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/assets/{asset_version_id}/preview"))
                .header("authorization", "Bearer test-session-token")
                .header("range", "bytes=0-1,4-5")
                .body(Body::empty())
                .expect("invalid preview request"),
        )
        .await
        .expect("invalid preview response");
    assert_eq!(invalid_range.status(), StatusCode::RANGE_NOT_SATISFIABLE);

    let stale_selection = command(
        &app,
        &format!("/v1/candidates/{candidate_id}/selection"),
        r#"{"expectedRevision":5,"startMicros":0}"#,
    )
    .await;
    assert_eq!(stale_selection.0, StatusCode::CONFLICT);
    assert_eq!(stale_selection.1["code"], "project_revision_conflict");

    let selected = command(
        &app,
        &format!("/v1/candidates/{candidate_id}/selection"),
        r#"{"expectedRevision":6,"startMicros":0}"#,
    )
    .await;
    assert_eq!(selected.0, StatusCode::OK);
    assert_eq!(selected.1["selection"]["candidateId"], candidate_id);
    assert_eq!(
        selected.1["timeline"]["clips"]
            .as_array()
            .expect("clips")
            .len(),
        1
    );

    let handed_off = command(&app, "/v1/handoffs", r#"{"expectedRevision":7}"#).await;
    assert_eq!(handed_off.0, StatusCode::OK);
    assert_eq!(handed_off.1["revision"], 8);
    assert_eq!(
        handed_off.1["exports"].as_array().expect("exports").len(),
        1
    );
    let handoff_path = handed_off.1["exports"][0]["relativePath"]
        .as_str()
        .expect("handoff path");
    assert!(package.join(handoff_path).join("manifest.json").is_file());

    let backed_up = command(
        &app,
        "/v1/projects/current/backup",
        r#"{"expectedRevision":8}"#,
    )
    .await;
    assert_eq!(backed_up.0, StatusCode::OK);
    assert_eq!(backed_up.1["sourceProjectRevision"], 8);
    let backup_name = backed_up.1["backupName"].as_str().expect("backup name");
    assert!(backup_root.join(backup_name).join("project.db").is_file());
}

async fn command(app: &axum::Router, uri: &str, json: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(json.to_owned()))
                .expect("command request"),
        )
        .await
        .expect("command response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("command body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&body).expect("JSON response"),
    )
}
