use std::sync::Arc;

use autostudio_core::agent::AgentRunStatus;
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_provider::{AgentPlanner, DeterministicInferenceAdapter};

#[tokio::test]
async fn deterministic_agent_plans_from_a_project_snapshot_without_private_reasoning() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("fake-agent.autostudio"))
            .expect("open project store"),
    );
    let projects = Arc::new(ProjectService::new(store));
    projects.create_project("Night Drive").expect("project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "Nocturnal synthwave cue".to_owned(),
                purpose: Some("film opening".to_owned()),
                style: vec!["synthwave".to_owned()],
                mood: vec!["tense".to_owned()],
                instrumentation: vec!["analog synth".to_owned()],
                target_duration_seconds: Some(60),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");
    let planner = AgentPlanner::new(projects, Arc::new(DeterministicInferenceAdapter));

    let project = planner.plan(1).await.expect("plan Agent Run");
    let run = project.agent_runs().first().expect("Agent Run");
    assert_eq!(run.status(), AgentRunStatus::AwaitingApproval);
    let json = serde_json::to_string(run).expect("run JSON");
    assert!(!json.contains("reasoning"));
    assert!(!json.contains("chainOfThought"));
    assert!(json.contains(r#""thinkingControl":"effort""#));
    assert!(json.contains(r#""modelEffort":"high""#));
    assert!(json.contains(r#""inputTokens":42"#));
    assert!(json.contains(r#""outputTokens":12"#));
    assert!(run.plan_value().input_hash().starts_with("sha256:"));
}
