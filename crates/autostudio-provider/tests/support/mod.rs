use std::sync::Arc;

use autostudio_core::agent::AgentRunId;
use autostudio_core::context::{
    CanonicalToolDefinition, InferenceTurnId, ProviderBinding, TokenBudgetPlan,
};
use autostudio_core::project::CreativeBrief;
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::InferenceTurnRequest;
use autostudio_provider::constants::{
    CONTEXT_SAFETY_MARGIN_TOKENS, PLAN_MAX_OUTPUT_TOKENS, PLAN_SCHEMA_JSON, PLAN_TOOL_DESCRIPTION,
    PLAN_TOOL_NAME,
};
use autostudio_provider::context::{ContextManager, PrepareContext, fingerprint_tool_catalog};
use sha2::{Digest, Sha256};

pub fn inference_request(
    brief: &CreativeBrief,
    store: Arc<autostudio_storage::SqliteProjectStore>,
) -> InferenceTurnRequest {
    let descriptor_bytes =
        serde_json::to_vec(&(PLAN_TOOL_NAME, PLAN_TOOL_DESCRIPTION, PLAN_SCHEMA_JSON))
            .expect("Tool descriptor JSON");
    let tool = CanonicalToolDefinition::new(
        PLAN_TOOL_NAME,
        PLAN_TOOL_DESCRIPTION,
        PLAN_SCHEMA_JSON,
        format!("sha256:{:x}", Sha256::digest(descriptor_bytes)),
    )
    .expect("canonical Tool");
    let tools = vec![tool];
    let manager = ContextManager::new(store);
    let prepared = manager
        .prepare_turn(PrepareContext {
            run_id: AgentRunId::new(),
            turn_id: InferenceTurnId::new(),
            project_id: "provider-contract-project".to_owned(),
            project_revision: 1,
            instructions: autostudio_provider::constants::PLAN_SYSTEM_PROMPT.to_owned(),
            new_user_messages: vec![serde_json::to_string(brief).expect("Creative Brief JSON")],
            provider_binding: ProviderBinding {
                provider_kind: "provider-contract".to_owned(),
                model: "provider-contract-model".to_owned(),
                protocol: "provider-contract-protocol".to_owned(),
                thinking_level: ThinkingLevel::High,
                thinking_control: ThinkingControl::Effort,
                thinking_budget_tokens: None,
                capability_revision: "provider-contract/1".to_owned(),
                mapping_revision: "provider-contract-mapping/1".to_owned(),
                tool_catalog_fingerprint: fingerprint_tool_catalog(&tools),
            },
            tools,
            token_budget: TokenBudgetPlan::unknown(
                u64::from(PLAN_MAX_OUTPUT_TOKENS),
                CONTEXT_SAFETY_MARGIN_TOKENS,
            ),
        })
        .expect("prepare Provider contract Context");
    InferenceTurnRequest { prepared }
}
