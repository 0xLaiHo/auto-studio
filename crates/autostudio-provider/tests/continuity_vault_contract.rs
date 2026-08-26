use std::fs;

use autostudio_core::agent::AgentRunId;
use autostudio_core::context::{InferenceTurnId, ProviderBinding};
use autostudio_core::continuity::ContinuityBinding;
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::ContinuityVaultError;
use autostudio_provider::continuity::{
    ContinuityFormat, ContinuityVault, FileContinuityVault, ProviderContinuityState,
};
use serde_json::{Value, json};

const SENTINEL: &str = "PRIVATE_REASONING_SENTINEL_CM2";
const TTL_MILLIS: u64 = 1_000;

#[test]
fn encrypted_vault_roundtrips_without_exposing_private_payload() {
    let fixture = Fixture::new();
    let run_id = AgentRunId::new();
    let binding = binding(run_id.clone(), "openai", "gpt-5.2", "responses");
    let turn_id = InferenceTurnId::new();
    let state = openai_state();

    let reference = fixture
        .vault
        .store(&binding, &turn_id, &state, 10_000)
        .expect("store encrypted Continuity");
    let state_path = fixture.state_path(&run_id);
    let encrypted = fs::read(&state_path).expect("encrypted state file");
    let key = fs::read(&fixture.key_path).expect("master key");

    assert!(!contains(&encrypted, SENTINEL));
    assert!(!contains(&key, SENTINEL));
    assert!(!format!("{state:?}").contains(SENTINEL));
    assert_eq!(reference.source_turn_id(), &turn_id);
    assert_eq!(reference.binding_hash(), binding.binding_hash().unwrap());
    assert_eq!(reference.created_at_unix_millis(), 10_000);
    assert_eq!(reference.expires_at_unix_millis(), 11_000);

    let reopened = FileContinuityVault::open(&fixture.root, &fixture.key_path, TTL_MILLIS)
        .expect("reopen Continuity Vault after restart");
    let loaded = reopened
        .load(&binding, 10_500)
        .expect("load encrypted Continuity")
        .expect("compatible state");
    assert_eq!(loaded.reference, reference);
    assert_eq!(loaded.state.format(), ContinuityFormat::OpenAiResponses);
    assert!(!format!("{loaded:?}").contains(SENTINEL));
}

#[test]
fn incompatible_binding_is_rejected_and_purged() {
    let fixture = Fixture::new();
    let run_id = AgentRunId::new();
    let original = binding(run_id.clone(), "openai", "gpt-5.2", "responses");
    fixture
        .vault
        .store(&original, &InferenceTurnId::new(), &openai_state(), 20_000)
        .expect("store Continuity");

    let switched_model = binding(run_id.clone(), "openai", "gpt-5.3", "responses");
    assert!(
        fixture
            .vault
            .load(&switched_model, 20_100)
            .expect("binding mismatch is handled")
            .is_none()
    );
    assert!(!fixture.state_path(&run_id).exists());
}

#[test]
fn expired_and_terminal_run_state_is_removed() {
    let fixture = Fixture::new();
    let expired_run = AgentRunId::new();
    let active_run = AgentRunId::new();
    let expired = binding(
        expired_run.clone(),
        "anthropic",
        "claude-sonnet-4-6",
        "messages",
    );
    let active = binding(
        active_run.clone(),
        "anthropic",
        "claude-sonnet-4-6",
        "messages",
    );
    fixture
        .vault
        .store(
            &expired,
            &InferenceTurnId::new(),
            &anthropic_state(),
            30_000,
        )
        .expect("store expired fixture");
    fixture
        .vault
        .store(&active, &InferenceTurnId::new(), &anthropic_state(), 31_000)
        .expect("store active fixture");

    assert_eq!(
        fixture.vault.purge_expired(31_000).expect("purge expired"),
        1
    );
    assert!(!fixture.state_path(&expired_run).exists());
    assert!(fixture.state_path(&active_run).exists());

    fixture
        .vault
        .purge_run(&active_run)
        .expect("terminal purge");
    assert!(!fixture.state_path(&active_run).exists());
}

#[test]
fn corrupted_ciphertext_fails_closed_and_is_removed() {
    let fixture = Fixture::new();
    let run_id = AgentRunId::new();
    let binding = binding(run_id.clone(), "openai", "gpt-5.2", "responses");
    fixture
        .vault
        .store(&binding, &InferenceTurnId::new(), &openai_state(), 40_000)
        .expect("store Continuity");
    let state_path = fixture.state_path(&run_id);
    let mut envelope: Value =
        serde_json::from_slice(&fs::read(&state_path).expect("read envelope"))
            .expect("envelope JSON");
    let first = envelope["ciphertext"]
        .as_array_mut()
        .and_then(|bytes| bytes.first_mut())
        .expect("ciphertext byte");
    *first = json!((first.as_u64().unwrap_or_default() + 1) % 256);
    fs::write(&state_path, serde_json::to_vec(&envelope).unwrap()).expect("tamper envelope");

    let error = fixture
        .vault
        .load(&binding, 40_100)
        .expect_err("tampered ciphertext must fail closed");
    assert!(matches!(error, ContinuityVaultError::Corrupt));
    assert!(!state_path.exists());
}

