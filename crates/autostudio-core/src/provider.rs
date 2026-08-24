//! Provider Connection application interface and secret-safe value types.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub use crate::error::ProviderConnectionError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmConnectionSource {
    PrivateFile,
    Environment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderDescriptor {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelDescriptor {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub thinking: ThinkingCapability,
}

/// Creator-selected reasoning preference for the active Agent model.
///
/// This value controls Provider request parameters. It never represents or
/// persists private chain-of-thought content.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
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

impl ThinkingLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderDefault => "provider_default",
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingControl {
    #[default]
    Unsupported,
    Toggle,
    Effort,
    AdaptiveEffort,
    TokenBudget,
}

/// Model-scoped reasoning controls that have been verified for one Provider
/// transport. `ProviderDefault` is used only when no adjustable control is
/// available; it is not an alias for `Off`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingCapability {
    pub control: ThinkingControl,
    pub levels: Vec<ThinkingLevel>,
    pub default_level: ThinkingLevel,
}

impl Default for ThinkingCapability {
    fn default() -> Self {
        Self::unsupported()
    }
}

impl ThinkingCapability {
    #[must_use]
    pub fn unsupported() -> Self {
        Self {
            control: ThinkingControl::Unsupported,
            levels: vec![ThinkingLevel::ProviderDefault],
            default_level: ThinkingLevel::ProviderDefault,
        }
    }

    #[must_use]
    pub fn new(
        control: ThinkingControl,
        levels: impl IntoIterator<Item = ThinkingLevel>,
        default_level: ThinkingLevel,
    ) -> Self {
        let levels = levels.into_iter().collect::<Vec<_>>();
        debug_assert!(levels.contains(&default_level));
        Self {
            control,
            levels,
            default_level,
        }
    }

    #[must_use]
    pub fn supports(&self, level: ThinkingLevel) -> bool {
        self.levels.contains(&level)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmModelCatalogState {
    #[default]
    NotLoaded,
    Refreshing,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelCatalog {
    pub state: LlmModelCatalogState,
    pub models: Vec<LlmModelDescriptor>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConnectionStatus {
    pub configured: bool,
    pub provider_kind: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "modelEffort")]
    pub thinking_level: ThinkingLevel,
    #[serde(default)]
    pub model_thinking_levels: BTreeMap<String, ThinkingLevel>,
    pub source: Option<LlmConnectionSource>,
    pub catalog: LlmModelCatalog,
}

impl LlmConnectionStatus {
    #[must_use]
    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            provider_kind: None,
            model: None,
            thinking_level: ThinkingLevel::default(),
            model_thinking_levels: BTreeMap::new(),
            source: None,
            catalog: LlmModelCatalog::default(),
        }
    }
}

pub type LlmModelCatalogFuture<'a> =
    Pin<Box<dyn Future<Output = Result<LlmModelCatalog, ProviderConnectionError>> + Send + 'a>>;

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct LlmConnectionConfiguration {
    provider_kind: String,
    model: Option<String>,
    base_url: Option<String>,
    api_key: String,
}

impl LlmConnectionConfiguration {
    #[must_use]
    pub fn new(
        provider_kind: impl Into<String>,
        model: Option<String>,
        base_url: Option<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            provider_kind: provider_kind.into(),
            model,
            base_url,
            api_key: api_key.into(),
        }
    }

    #[must_use]
    pub fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }
}

/// Application seam used by local clients to configure an LLM connection
/// without reading credentials back from the Core.
pub trait LlmConnectionControl: Send + Sync {
    /// Returns the Provider choices supported by this Core build.
    fn providers(&self) -> Vec<LlmProviderDescriptor>;

    /// Returns non-secret metadata for the currently resolved connection.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConnectionError`] when the backing connection store
    /// exists but cannot be validated or read safely.
    fn status(&self) -> Result<LlmConnectionStatus, ProviderConnectionError>;

    /// Validates and replaces the connection used by subsequent inference.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConnectionError`] when configuration validation fails
    /// or the backing credential store cannot publish the replacement safely.
    fn configure(
        &self,
        configuration: LlmConnectionConfiguration,
    ) -> Result<LlmConnectionStatus, ProviderConnectionError>;

    /// Returns the last durable model catalog without contacting the Provider.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConnectionError`] when connection state cannot be
    /// read safely.
    fn model_catalog(&self) -> Result<LlmModelCatalog, ProviderConnectionError>;

    /// Refreshes the configured Provider's model catalog and durably caches it.
    fn refresh_model_catalog(&self) -> LlmModelCatalogFuture<'_>;

    /// Atomically selects one model and its compute preference.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConnectionError`] when the model is absent from the
    /// current catalog or the selection cannot be stored safely.
    fn select_model(
        &self,
        model: &str,
        thinking_level: ThinkingLevel,
    ) -> Result<LlmConnectionStatus, ProviderConnectionError>;
}
