use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use autostudio_core::agent::{AgentRunId, AgentRunStatus, InferenceUsage};
use autostudio_core::context::{CanonicalToolCall, ContextEventStore, InferenceTurnId};
use autostudio_core::continuity::{ContinuityBinding, ContinuityReference};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::constants::{
    PLAN_TOOL_NAME, PROJECT_DESCRIBE_TOOL_NAME, PROTOCOL_OPENAI_RESPONSES,
};
use autostudio_provider::continuity::{
    ContinuityFormat, ContinuityVault, FileContinuityVault, LoadedContinuity,
    ProviderContinuityState,
};
use autostudio_provider::{
    AgentPlanner, ContinuityVaultError, DeterministicInferenceAdapter, InferenceAdapter,
    InferenceFuture, InferenceOutcome, InferenceProviderDescriptor, InferenceTurnRequest,
};
use autostudio_storage::{ProjectPackageBackup, SqliteProjectStore};
use serde_json::json;

const SENTINEL: &str = "PLANNER_PRIVATE_REASONING_SENTINEL_CM2";

#[tokio::test]
async fn planner_replays_only_a_reference_and_purges_private_state_at_terminal_commit() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("project.autostudio");
    let vault_root = temp.path().join("app-state").join("continuity");
    let key_path = temp.path().join("app-state").join("continuity.key");
    let backup_root = temp.path().join("backups");
    let store = Arc::new(SqliteProjectStore::open(&package).expect("Project store"));
    let projects = Arc::new(ProjectService::new(store.clone()));
    let contexts = Arc::new(autostudio_provider::context::ContextManager::new(
        store.clone(),
    ));
    projects.create_project("CM-2 Contract").expect("Project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A concise electronic cue".to_owned(),
                purpose: Some("title sequence".to_owned()),
                style: vec!["electronic".to_owned()],
                mood: vec!["focused".to_owned()],
                instrumentation: vec!["synth".to_owned()],
                target_duration_seconds: Some(30),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("Brief");
    let vault = Arc::new(
        FileContinuityVault::open(&vault_root, &key_path, 60_000).expect("Continuity Vault"),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let planner = AgentPlanner::with_continuity_vault(
        projects.clone(),
        contexts,
        Arc::new(ContinuityInference {
            calls: calls.clone(),
            vault_root: vault_root.clone(),
        }),
        vault,
    );

    let completed = planner.plan(1).await.expect("two-turn Planning Run");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        completed.agent_runs()[0].status(),
        AgentRunStatus::AwaitingApproval
    );
    let run_id = completed.agent_runs()[0].id();
    assert!(
        !vault_root
            .join(format!("{}.continuity", run_id.as_str()))
            .exists()
    );

    let event_json = serde_json::to_vec(&store.context_events(run_id).expect("Context events"))
        .expect("Context JSON");
    assert!(!contains(&event_json, SENTINEL));
    assert!(!tree_contains(&package, SENTINEL));

    let backup = ProjectPackageBackup::new(&package, &backup_root).expect("backup sink");
    let receipt = projects
        .backup_project(completed.revision(), &backup)
        .expect("Project backup");
    let receipt_json = serde_json::to_value(receipt).expect("backup receipt");
    let backup_name = receipt_json["backupName"].as_str().expect("backup name");
    assert!(!tree_contains(&backup_root.join(backup_name), SENTINEL));
}

#[tokio::test]
async fn purge_failure_prevents_a_successful_planning_commit() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("purge-failure.autostudio");
    let store = Arc::new(SqliteProjectStore::open(&package).expect("Project store"));
    let projects = Arc::new(ProjectService::new(store.clone()));
    let contexts = Arc::new(autostudio_provider::context::ContextManager::new(store));
    projects.create_project("Purge Failure").expect("Project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A concise piano cue".to_owned(),
                purpose: None,
                style: vec!["acoustic".to_owned()],
                mood: vec!["warm".to_owned()],
                instrumentation: vec!["piano".to_owned()],
                target_duration_seconds: Some(30),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("Brief");
    let planner = AgentPlanner::with_continuity_vault(
        projects.clone(),
        contexts,
        Arc::new(DeterministicInferenceAdapter),
        Arc::new(FailingPurgeVault),
    );

    let error = planner
        .plan(1)
        .await
        .expect_err("purge failure must block successful Planning");
    assert!(matches!(
        error,
        autostudio_provider::AgentPlannerError::Continuity(ContinuityVaultError::Crypto)
    ));
    let failed = projects.open_project().expect("failed Project");
    assert_eq!(failed.agent_runs()[0].status(), AgentRunStatus::Failed);
    assert!(failed.agent_runs()[0].plan_value().is_none());
}

