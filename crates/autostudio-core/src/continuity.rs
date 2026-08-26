//! Non-secret, Provider-independent continuity identity and binding vocabulary.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::agent::AgentRunId;
use crate::constants::CONTINUITY_BINDING_FORMAT_REVISION;
use crate::context::{InferenceTurnId, ProviderBinding};
pub use crate::error::ContinuityError;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ContinuityStateId(Uuid);

impl ContinuityStateId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parses an identity restored from the encrypted Vault envelope.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityError::InvalidId`] for malformed input.
    pub fn parse(value: &str) -> Result<Self, ContinuityError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ContinuityError::InvalidId)
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for ContinuityStateId {
    fn default() -> Self {
        Self::new()
    }
}

/// Exact Provider chain to which private continuity data belongs.
///
/// A change to Provider, model, protocol, thinking configuration, capability
/// mapping, or Tool Catalog produces a different binding hash.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuityBinding {
    run_id: AgentRunId,
    provider: ProviderBinding,
    format_revision: String,
}

impl ContinuityBinding {
    /// Creates a binding from the exact Provider binding frozen in a Context Manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityError`] when the Provider binding or revision is invalid.
    pub fn new(run_id: AgentRunId, provider: ProviderBinding) -> Result<Self, ContinuityError> {
        provider
            .validate()
            .map_err(|error| ContinuityError::Serialization(error.to_string()))?;
        let binding = Self {
            run_id,
            provider,
            format_revision: CONTINUITY_BINDING_FORMAT_REVISION.to_owned(),
        };
        binding.validate()?;
        Ok(binding)
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    #[must_use]
    pub const fn provider(&self) -> &ProviderBinding {
        &self.provider
    }

    #[must_use]
    pub fn format_revision(&self) -> &str {
        &self.format_revision
    }

    /// Computes the stable, non-secret hash used to invalidate incompatible state.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityError::Serialization`] if canonical JSON encoding fails.
    pub fn binding_hash(&self) -> Result<String, ContinuityError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| ContinuityError::Serialization(error.to_string()))?;
        Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
    }

    /// Revalidates a binding restored from a Vault envelope.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityError`] for invalid Provider fields or revision.
    pub fn validate(&self) -> Result<(), ContinuityError> {
        if self.format_revision.trim().is_empty() {
            return Err(ContinuityError::EmptyField("binding.format_revision"));
        }
        if self.format_revision != CONTINUITY_BINDING_FORMAT_REVISION {
            return Err(ContinuityError::Serialization(
                "unsupported Continuity binding revision".to_owned(),
            ));
        }
        self.provider
            .validate()
            .map_err(|error| ContinuityError::Serialization(error.to_string()))
    }
}

/// Non-secret receipt recorded in a Context Manifest when continuity is replayed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuityReference {
    state_id: ContinuityStateId,
    source_turn_id: InferenceTurnId,
    binding_hash: String,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
}

impl ContinuityReference {
    /// Creates a non-secret receipt for an encrypted Vault entry.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityError`] for an invalid digest or expiry interval.
    pub fn new(
        state_id: ContinuityStateId,
        source_turn_id: InferenceTurnId,
        binding_hash: String,
        created_at_unix_millis: u64,
        expires_at_unix_millis: u64,
    ) -> Result<Self, ContinuityError> {
        let reference = Self {
            state_id,
            source_turn_id,
            binding_hash,
            created_at_unix_millis,
            expires_at_unix_millis,
        };
        reference.validate()?;
        Ok(reference)
    }

    #[must_use]
    pub const fn state_id(&self) -> &ContinuityStateId {
        &self.state_id
    }

    #[must_use]
    pub const fn source_turn_id(&self) -> &InferenceTurnId {
        &self.source_turn_id
    }

    #[must_use]
    pub fn binding_hash(&self) -> &str {
        &self.binding_hash
    }

    #[must_use]
    pub const fn created_at_unix_millis(&self) -> u64 {
        self.created_at_unix_millis
    }

    #[must_use]
    pub const fn expires_at_unix_millis(&self) -> u64 {
        self.expires_at_unix_millis
    }

    /// Revalidates a reference restored from the Project context journal.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityError`] for an invalid digest or expiry interval.
    pub fn validate(&self) -> Result<(), ContinuityError> {
        require_digest(&self.binding_hash)?;
        if self.expires_at_unix_millis <= self.created_at_unix_millis {
            return Err(ContinuityError::InvalidExpiry);
        }
        Ok(())
    }
}

fn require_digest(value: &str) -> Result<(), ContinuityError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ContinuityError::InvalidDigest);
    };
    if hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ContinuityError::InvalidDigest)
    }
}
