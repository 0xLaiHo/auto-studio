#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub name: String,
    pub revision: u64,
    pub brief: Option<CreativeBriefView>,
    pub agent_runs: Vec<AgentRunView>,
    pub candidates: Vec<CandidateView>,
    pub selection: Option<SelectionView>,
    pub exports: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeBriefView {
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunView {
    pub id: String,
    pub status: AgentRunStatusView,
    pub plan: Option<AgentPlanView>,
    pub failure: Option<AgentRunFailureView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatusView {
    Planning,
    AwaitingApproval,
    ReadyToSubmit,
    Submitting,
    Submitted,
    UnknownOutcome,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPlanView {
    pub visible_summary: String,
    pub input_hash: String,
    pub estimated_cost: CostEstimateView,
    pub inference: InferenceProvenanceView,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum CostEstimateView {
    Known {
        currency: String,
        lower_minor_units: u64,
        upper_minor_units: u64,
    },
    Unknown,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceProvenanceView {
    pub provider_kind: String,
    pub model: String,
    #[serde(rename = "modelEffort")]
    pub thinking_level: ThinkingLevelView,
    pub thinking_control: ThinkingControlView,
    pub thinking_budget_tokens: Option<u32>,
    pub capability_revision: String,
    pub mapping_revision: String,
    pub protocol: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentRunFailureView {
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CandidateView {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionView {
    pub candidate_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeBriefInput {
    pub summary: String,
    pub purpose: Option<String>,
    pub style: Vec<String>,
    pub mood: Vec<String>,
    pub instrumentation: Vec<String>,
    pub target_duration_seconds: Option<u32>,
    pub lyrics: Option<String>,
    pub constraints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalInput {
    pub currency: String,
    pub max_minor_units: u64,
    pub input_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmConnectionStatusView {
    pub configured: bool,
    pub provider_kind: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "modelEffort")]
    pub thinking_level: ThinkingLevelView,
    #[serde(default)]
    pub model_thinking_levels: BTreeMap<String, ThinkingLevelView>,
    pub source: Option<LlmConnectionSourceView>,
    pub catalog: LlmModelCatalogView,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevelView {
    #[default]
    ProviderDefault,
    Off,
    Minimal,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    XHigh,
    Max,
}

impl ThinkingLevelView {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ProviderDefault => "Provider default",
            Self::Off => "Off",
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::XHigh => "XHigh",
            Self::Max => "Max",
        }
    }

    #[must_use]
    pub const fn compact_label(self) -> &'static str {
        match self {
            Self::ProviderDefault => "default",
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingControlView {
    #[default]
    Unsupported,
    Toggle,
    Effort,
    AdaptiveEffort,
    TokenBudget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingCapabilityView {
    pub control: ThinkingControlView,
    pub levels: Vec<ThinkingLevelView>,
    pub default_level: ThinkingLevelView,
}

impl Default for ThinkingCapabilityView {
    fn default() -> Self {
        Self {
            control: ThinkingControlView::Unsupported,
            levels: vec![ThinkingLevelView::ProviderDefault],
            default_level: ThinkingLevelView::ProviderDefault,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderView {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelView {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub thinking: ThinkingCapabilityView,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelCatalogView {
    pub state: LlmModelCatalogStateView,
    pub models: Vec<LlmModelView>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LlmModelCatalogStateView {
    NotLoaded,
    Refreshing,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LlmConnectionSourceView {
    PrivateFile,
    Environment,
}

#[derive(Clone, Eq, PartialEq, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureLlmConnectionInput {
    pub provider_kind: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: String,
}

impl fmt::Debug for ConfigureLlmConnectionInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigureLlmConnectionInput")
            .field("provider_kind", &self.provider_kind)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthView {
    #[serde(rename = "status")]
    pub _status: String,
    #[serde(rename = "coreVersion")]
    pub _core_version: String,
    pub protocol_version: String,
    #[serde(rename = "schemaVersion")]
    pub _schema_version: String,
}
