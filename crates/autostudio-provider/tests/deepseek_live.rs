use std::sync::Arc;

use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_provider::constants::PROVIDER_DEEPSEEK;
use autostudio_provider::llm::{HttpInferenceAdapter, LlmProviderConfig};
use autostudio_provider::{InferenceAdapter, InferenceRequest};

#[tokio::test]
#[ignore = "requires an explicitly supplied DEEPSEEK_API_KEY and makes a billable request"]
async fn deepseek_returns_a_schema_valid_creative_plan() {
    let config = LlmProviderConfig::from_environment(PROVIDER_DEEPSEEK)
        .expect("DEEPSEEK_API_KEY and valid DeepSeek configuration");
    let adapter = HttpInferenceAdapter::new(config).expect("DeepSeek HTTP adapter");
    let request = request();

    let outcome = adapter
        .infer(request)
        .await
        .expect("live DeepSeek plan response");

    assert_eq!(adapter.descriptor().provider_kind, PROVIDER_DEEPSEEK);
    assert!(!outcome.visible_summary.trim().is_empty());
    assert!(outcome.response_id.is_some());
}

fn request() -> InferenceRequest {
    let temp = tempfile::tempdir().expect("temporary project");
    let store = autostudio_storage::SqliteProjectStore::open(&temp.path().join("live.autostudio"))
        .expect("project store");
    let projects = ProjectService::new(Arc::new(store));
    projects
        .create_project("DeepSeek live smoke")
        .expect("project");
    let project = projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "Create a concise instrumental ident for a professional audio tool"
                    .to_owned(),
                purpose: Some("product ident".to_owned()),
                style: vec!["modern electronic".to_owned()],
                mood: vec!["confident".to_owned()],
                instrumentation: vec!["synth".to_owned(), "percussion".to_owned()],
                target_duration_seconds: Some(12),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");
    InferenceRequest {
        brief: project.brief().expect("saved brief").clone(),
        context_revision: project.revision(),
    }
}