struct FailingPurgeVault;

impl ContinuityVault for FailingPurgeVault {
    fn load(
        &self,
        _binding: &ContinuityBinding,
        _now_unix_millis: u64,
    ) -> Result<Option<LoadedContinuity>, ContinuityVaultError> {
        Ok(None)
    }

    fn store(
        &self,
        _binding: &ContinuityBinding,
        _source_turn_id: &InferenceTurnId,
        _state: &ProviderContinuityState,
        _now_unix_millis: u64,
    ) -> Result<ContinuityReference, ContinuityVaultError> {
        Err(ContinuityVaultError::Crypto)
    }

    fn purge_run(&self, _run_id: &AgentRunId) -> Result<(), ContinuityVaultError> {
        Err(ContinuityVaultError::Crypto)
    }

    fn purge_expired(&self, _now_unix_millis: u64) -> Result<usize, ContinuityVaultError> {
        Ok(0)
    }
}

struct ContinuityInference {
    calls: Arc<AtomicUsize>,
    vault_root: PathBuf,
}

impl InferenceAdapter for ContinuityInference {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        descriptor()
    }

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let run_id = request.prepared.manifest().run_id().clone();
        if call == 0 {
            assert!(request.continuity.is_none());
            assert!(request.prepared.manifest().continuity_reference().is_none());
        } else {
            assert_eq!(call, 1, "Planning contract requires exactly two turns");
            let state = request.continuity.as_ref().expect("loaded Continuity");
            assert_eq!(state.format(), ContinuityFormat::OpenAiResponses);
            assert!(!format!("{state:?}").contains(SENTINEL));
            assert!(request.prepared.manifest().continuity_reference().is_some());
            let ciphertext = fs::read(
                self.vault_root
                    .join(format!("{}.continuity", run_id.as_str())),
            )
            .expect("active encrypted state");
            assert!(!contains(&ciphertext, SENTINEL));
        }
        Box::pin(async move {
            let (name, arguments_json, continuity) = if call == 0 {
                (
                    PROJECT_DESCRIBE_TOOL_NAME,
                    "{}".to_owned(),
                    Some(
                        ProviderContinuityState::from_json(
                            ContinuityFormat::OpenAiResponses,
                            &json!([{
                                "type": "reasoning",
                                "id": "rs_planner_cm2",
                                "encrypted_content": SENTINEL
                            }, {
                                "type": "function_call",
                                "call_id": "describe-cm2",
                                "name": PROJECT_DESCRIBE_TOOL_NAME,
                                "arguments": "{}"
                            }]),
                        )
                        .expect("private Continuity fixture"),
                    ),
                )
            } else {
                (
                    PLAN_TOOL_NAME,
                    json!({
                        "visibleSummary": "Create one focused electronic direction",
                        "generationPrompt": "focused electronic instrumental cue",
                        "durationSeconds": 30,
                        "candidateCount": 1
                    })
                    .to_string(),
                    None,
                )
            };
            Ok(InferenceOutcome {
                provider: descriptor(),
                visible_text: None,
                tool_calls: vec![CanonicalToolCall {
                    call_id: if call == 0 {
                        "describe-cm2".to_owned()
                    } else {
                        "plan-cm2".to_owned()
                    },
                    name: name.to_owned(),
                    arguments_json,
                }],
                usage: InferenceUsage {
                    input_tokens: Some(20),
                    output_tokens: Some(10),
                    actual_cost_minor_units: None,
                    currency: None,
                },
                response_id: Some(format!("response-cm2-{call}")),
                continuity,
            })
        })
    }
}

fn descriptor() -> InferenceProviderDescriptor {
    InferenceProviderDescriptor {
        provider_kind: "openai".to_owned(),
        model: "gpt-5.2".to_owned(),
        thinking_level: ThinkingLevel::High,
        thinking_control: ThinkingControl::Effort,
        thinking_budget_tokens: None,
        capability_revision: "continuity-planner-contract/1".to_owned(),
        mapping_revision: "continuity-planner-mapping/1".to_owned(),
        protocol: PROTOCOL_OPENAI_RESPONSES.to_owned(),
    }
}

fn tree_contains(path: &Path, sentinel: &str) -> bool {
    if path.is_file() {
        return fs::read(path).is_ok_and(|bytes| contains(&bytes, sentinel));
    }
    fs::read_dir(path).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| tree_contains(&entry.path(), sentinel))
    })
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}
