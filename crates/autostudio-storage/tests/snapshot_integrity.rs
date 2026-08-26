use std::sync::Arc;

use autostudio_core::project::ProjectService;
use autostudio_storage::SqliteProjectStore;
use rusqlite::Connection;

#[test]
fn corrupted_snapshot_is_rejected_instead_of_becoming_project_truth() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("corrupt.autostudio");
    {
        let store = SqliteProjectStore::open(&package).expect("project store");
        ProjectService::new(Arc::new(store))
            .create_project("Safe Project")
            .expect("create project");
    }

    let database = Connection::open(package.join("project.db")).expect("database");
    let snapshot: String = database
        .query_row(
            "SELECT snapshot_json FROM project_snapshot WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("snapshot");
    let mut snapshot: serde_json::Value = serde_json::from_str(&snapshot).expect("snapshot JSON");
    snapshot["name"] = serde_json::Value::String("   ".to_owned());
    database
        .execute(
            "UPDATE project_snapshot SET snapshot_json = ?1 WHERE singleton = 1",
            [serde_json::to_string(&snapshot).expect("corrupt JSON")],
        )
        .expect("corrupt snapshot");
    drop(database);

    let store = SqliteProjectStore::open(&package).expect("reopen package");
    let error = ProjectService::new(Arc::new(store))
        .open_project()
        .expect_err("invalid snapshot must not open");
    assert!(error.to_string().contains("project name must not be empty"));
}

#[test]
fn older_agent_plan_snapshot_without_usage_restores_with_unknown_usage() {
    use autostudio_core::agent::{
        AgentDecision, AgentPlanDraft, CostEstimate, GenerationIntent, InferenceProvenance,
        InferenceUsage,
    };

    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("compatible.autostudio");
    {
        let store = SqliteProjectStore::open(&package).expect("project store");
        let projects = ProjectService::new(Arc::new(store));
        projects.create_project("Compatible").expect("project");
        projects
            .plan_agent_run(
                0,
                AgentPlanDraft {
                    visible_summary: "Generate one direction".to_owned(),
                    decision: AgentDecision::GenerateMusic(GenerationIntent {
                        prompt: "short cue".to_owned(),
                        duration_seconds: 10,
                        candidate_count: 1,
                    }),
                    estimated_cost: CostEstimate::Unknown,
                    usage: InferenceUsage::default(),
                    inference: InferenceProvenance::default(),
                    input_hash: "sha256:old".to_owned(),
                },
            )
            .expect("plan");
    }
    let database = Connection::open(package.join("project.db")).expect("database");
    let snapshot: String = database
        .query_row(
            "SELECT snapshot_json FROM project_snapshot WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("snapshot");
    let mut snapshot: serde_json::Value = serde_json::from_str(&snapshot).expect("snapshot JSON");
    snapshot["agentRuns"][0]["plan"]
        .as_object_mut()
        .expect("plan object")
        .remove("usage");
    database
        .execute(
            "UPDATE project_snapshot SET snapshot_json = ?1 WHERE singleton = 1",
            [serde_json::to_string(&snapshot).expect("old snapshot JSON")],
        )
        .expect("store old snapshot");
    drop(database);

    let store = SqliteProjectStore::open(&package).expect("reopen package");
    let restored = ProjectService::new(Arc::new(store))
        .open_project()
        .expect("restore compatible snapshot");
    assert_eq!(
        restored.agent_runs()[0]
            .plan_value()
            .expect("restored planned run")
            .usage(),
        &InferenceUsage::default()
    );
}

#[test]
fn snapshot_and_metadata_revision_mismatch_is_rejected() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("mismatch.autostudio");
    {
        let store = SqliteProjectStore::open(&package).expect("project store");
        ProjectService::new(Arc::new(store))
            .create_project("Safe Project")
            .expect("create project");
    }
    let database = Connection::open(package.join("project.db")).expect("database");
    database
        .execute(
            "UPDATE project_metadata SET revision = 1 WHERE singleton = 1",
            [],
        )
        .expect("change metadata");
    drop(database);

    let store = SqliteProjectStore::open(&package).expect("reopen package");
    let error = ProjectService::new(Arc::new(store))
        .open_project()
        .expect_err("split-brain metadata must not open");
    assert!(
        error
            .to_string()
            .contains("does not match authoritative metadata")
    );
}

#[test]
fn missing_snapshot_is_rejected_when_authoritative_metadata_exists() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("missing-snapshot.autostudio");
    {
        let store = SqliteProjectStore::open(&package).expect("project store");
        ProjectService::new(Arc::new(store))
            .create_project("Must Not Become Empty")
            .expect("create project");
    }

    let database = Connection::open(package.join("project.db")).expect("database");
    database
        .execute("DELETE FROM project_snapshot WHERE singleton = 1", [])
        .expect("remove snapshot");
    drop(database);

    let store = SqliteProjectStore::open(&package).expect("reopen package");
    let error = ProjectService::new(Arc::new(store))
        .open_project()
        .expect_err("missing snapshot must not restore an empty Project");
    assert!(error.to_string().contains("snapshot is missing"));
}
