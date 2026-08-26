use std::env;
use std::fs;
use std::sync::Arc;

use autostudio_core::agent::AgentRunStatus;
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::ThinkingLevel;
use autostudio_provider::AgentPlanner;
use autostudio_provider::constants::{
    DEFAULT_OPENAI_BASE_URL, ENV_OPENAI_API_KEY, PROVIDER_OPENAI,
};
use autostudio_provider::context::ContextManager;
use autostudio_provider::continuity::FileContinuityVault;
use autostudio_provider::llm::{HttpInferenceAdapter, LlmProtocol, LlmProviderConfig};
use autostudio_storage::SqliteProjectStore;

const LIVE_MODEL: &str = "gpt-5-mini";
const LIVE_MODEL_ENV: &str = "OPENAI_LIVE_MODEL";
const LIVE_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const LIVE_VAULT_TTL_MILLIS: u64 = 60_000;

#[tokio::test]
#[ignore = "requires an explicitly supplied OPENAI_API_KEY and makes billable requests"]
async fn openai_responses_preserves_private_continuity_across_the_planning_tool_loop() {
    let api_key = env::var(ENV_OPENAI_API_KEY).expect("an explicitly supplied OPENAI_API_KEY");
    let model = env::var(LIVE_MODEL_ENV).unwrap_or_else(|_| LIVE_MODEL.to_owned());
    assert_eq!(
        model, LIVE_MODEL,
        "the live contract is pinned to the low-cost qualification model"
    );
    let base_url =
        env::var(LIVE_BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.to_owned());
    let config = LlmProviderConfig::new(
        PROVIDER_OPENAI,
        LlmProtocol::OpenAiResponses,
        &base_url,
        model.clone(),
        api_key,
    )
    .expect("OpenAI live configuration")
    .with_thinking_level(ThinkingLevel::Low);

    let temp = tempfile::tempdir().expect("temporary live qualification directory");
    let package = temp.path().join("openai-live.autostudio");
    let app_state = temp.path().join("app-state");
    let vault_root = app_state.join("continuity");
    let key_path = app_state.join("continuity.key");
    let store = Arc::new(SqliteProjectStore::open(&package).expect("Project store"));
    let projects = Arc::new(ProjectService::new(store.clone()));
    let contexts = Arc::new(ContextManager::new(store));
    projects
        .create_project("OpenAI continuity live")
        .expect("Project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "Create a twelve-second instrumental product ident".to_owned(),
                purpose: Some("low-cost provider qualification".to_owned()),
                style: vec!["minimal electronic".to_owned()],
                mood: vec!["confident".to_owned()],
                instrumentation: vec!["synth".to_owned(), "percussion".to_owned()],
                target_duration_seconds: Some(12),
                lyrics: None,
                constraints: vec!["instrumental".to_owned(), "one candidate".to_owned()],
            },
        )
        .expect("Creative Brief");
    let vault = Arc::new(
        FileContinuityVault::open_for_project(
            &vault_root,
            &key_path,
            &package,
            LIVE_VAULT_TTL_MILLIS,
        )
        .expect("Project-external Continuity Vault"),
    );
    let planner = AgentPlanner::with_continuity_vault(
        projects,
        contexts.clone(),
        Arc::new(HttpInferenceAdapter::new(config).expect("OpenAI HTTP adapter")),
        vault,
    );

    let completed = planner.plan(1).await.expect("OpenAI live Planning Run");
    let run = completed.agent_runs().first().expect("Agent Run");
    assert_eq!(run.status(), AgentRunStatus::AwaitingApproval);
    let plan = run.plan_value().expect("typed Creative Plan");
    assert_eq!(plan.inference().provider_kind, PROVIDER_OPENAI);
    assert_eq!(plan.inference().model, LIVE_MODEL);
    assert!(plan.usage().input_tokens.is_some());
    assert!(plan.usage().output_tokens.is_some());

    let projection = contexts.inspect_run(run.id()).expect("durable transcript");
    assert!(projection.manifests().len() >= 2);
    assert!(
        projection
            .manifests()
            .iter()
            .skip(1)
            .any(|manifest| manifest.continuity_reference().is_some()),
        "a later Provider turn must bind the encrypted first-turn reasoning state"
    );
    assert!(
        fs::read_dir(&vault_root)
            .expect("Continuity Vault")
            .filter_map(Result::ok)
            .all(
                |entry| entry.path().extension().and_then(|value| value.to_str())
                    != Some("continuity")
            ),
        "terminal Planning commit must purge private Continuity payload"
    );

    eprintln!(
        "OpenAI continuity live PASS: model={}, turns={}, input_tokens={}, output_tokens={}",
        model,
        projection.manifests().len(),
        plan.usage().input_tokens.unwrap_or_default(),
        plan.usage().output_tokens.unwrap_or_default()
    );
}
