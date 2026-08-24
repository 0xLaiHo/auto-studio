use std::sync::Arc;

use autostudio_core::agent::{
    AgentDecision, AgentPlanDraft, AgentRunStatus, CostApproval, CostEstimate, GenerationIntent,
    InferenceProvenance, InferenceUsage,
};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};

#[test]
fn agent_plan_and_creator_approval_are_distinct_durable_project_facts() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("agent.autostudio"))
            .expect("open project store"),
    );
    let projects = ProjectService::new(store);
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
                target_duration_seconds: Some(60),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");

    let planned = projects
        .plan_agent_run(
            1,
            AgentPlanDraft {
                visible_summary: "Generate two synthwave directions for comparison".to_owned(),
                decision: AgentDecision::GenerateMusic(GenerationIntent {
                    prompt: "tense nocturnal synthwave, instrumental".to_owned(),
                    duration_seconds: 60,
                    candidate_count: 2,
                }),
                estimated_cost: CostEstimate::Known {
                    currency: "USD".to_owned(),
                    lower_minor_units: 40,
                    upper_minor_units: 80,
                },
                usage: InferenceUsage::default(),
                inference: InferenceProvenance::default(),
                input_hash: "sha256:brief-revision-1".to_owned(),
            },
        )
        .expect("plan Agent Run");
    let run = planned.agent_runs().first().expect("Agent Run");
    assert_eq!(run.status(), AgentRunStatus::AwaitingApproval);
    assert!(run.approval().is_none());
    let run_id = run.id().clone();

    let approved = projects
        .approve_agent_run(
            2,
            &run_id,
            CostApproval {
                currency: "USD".to_owned(),
                max_minor_units: 80,
                input_hash: "sha256:brief-revision-1".to_owned(),
            },
        )
        .expect("approve Agent Run");
    let run = approved.agent_runs().first().expect("approved Agent Run");
    assert_eq!(run.status(), AgentRunStatus::ReadyToSubmit);
    assert!(run.approval().is_some());
    assert_eq!(projects.open_project().expect("reopen"), approved);

    let event_names: Vec<_> = projects
        .events_after(0)
        .expect("events")
        .iter()
        .map(|event| event.event().kind_name())
        .collect();
    assert_eq!(
        event_names,
        [
            "project.created",
            "brief.updated",
            "agent_run.planned",
            "agent_run.approved"
        ]
    );
}