#[cfg(unix)]
#[test]
fn vault_artifacts_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let run_id = AgentRunId::new();
    let binding = binding(run_id.clone(), "openai", "gpt-5.2", "responses");
    fixture
        .vault
        .store(&binding, &InferenceTurnId::new(), &openai_state(), 50_000)
        .expect("store Continuity");

    assert_eq!(
        fs::metadata(&fixture.root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&fixture.key_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(fixture.state_path(&run_id))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn production_open_rejects_vault_or_key_paths_inside_the_project() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project = temp.path().join("inside.autostudio");
    fs::create_dir_all(&project).expect("Project Package");
    let external = temp.path().join("app-state");
    fs::create_dir_all(&external).expect("application state");

    let vault_error = FileContinuityVault::open_for_project(
        project.join("continuity"),
        external.join("continuity.key"),
        &project,
        TTL_MILLIS,
    )
    .err()
    .expect("Vault payload cannot enter Project Package");
    assert!(matches!(vault_error, ContinuityVaultError::InsideProject));
    assert!(!project.join("continuity").exists());

    let key_error = FileContinuityVault::open_for_project(
        external.join("continuity"),
        project.join("continuity.key"),
        &project,
        TTL_MILLIS,
    )
    .err()
    .expect("Vault key cannot enter Project Package");
    assert!(matches!(key_error, ContinuityVaultError::InsideProject));
    assert!(!external.join("continuity").exists());
    assert!(!project.join("continuity.key").exists());
}

#[cfg(unix)]
#[test]
fn production_open_rejects_a_symlinked_path_into_the_project() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary directory");
    let project = temp.path().join("inside.autostudio");
    let external = temp.path().join("app-state");
    fs::create_dir_all(&project).expect("Project Package");
    fs::create_dir_all(&external).expect("application state");
    let alias = temp.path().join("project-alias");
    symlink(&project, &alias).expect("Project symlink");

    let error = FileContinuityVault::open_for_project(
        alias.join("continuity"),
        external.join("continuity.key"),
        &project,
        TTL_MILLIS,
    )
    .err()
    .expect("resolved Vault path cannot enter Project Package");
    assert!(matches!(error, ContinuityVaultError::InsideProject));
    assert!(!project.join("continuity").exists());
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
    key_path: std::path::PathBuf,
    vault: FileContinuityVault,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let root = temp.path().join("continuity");
        let key_path = temp.path().join("keys").join("continuity.key");
        let vault = FileContinuityVault::open(&root, &key_path, TTL_MILLIS)
            .expect("private Continuity Vault");
        Self {
            _temp: temp,
            root,
            key_path,
            vault,
        }
    }

    fn state_path(&self, run_id: &AgentRunId) -> std::path::PathBuf {
        self.root.join(format!("{}.continuity", run_id.as_str()))
    }
}

fn binding(
    run_id: AgentRunId,
    provider_kind: &str,
    model: &str,
    protocol: &str,
) -> ContinuityBinding {
    ContinuityBinding::new(
        run_id,
        ProviderBinding {
            provider_kind: provider_kind.to_owned(),
            model: model.to_owned(),
            protocol: protocol.to_owned(),
            thinking_level: ThinkingLevel::High,
            thinking_control: ThinkingControl::Effort,
            thinking_budget_tokens: None,
            capability_revision: "continuity-contract/1".to_owned(),
            mapping_revision: "continuity-mapping/1".to_owned(),
            tool_catalog_fingerprint: format!("sha256:{}", "a".repeat(64)),
        },
    )
    .expect("Continuity binding")
}

fn openai_state() -> ProviderContinuityState {
    ProviderContinuityState::from_json(
        ContinuityFormat::OpenAiResponses,
        &json!([{
            "type": "reasoning",
            "id": "rs_cm2",
            "encrypted_content": SENTINEL
        }]),
    )
    .expect("OpenAI Continuity fixture")
}

fn anthropic_state() -> ProviderContinuityState {
    ProviderContinuityState::from_json(
        ContinuityFormat::AnthropicMessages,
        &json!([{
            "type": "thinking",
            "thinking": "private fixture",
            "signature": SENTINEL
        }]),
    )
    .expect("Anthropic Continuity fixture")
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}
