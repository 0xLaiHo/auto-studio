use std::sync::Arc;

use autostudio_core::agent::{AgentRunStatus, CostApproval};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_media::ProjectMedia;
use autostudio_provider::{
    AgentPlanner, DeterministicGenerationAdapter, DeterministicInferenceAdapter,
    GenerationCoordinator,
};

#[tokio::test]
async fn approved_fake_generation_commits_local_candidates_and_survives_reopen() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("generation.autostudio");
    let staging = temp.path().join("provider-staging");
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("project store"));
    let projects = Arc::new(ProjectService::new(store.clone()));
    let contexts = Arc::new(autostudio_provider::context::ContextManager::new(store));
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
                target_duration_seconds: Some(2),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");
    let agent_planner = AgentPlanner::new(
        projects.clone(),
        contexts,
        Arc::new(DeterministicInferenceAdapter),
    );
    let planned = agent_planner.plan(1).await.expect("plan");
    let run = &planned.agent_runs()[0];
    let run_id = run.id().clone();
    let input_hash = run
        .plan_value()
        .expect("approved run has a plan")
        .input_hash()
        .to_owned();
    projects
        .approve_agent_run(
            planned.revision(),
            &run_id,
            CostApproval {
                currency: "USD".to_owned(),
                max_minor_units: 100,
                input_hash,
            },
        )
        .expect("approval");
    let media = Arc::new(ProjectMedia::new(&package, &staging).expect("Project media"));
    let generation = GenerationCoordinator::new(
        projects.clone(),
        Arc::new(DeterministicGenerationAdapter::new(&staging).expect("fake Music Provider")),
        media,
    );

    let approved = projects.open_project().expect("approved Project");
    let completed = generation
        .execute_approved(approved.revision(), &run_id)
        .await
        .expect("execute approved generation");
    assert_eq!(completed.revision(), 7);
    assert_eq!(
        completed.agent_runs()[0].status(),
        AgentRunStatus::Completed
    );
    assert_eq!(completed.candidates().len(), 2);
    for candidate in completed.candidates() {
        assert!(package.join(candidate.asset().relative_path()).is_file());
    }
    assert_eq!(projects.open_project().expect("reopen"), completed);
}
