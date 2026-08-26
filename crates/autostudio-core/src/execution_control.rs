//! Provider-independent Approval Grant and Run Budget contracts.
//!
//! This module owns pre-execution authorization and budget accounting. It does
//! not execute Tools or mutate a Music Project; the future Tool Runtime crosses
//! this seam before it creates a durable `ToolExecution`.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::agent::AgentRunId;
use crate::constants::{
    EXECUTION_CONTROL_FORMAT_REVISION, MAX_APPROVAL_CREATOR_ACTION_CHARS,
    MAX_APPROVAL_PROJECT_ID_CHARS, MAX_APPROVAL_SUBJECT_CHARS, MAX_APPROVAL_TARGET_CHARS,
    MAX_APPROVAL_TARGETS, MAX_BUDGET_LEDGER_ID_CHARS, MAX_CURRENCY_CHARS,
};
pub use crate::error::{
    ExecutionControlError, ExecutionControlManagerError, ExecutionControlStoreError,
    RunBudgetDimension, ToolResourceDimension,
};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ApprovalGrantId(Uuid);

impl ApprovalGrantId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    /// Parses an Approval Grant identity received through a transport.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError::InvalidId`] for a malformed UUID.
    pub fn parse(value: &str) -> Result<Self, ExecutionControlError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ExecutionControlError::InvalidId)
    }
}

impl Default for ApprovalGrantId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExecutionReservationId(Uuid);

impl ExecutionReservationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    /// Parses a durable pre-execution reservation identity.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError::InvalidId`] for a malformed UUID.
    pub fn parse(value: &str) -> Result<Self, ExecutionControlError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ExecutionControlError::InvalidId)
    }
}

impl Default for ExecutionReservationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalSubject {
    Plan { input_hash: String },
    AgentStep { step_id: String },
}

impl ApprovalSubject {
    fn validate(&self) -> Result<(), ExecutionControlError> {
        match self {
            Self::Plan { input_hash } => validate_digest(input_hash),
            Self::AgentStep { step_id } => validate_bounded(
                step_id,
                MAX_APPROVAL_SUBJECT_CHARS,
                "Approval Agent Step id",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    ReadOnly,
    ProjectMutation,
    AssetWrite,
    ExternalAction,
    ExternalCharge,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Money {
    currency: String,
    minor_units: u64,
}

impl Money {
    /// Creates one normalized monetary amount.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError::InvalidMoney`] when the currency is empty
    /// or unbounded.
    pub fn new(currency: &str, minor_units: u64) -> Result<Self, ExecutionControlError> {
        let currency = currency.trim().to_ascii_uppercase();
        if currency.is_empty() || currency.chars().count() > MAX_CURRENCY_CHARS {
            return Err(ExecutionControlError::InvalidMoney);
        }
        Ok(Self {
            currency,
            minor_units,
        })
    }

    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }

    #[must_use]
    pub const fn minor_units(&self) -> u64 {
        self.minor_units
    }

    fn validate(&self) -> Result<(), ExecutionControlError> {
        if Self::new(&self.currency, self.minor_units)? == *self {
            Ok(())
        } else {
            Err(ExecutionControlError::InvalidMoney)
        }
    }

    fn checked_add(&self, other: &Self) -> Result<Self, ExecutionControlError> {
        if self.currency != other.currency {
            return Err(ExecutionControlError::InvalidMoney);
        }
        Self::new(
            &self.currency,
            self.minor_units
                .checked_add(other.minor_units)
                .ok_or(ExecutionControlError::NumericOverflow)?,
        )
    }

    fn is_within(&self, limit: &Self) -> bool {
        self.currency == limit.currency && self.minor_units <= limit.minor_units
    }
}

#[derive(Clone, Debug)]
pub struct ApprovalGrantDraft {
    pub creator_action_id: String,
    pub run_id: AgentRunId,
    pub project_id: String,
    pub project_revision: u64,
    pub subject: ApprovalSubject,
    pub tool_descriptor_fingerprint: String,
    pub targets: Vec<String>,
    pub side_effect_class: SideEffectClass,
    pub max_effects: u64,
    pub max_cost: Option<Money>,
    pub issued_at_unix_millis: u64,
    pub expires_at_unix_millis: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGrant {
    id: ApprovalGrantId,
    creator_action_id: String,
    run_id: AgentRunId,
    project_id: String,
    project_revision: u64,
    subject: ApprovalSubject,
    tool_descriptor_fingerprint: String,
    targets: Vec<String>,
    side_effect_class: SideEffectClass,
    max_effects: u64,
    max_cost: Option<Money>,
    issued_at_unix_millis: u64,
    expires_at_unix_millis: Option<u64>,
}

impl ApprovalGrant {
    /// Issues one immutable Grant for an exact Run, Project revision, subject,
    /// Tool fingerprint, target set, side-effect class, quantity, and cost scope.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] when any binding is absent or inconsistent.
    pub fn issue(
        id: ApprovalGrantId,
        draft: ApprovalGrantDraft,
    ) -> Result<Self, ExecutionControlError> {
        let mut grant = Self {
            id,
            creator_action_id: draft.creator_action_id.trim().to_owned(),
            run_id: draft.run_id,
            project_id: draft.project_id.trim().to_owned(),
            project_revision: draft.project_revision,
            subject: draft.subject,
            tool_descriptor_fingerprint: draft.tool_descriptor_fingerprint,
            targets: draft
                .targets
                .into_iter()
                .map(|target| target.trim().to_owned())
                .collect(),
            side_effect_class: draft.side_effect_class,
            max_effects: draft.max_effects,
            max_cost: draft.max_cost,
            issued_at_unix_millis: draft.issued_at_unix_millis,
            expires_at_unix_millis: draft.expires_at_unix_millis,
        };
        grant.targets.sort();
        grant.validate()?;
        Ok(grant)
    }

    #[must_use]
    pub const fn id(&self) -> &ApprovalGrantId {
        &self.id
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn targets(&self) -> &[String] {
        &self.targets
    }

    fn validate(&self) -> Result<(), ExecutionControlError> {
        ApprovalGrantId::parse(&self.id.as_str())?;
        validate_bounded(
            &self.creator_action_id,
            MAX_APPROVAL_CREATOR_ACTION_CHARS,
            "Approval creator action id",
        )?;
        AgentRunId::parse(&self.run_id.as_str()).map_err(|_| ExecutionControlError::InvalidId)?;
        validate_bounded(
            &self.project_id,
            MAX_APPROVAL_PROJECT_ID_CHARS,
            "Approval Project id",
        )?;
        self.subject.validate()?;
        validate_digest(&self.tool_descriptor_fingerprint)?;
        validate_targets(&self.targets)?;
        if self.side_effect_class == SideEffectClass::ReadOnly && self.max_effects != 0 {
            return Err(ExecutionControlError::InvalidGrant);
        }
        if self.side_effect_class != SideEffectClass::ReadOnly && self.max_effects == 0 {
            return Err(ExecutionControlError::InvalidGrant);
        }
        if let Some(cost) = &self.max_cost {
            cost.validate()?;
        }
        if self.side_effect_class == SideEffectClass::ExternalCharge
            && self
                .max_cost
                .as_ref()
                .is_none_or(|cost| cost.minor_units == 0)
        {
            return Err(ExecutionControlError::InvalidGrant);
        }
        if self
            .expires_at_unix_millis
            .is_some_and(|expires| expires <= self.issued_at_unix_millis)
        {
            return Err(ExecutionControlError::InvalidGrant);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RunBudgetLimitsDraft {
    pub max_inference_turns: u64,
    pub max_tool_executions: u64,
    pub max_tokens: u64,
    pub max_cost: Money,
    pub max_wall_clock_millis: u64,
    pub max_preview_renders: u64,
    pub max_side_effects: u64,
    pub max_asset_bytes: u64,
    pub max_concurrent_tools: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
pub struct RunBudgetLimits {
    max_inference_turns: u64,
    max_tool_executions: u64,
    max_tokens: u64,
    max_cost: Money,
    max_wall_clock_millis: u64,
    max_preview_renders: u64,
    max_side_effects: u64,
    max_asset_bytes: u64,
    max_concurrent_tools: u64,
}

impl RunBudgetLimits {
    /// Creates an explicit Run ceiling. Zero is a valid deny-all limit.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] when the monetary dimension is invalid.
    pub fn new(draft: RunBudgetLimitsDraft) -> Result<Self, ExecutionControlError> {
        draft.max_cost.validate()?;
        Ok(Self {
            max_inference_turns: draft.max_inference_turns,
            max_tool_executions: draft.max_tool_executions,
            max_tokens: draft.max_tokens,
            max_cost: draft.max_cost,
            max_wall_clock_millis: draft.max_wall_clock_millis,
            max_preview_renders: draft.max_preview_renders,
            max_side_effects: draft.max_side_effects,
            max_asset_bytes: draft.max_asset_bytes,
            max_concurrent_tools: draft.max_concurrent_tools,
        })
    }

    #[must_use]
    pub const fn max_inference_turns(&self) -> u64 {
        self.max_inference_turns
    }

    #[must_use]
    pub const fn max_tool_executions(&self) -> u64 {
        self.max_tool_executions
    }

    #[must_use]
    pub const fn max_tokens(&self) -> u64 {
        self.max_tokens
    }

    #[must_use]
    pub const fn max_cost(&self) -> &Money {
        &self.max_cost
    }

    fn validate(&self) -> Result<(), ExecutionControlError> {
        self.max_cost.validate()
    }

    fn first_excess(&self, ceiling: &Self) -> Option<RunBudgetDimension> {
        if self.max_inference_turns > ceiling.max_inference_turns {
            Some(RunBudgetDimension::InferenceTurns)
        } else if self.max_tool_executions > ceiling.max_tool_executions {
            Some(RunBudgetDimension::ToolExecutions)
        } else if self.max_tokens > ceiling.max_tokens {
            Some(RunBudgetDimension::Tokens)
        } else if !self.max_cost.is_within(&ceiling.max_cost) {
            Some(RunBudgetDimension::Cost)
        } else if self.max_wall_clock_millis > ceiling.max_wall_clock_millis {
            Some(RunBudgetDimension::WallClock)
        } else if self.max_preview_renders > ceiling.max_preview_renders {
            Some(RunBudgetDimension::PreviewRenders)
        } else if self.max_side_effects > ceiling.max_side_effects {
            Some(RunBudgetDimension::SideEffects)
        } else if self.max_asset_bytes > ceiling.max_asset_bytes {
            Some(RunBudgetDimension::AssetBytes)
        } else if self.max_concurrent_tools > ceiling.max_concurrent_tools {
            Some(RunBudgetDimension::ConcurrentTools)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceBudgetCharge {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub wall_clock_millis: u64,
    pub cost: Money,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolBudgetCharge {
    pub side_effects: u64,
    pub preview_renders: u64,
    pub asset_bytes: u64,
    pub wall_clock_millis: u64,
    pub cost: Money,
}

impl ToolBudgetCharge {
    fn is_within(&self, reserved: &Self) -> bool {
        self.side_effects <= reserved.side_effects
            && self.preview_renders <= reserved.preview_renders
            && self.asset_bytes <= reserved.asset_bytes
            && self.wall_clock_millis <= reserved.wall_clock_millis
            && self.cost.is_within(&reserved.cost)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToolResourceLimitDraft {
    pub max_input_bytes: u64,
    pub max_target_count: u64,
    pub max_cpu_millis: u64,
    pub max_memory_bytes: u64,
    pub max_output_bytes: u64,
    pub deadline_millis: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResourceLimit {
    max_input_bytes: u64,
    max_target_count: u64,
    max_cpu_millis: u64,
    max_memory_bytes: u64,
    max_output_bytes: u64,
    deadline_millis: u64,
}

impl ToolResourceLimit {
    #[must_use]
    pub const fn new(draft: ToolResourceLimitDraft) -> Self {
        Self {
            max_input_bytes: draft.max_input_bytes,
            max_target_count: draft.max_target_count,
            max_cpu_millis: draft.max_cpu_millis,
            max_memory_bytes: draft.max_memory_bytes,
            max_output_bytes: draft.max_output_bytes,
            deadline_millis: draft.deadline_millis,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResourceUsage {
    pub input_bytes: u64,
    pub target_count: u64,
    pub cpu_millis: u64,
    pub memory_bytes: u64,
    pub output_bytes: u64,
    pub deadline_millis: u64,
}

impl ToolResourceUsage {
    fn first_excess(&self, limit: &ToolResourceLimit) -> Option<ToolResourceDimension> {
        if self.input_bytes > limit.max_input_bytes {
            Some(ToolResourceDimension::InputBytes)
        } else if self.target_count > limit.max_target_count {
            Some(ToolResourceDimension::TargetCount)
        } else if self.cpu_millis > limit.max_cpu_millis {
            Some(ToolResourceDimension::CpuMillis)
        } else if self.memory_bytes > limit.max_memory_bytes {
            Some(ToolResourceDimension::MemoryBytes)
        } else if self.output_bytes > limit.max_output_bytes {
            Some(ToolResourceDimension::OutputBytes)
        } else if self.deadline_millis > limit.deadline_millis {
            Some(ToolResourceDimension::DeadlineMillis)
        } else {
            None
        }
    }

    fn is_within(&self, reserved: &Self) -> bool {
        self.input_bytes <= reserved.input_bytes
            && self.target_count <= reserved.target_count
            && self.cpu_millis <= reserved.cpu_millis
            && self.memory_bytes <= reserved.memory_bytes
            && self.output_bytes <= reserved.output_bytes
            && self.deadline_millis <= reserved.deadline_millis
    }
}

#[derive(Clone, Debug)]
pub struct ToolExecutionClaimDraft {
    pub grant_id: ApprovalGrantId,
    pub run_id: AgentRunId,
    pub project_id: String,
    pub project_revision: u64,
    pub subject: ApprovalSubject,
    pub tool_descriptor_fingerprint: String,
    pub targets: Vec<String>,
    pub side_effect_class: SideEffectClass,
    pub budget_charge: ToolBudgetCharge,
    pub resources: ToolResourceUsage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionClaim {
    grant_id: ApprovalGrantId,
    run_id: AgentRunId,
    project_id: String,
    project_revision: u64,
    subject: ApprovalSubject,
    tool_descriptor_fingerprint: String,
    targets: Vec<String>,
    side_effect_class: SideEffectClass,
    budget_charge: ToolBudgetCharge,
    resources: ToolResourceUsage,
}

impl ToolExecutionClaim {
    /// Creates a canonical pre-execution claim.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] when a binding or target is invalid.
    pub fn new(draft: ToolExecutionClaimDraft) -> Result<Self, ExecutionControlError> {
        let mut claim = Self {
            grant_id: draft.grant_id,
            run_id: draft.run_id,
            project_id: draft.project_id.trim().to_owned(),
            project_revision: draft.project_revision,
            subject: draft.subject,
            tool_descriptor_fingerprint: draft.tool_descriptor_fingerprint,
            targets: draft
                .targets
                .into_iter()
                .map(|target| target.trim().to_owned())
                .collect(),
            side_effect_class: draft.side_effect_class,
            budget_charge: draft.budget_charge,
            resources: draft.resources,
        };
        claim.targets.sort();
        claim.validate()?;
        Ok(claim)
    }

    #[must_use]
    pub const fn grant_id(&self) -> &ApprovalGrantId {
        &self.grant_id
    }

    fn validate(&self) -> Result<(), ExecutionControlError> {
        ApprovalGrantId::parse(&self.grant_id.as_str())?;
        AgentRunId::parse(&self.run_id.as_str()).map_err(|_| ExecutionControlError::InvalidId)?;
        validate_bounded(
            &self.project_id,
            MAX_APPROVAL_PROJECT_ID_CHARS,
            "Tool claim Project id",
        )?;
        self.subject.validate()?;
        validate_digest(&self.tool_descriptor_fingerprint)?;
        validate_targets(&self.targets)?;
        self.budget_charge.cost.validate()?;
        if self.side_effect_class == SideEffectClass::ReadOnly
            && self.budget_charge.side_effects != 0
        {
            return Err(ExecutionControlError::InvalidGrant);
        }
        if self.budget_charge.wall_clock_millis > self.resources.deadline_millis {
            return Err(ExecutionControlError::InvalidBudget);
        }
        Ok(())
    }

    fn fingerprint(&self) -> Result<String, ExecutionControlError> {
        let bytes =
            serde_json::to_vec(self).map_err(|_| ExecutionControlError::CorruptRestoredState)?;
        Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSettlement {
    pub budget_charge: ToolBudgetCharge,
    pub resources: ToolResourceUsage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationStatus {
    Reserved,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolBudgetReservation {
    id: ExecutionReservationId,
    claim_fingerprint: String,
    claim: ToolExecutionClaim,
    resource_limit: ToolResourceLimit,
    requested_at_unix_millis: u64,
    status: ReservationStatus,
    settlement: Option<ToolSettlement>,
    finished_at_unix_millis: Option<u64>,
}

impl ToolBudgetReservation {
    #[must_use]
    pub const fn id(&self) -> &ExecutionReservationId {
        &self.id
    }

    #[must_use]
    pub const fn status(&self) -> ReservationStatus {
        self.status
    }

    #[must_use]
    pub const fn claim(&self) -> &ToolExecutionClaim {
        &self.claim
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordedInferenceCharge {
    turn_id: String,
    charge: InferenceBudgetCharge,
    recorded_at_unix_millis: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBudgetUsage {
    inference_turns: u64,
    tool_executions: u64,
    tokens: u64,
    cost: Money,
    wall_clock_millis: u64,
    preview_renders: u64,
    side_effects: u64,
    asset_bytes: u64,
    concurrent_tools: u64,
    peak_concurrent_tools: u64,
}

impl RunBudgetUsage {
    #[must_use]
    pub const fn inference_turns(&self) -> u64 {
        self.inference_turns
    }

    #[must_use]
    pub const fn tool_executions(&self) -> u64 {
        self.tool_executions
    }

    #[must_use]
    pub const fn tokens(&self) -> u64 {
        self.tokens
    }

    #[must_use]
    pub const fn cost(&self) -> &Money {
        &self.cost
    }

    #[must_use]
    pub const fn concurrent_tools(&self) -> u64 {
        self.concurrent_tools
    }

    #[must_use]
    pub const fn wall_clock_millis(&self) -> u64 {
        self.wall_clock_millis
    }

    #[must_use]
    pub const fn preview_renders(&self) -> u64 {
        self.preview_renders
    }

    #[must_use]
    pub const fn side_effects(&self) -> u64 {
        self.side_effects
    }

    #[must_use]
    pub const fn asset_bytes(&self) -> u64 {
        self.asset_bytes
    }

    #[must_use]
    pub const fn peak_concurrent_tools(&self) -> u64 {
        self.peak_concurrent_tools
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionControl {
    run_id: AgentRunId,
    configured_budget: RunBudgetLimits,
    system_ceiling: RunBudgetLimits,
    started_at_unix_millis: u64,
    grants: Vec<ApprovalGrant>,
    inference_charges: Vec<RecordedInferenceCharge>,
    reservations: Vec<ToolBudgetReservation>,
    peak_concurrent_tools: u64,
    format_revision: String,
}

impl ExecutionControl {
    /// Creates one Run-local execution control ledger. The configured budget may
    /// be stricter than, but never exceed, the host-owned system ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] for invalid or elevated limits.
    pub fn new(
        run_id: AgentRunId,
        configured_budget: RunBudgetLimits,
        system_ceiling: RunBudgetLimits,
        started_at_unix_millis: u64,
    ) -> Result<Self, ExecutionControlError> {
        AgentRunId::parse(&run_id.as_str()).map_err(|_| ExecutionControlError::InvalidId)?;
        configured_budget.validate()?;
        system_ceiling.validate()?;
        if let Some(dimension) = configured_budget.first_excess(&system_ceiling) {
            return Err(ExecutionControlError::ConfiguredBudgetExceedsSystemCeiling { dimension });
        }
        Ok(Self {
            run_id,
            configured_budget,
            system_ceiling,
            started_at_unix_millis,
            grants: Vec::new(),
            inference_charges: Vec::new(),
            reservations: Vec::new(),
            peak_concurrent_tools: 0,
            format_revision: EXECUTION_CONTROL_FORMAT_REVISION.to_owned(),
        })
    }

    /// Adds an immutable Creator-issued Grant. Replaying the same Grant is
    /// idempotent; reusing its identity with different content fails closed.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] for a wrong Run or conflicting identity.
    pub fn issue_grant(&mut self, grant: ApprovalGrant) -> Result<(), ExecutionControlError> {
        grant.validate()?;
        if grant.run_id != self.run_id {
            return Err(ExecutionControlError::GrantBindingMismatch);
        }
        if let Some(existing) = self.grants.iter().find(|item| item.id == grant.id) {
            return if existing == &grant {
                Ok(())
            } else {
                Err(ExecutionControlError::IdentityConflict)
            };
        }
        self.grants.push(grant);
        Ok(())
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    /// Charges one completed Inference Turn exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] when identity, currency, wall clock, or
    /// Run Budget would be exceeded.
    pub fn record_inference(
        &mut self,
        turn_id: &str,
        charge: InferenceBudgetCharge,
        recorded_at_unix_millis: u64,
    ) -> Result<RunBudgetUsage, ExecutionControlError> {
        validate_bounded(turn_id, MAX_BUDGET_LEDGER_ID_CHARS, "Inference Turn id")?;
        charge.cost.validate()?;
        if let Some(existing) = self
            .inference_charges
            .iter()
            .find(|item| item.turn_id == turn_id)
        {
            return if existing.charge == charge {
                self.usage()
            } else {
                Err(ExecutionControlError::IdentityConflict)
            };
        }
        if recorded_at_unix_millis < self.started_at_unix_millis {
            return Err(ExecutionControlError::InvalidBudget);
        }
        if charge.cost.currency != self.configured_budget.max_cost.currency {
            return Err(ExecutionControlError::RunBudgetExceeded {
                dimension: RunBudgetDimension::Cost,
            });
        }
        let mut usage = self.usage()?;
        usage.inference_turns = checked_add(usage.inference_turns, 1)?;
        usage.tokens = checked_add(
            usage.tokens,
            checked_add(charge.input_tokens, charge.output_tokens)?,
        )?;
        usage.cost = usage.cost.checked_add(&charge.cost)?;
        usage.wall_clock_millis = checked_add(usage.wall_clock_millis, charge.wall_clock_millis)?;
        self.enforce_budget(&usage)?;
        self.inference_charges.push(RecordedInferenceCharge {
            turn_id: turn_id.to_owned(),
            charge,
            recorded_at_unix_millis,
        });
        Ok(usage)
    }

    /// Authorizes and reserves one Tool execution against Grant, Run Budget, and
    /// Tool Resource Limit in that order. A matching replay does not double-charge.
    ///
    /// # Errors
    ///
    /// Returns the precise failed control class without mutating the ledger.
    pub fn authorize_tool(
        &mut self,
        id: ExecutionReservationId,
        claim: ToolExecutionClaim,
        resource_limit: ToolResourceLimit,
        requested_at_unix_millis: u64,
    ) -> Result<ToolBudgetReservation, ExecutionControlError> {
        claim.validate()?;
        let claim_fingerprint = claim.fingerprint()?;
        if let Some(existing) = self.reservations.iter().find(|item| item.id == id) {
            return if existing.claim_fingerprint == claim_fingerprint
                && existing.resource_limit == resource_limit
            {
                Ok(existing.clone())
            } else {
                Err(ExecutionControlError::IdentityConflict)
            };
        }
        if claim.run_id != self.run_id {
            return Err(ExecutionControlError::GrantBindingMismatch);
        }
        if requested_at_unix_millis < self.started_at_unix_millis {
            return Err(ExecutionControlError::InvalidBudget);
        }
        if claim.budget_charge.cost.currency != self.configured_budget.max_cost.currency {
            return Err(ExecutionControlError::RunBudgetExceeded {
                dimension: RunBudgetDimension::Cost,
            });
        }
        let grant = self
            .grants
            .iter()
            .find(|grant| grant.id == claim.grant_id)
            .ok_or(ExecutionControlError::GrantNotFound)?;
        self.enforce_grant(grant, &claim, requested_at_unix_millis)?;
        let mut usage = self.usage()?;
        usage.tool_executions = checked_add(usage.tool_executions, 1)?;
        usage.side_effects = checked_add(usage.side_effects, claim.budget_charge.side_effects)?;
        usage.preview_renders =
            checked_add(usage.preview_renders, claim.budget_charge.preview_renders)?;
        usage.asset_bytes = checked_add(usage.asset_bytes, claim.budget_charge.asset_bytes)?;
        usage.cost = usage.cost.checked_add(&claim.budget_charge.cost)?;
        usage.wall_clock_millis = checked_add(
            usage.wall_clock_millis,
            claim.budget_charge.wall_clock_millis,
        )?;
        usage.concurrent_tools = checked_add(usage.concurrent_tools, 1)?;
        usage.peak_concurrent_tools = usage.peak_concurrent_tools.max(usage.concurrent_tools);
        self.enforce_budget(&usage)?;
        if let Some(dimension) = claim.resources.first_excess(&resource_limit) {
            return Err(ExecutionControlError::ToolResourceExceeded { dimension });
        }
        let reservation = ToolBudgetReservation {
            id,
            claim_fingerprint,
            claim,
            resource_limit,
            requested_at_unix_millis,
            status: ReservationStatus::Reserved,
            settlement: None,
            finished_at_unix_millis: None,
        };
        self.reservations.push(reservation.clone());
        self.peak_concurrent_tools = usage.peak_concurrent_tools;
        Ok(reservation)
    }

    /// Settles a reservation with actual usage no greater than its authorized
    /// upper bound. Matching replay is idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] for missing, cancelled, conflicting, or
    /// overrun settlements.
    pub fn settle_tool(
        &mut self,
        id: &ExecutionReservationId,
        settlement: ToolSettlement,
        completed_at_unix_millis: u64,
    ) -> Result<ToolBudgetReservation, ExecutionControlError> {
        settlement.budget_charge.cost.validate()?;
        let index = self
            .reservations
            .iter()
            .position(|item| &item.id == id)
            .ok_or(ExecutionControlError::ReservationNotFound)?;
        let reservation = &self.reservations[index];
        match reservation.status {
            ReservationStatus::Completed => {
                return if reservation.settlement.as_ref() == Some(&settlement) {
                    Ok(reservation.clone())
                } else {
                    Err(ExecutionControlError::IdentityConflict)
                };
            }
            ReservationStatus::Cancelled => {
                return Err(ExecutionControlError::InvalidReservationTransition);
            }
            ReservationStatus::Reserved => {}
        }
        if completed_at_unix_millis < reservation.requested_at_unix_millis {
            return Err(ExecutionControlError::InvalidReservationTransition);
        }
        if !settlement
            .budget_charge
            .is_within(&reservation.claim.budget_charge)
            || !settlement.resources.is_within(&reservation.claim.resources)
        {
            return Err(ExecutionControlError::SettlementExceedsReservation);
        }
        let reservation = &mut self.reservations[index];
        reservation.status = ReservationStatus::Completed;
        reservation.settlement = Some(settlement);
        reservation.finished_at_unix_millis = Some(completed_at_unix_millis);
        Ok(reservation.clone())
    }

    /// Cancels a reservation only after the caller has established that execution
    /// never started or produced no effect. An unknown outcome must remain reserved
    /// for reconciliation. The Tool execution count remains consumed, while the
    /// confirmed-unused effects, assets, renders, and cost return.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] for missing or completed reservations.
    pub fn cancel_tool(
        &mut self,
        id: &ExecutionReservationId,
        cancelled_at_unix_millis: u64,
    ) -> Result<ToolBudgetReservation, ExecutionControlError> {
        let reservation = self
            .reservations
            .iter_mut()
            .find(|item| &item.id == id)
            .ok_or(ExecutionControlError::ReservationNotFound)?;
        match reservation.status {
            ReservationStatus::Cancelled => return Ok(reservation.clone()),
            ReservationStatus::Completed => {
                return Err(ExecutionControlError::InvalidReservationTransition);
            }
            ReservationStatus::Reserved => {}
        }
        if cancelled_at_unix_millis < reservation.requested_at_unix_millis {
            return Err(ExecutionControlError::InvalidReservationTransition);
        }
        reservation.status = ReservationStatus::Cancelled;
        reservation.settlement = None;
        reservation.finished_at_unix_millis = Some(cancelled_at_unix_millis);
        Ok(reservation.clone())
    }

    #[must_use]
    pub fn grants(&self) -> &[ApprovalGrant] {
        &self.grants
    }

    #[must_use]
    pub fn reservations(&self) -> &[ToolBudgetReservation] {
        &self.reservations
    }

    /// Returns the conservative current ledger usage. Active reservations count
    /// at their authorized upper bound; completed entries use actual settlement.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError::NumericOverflow`] for impossible totals.
    pub fn usage(&self) -> Result<RunBudgetUsage, ExecutionControlError> {
        let currency = self.configured_budget.max_cost.currency();
        let mut usage = RunBudgetUsage {
            inference_turns: 0,
            tool_executions: 0,
            tokens: 0,
            cost: Money::new(currency, 0)?,
            wall_clock_millis: 0,
            preview_renders: 0,
            side_effects: 0,
            asset_bytes: 0,
            concurrent_tools: 0,
            peak_concurrent_tools: self.peak_concurrent_tools,
        };
        for entry in &self.inference_charges {
            usage.inference_turns = checked_add(usage.inference_turns, 1)?;
            usage.tokens = checked_add(
                usage.tokens,
                checked_add(entry.charge.input_tokens, entry.charge.output_tokens)?,
            )?;
            usage.cost = usage.cost.checked_add(&entry.charge.cost)?;
            usage.wall_clock_millis =
                checked_add(usage.wall_clock_millis, entry.charge.wall_clock_millis)?;
        }
        for reservation in &self.reservations {
            usage.tool_executions = checked_add(usage.tool_executions, 1)?;
            let charge = match reservation.status {
                ReservationStatus::Reserved => {
                    usage.concurrent_tools = checked_add(usage.concurrent_tools, 1)?;
                    Some(&reservation.claim.budget_charge)
                }
                ReservationStatus::Completed => reservation
                    .settlement
                    .as_ref()
                    .map(|settlement| &settlement.budget_charge),
                ReservationStatus::Cancelled => None,
            };
            if let Some(charge) = charge {
                usage.side_effects = checked_add(usage.side_effects, charge.side_effects)?;
                usage.preview_renders = checked_add(usage.preview_renders, charge.preview_renders)?;
                usage.asset_bytes = checked_add(usage.asset_bytes, charge.asset_bytes)?;
                usage.cost = usage.cost.checked_add(&charge.cost)?;
                usage.wall_clock_millis =
                    checked_add(usage.wall_clock_millis, charge.wall_clock_millis)?;
            }
        }
        Ok(usage)
    }

    /// Revalidates a deserialized ledger without trusting stored counters or
    /// fingerprints.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] for unsupported, inconsistent, or
    /// over-budget restored state.
    pub fn validate_restored(&self) -> Result<(), ExecutionControlError> {
        if self.format_revision != EXECUTION_CONTROL_FORMAT_REVISION {
            return Err(ExecutionControlError::CorruptRestoredState);
        }
        AgentRunId::parse(&self.run_id.as_str()).map_err(|_| ExecutionControlError::InvalidId)?;
        self.configured_budget.validate()?;
        self.system_ceiling.validate()?;
        if self
            .configured_budget
            .first_excess(&self.system_ceiling)
            .is_some()
        {
            return Err(ExecutionControlError::CorruptRestoredState);
        }
        self.validate_restored_grants()?;
        self.validate_restored_inference()?;
        self.validate_restored_reservations()?;
        self.validate_restored_grant_usage()?;
        let usage = self.usage()?;
        if usage.concurrent_tools > self.peak_concurrent_tools {
            return Err(ExecutionControlError::CorruptRestoredState);
        }
        self.enforce_budget(&usage)
            .map_err(|_| ExecutionControlError::CorruptRestoredState)
    }

    fn validate_restored_grants(&self) -> Result<(), ExecutionControlError> {
        let mut grant_ids = HashSet::new();
        for grant in &self.grants {
            grant.validate()?;
            if grant.run_id != self.run_id || !grant_ids.insert(&grant.id) {
                return Err(ExecutionControlError::CorruptRestoredState);
            }
        }
        Ok(())
    }

    fn validate_restored_inference(&self) -> Result<(), ExecutionControlError> {
        let mut turn_ids = HashSet::new();
        for entry in &self.inference_charges {
            validate_bounded(
                &entry.turn_id,
                MAX_BUDGET_LEDGER_ID_CHARS,
                "Inference Turn id",
            )?;
            entry.charge.cost.validate()?;
            if entry.recorded_at_unix_millis < self.started_at_unix_millis
                || !turn_ids.insert(&entry.turn_id)
            {
                return Err(ExecutionControlError::CorruptRestoredState);
            }
        }
        Ok(())
    }

    fn validate_restored_reservations(&self) -> Result<(), ExecutionControlError> {
        let mut reservation_ids = HashSet::new();
        for reservation in &self.reservations {
            ExecutionReservationId::parse(&reservation.id.as_str())?;
            reservation.claim.validate()?;
            let grant = self
                .grants
                .iter()
                .find(|grant| grant.id == reservation.claim.grant_id)
                .ok_or(ExecutionControlError::CorruptRestoredState)?;
            if !reservation_ids.insert(&reservation.id)
                || reservation.claim.run_id != self.run_id
                || reservation.claim.fingerprint()? != reservation.claim_fingerprint
                || reservation.requested_at_unix_millis < self.started_at_unix_millis
                || reservation
                    .claim
                    .resources
                    .first_excess(&reservation.resource_limit)
                    .is_some()
            {
                return Err(ExecutionControlError::CorruptRestoredState);
            }
            Self::validate_grant_binding(
                grant,
                &reservation.claim,
                reservation.requested_at_unix_millis,
            )
            .map_err(|_| ExecutionControlError::CorruptRestoredState)?;
            Self::validate_restored_reservation_status(reservation)?;
        }
        Ok(())
    }

    fn validate_restored_reservation_status(
        reservation: &ToolBudgetReservation,
    ) -> Result<(), ExecutionControlError> {
        let has_valid_finished_at = reservation
            .finished_at_unix_millis
            .is_some_and(|finished| finished >= reservation.requested_at_unix_millis);
        let valid = match reservation.status {
            ReservationStatus::Reserved => {
                reservation.settlement.is_none() && reservation.finished_at_unix_millis.is_none()
            }
            ReservationStatus::Completed => reservation.settlement.as_ref().is_some_and(|actual| {
                actual
                    .budget_charge
                    .is_within(&reservation.claim.budget_charge)
                    && actual.resources.is_within(&reservation.claim.resources)
                    && has_valid_finished_at
            }),
            ReservationStatus::Cancelled => {
                reservation.settlement.is_none() && has_valid_finished_at
            }
        };
        if valid {
            Ok(())
        } else {
            Err(ExecutionControlError::CorruptRestoredState)
        }
    }

    fn validate_restored_grant_usage(&self) -> Result<(), ExecutionControlError> {
        for grant in &self.grants {
            let (effects, cost) = self
                .grant_usage(grant)
                .map_err(|_| ExecutionControlError::CorruptRestoredState)?;
            if effects > grant.max_effects
                || cost
                    .as_ref()
                    .zip(grant.max_cost.as_ref())
                    .is_some_and(|(used, limit)| !used.is_within(limit))
            {
                return Err(ExecutionControlError::CorruptRestoredState);
            }
        }
        Ok(())
    }

    fn enforce_grant(
        &self,
        grant: &ApprovalGrant,
        claim: &ToolExecutionClaim,
        now: u64,
    ) -> Result<(), ExecutionControlError> {
        Self::validate_grant_binding(grant, claim, now)?;
        let (used_effects, used_cost) = self.grant_usage(grant)?;
        if checked_add(used_effects, claim.budget_charge.side_effects)? > grant.max_effects {
            return Err(ExecutionControlError::GrantEffectExceeded);
        }
        if claim.budget_charge.cost.minor_units > 0 {
            let max_cost = grant
                .max_cost
                .as_ref()
                .ok_or(ExecutionControlError::GrantCostExceeded)?;
            let total = used_cost
                .unwrap_or(Money::new(max_cost.currency(), 0)?)
                .checked_add(&claim.budget_charge.cost)
                .map_err(|_| ExecutionControlError::GrantCostExceeded)?;
            if !total.is_within(max_cost) {
                return Err(ExecutionControlError::GrantCostExceeded);
            }
        }
        Ok(())
    }

    fn validate_grant_binding(
        grant: &ApprovalGrant,
        claim: &ToolExecutionClaim,
        now: u64,
    ) -> Result<(), ExecutionControlError> {
        if now < grant.issued_at_unix_millis {
            return Err(ExecutionControlError::GrantNotYetValid);
        }
        if grant
            .expires_at_unix_millis
            .is_some_and(|expires| now >= expires)
        {
            return Err(ExecutionControlError::GrantExpired);
        }
        if grant.run_id != claim.run_id
            || grant.project_id != claim.project_id
            || grant.project_revision != claim.project_revision
            || grant.subject != claim.subject
            || grant.tool_descriptor_fingerprint != claim.tool_descriptor_fingerprint
            || grant.side_effect_class != claim.side_effect_class
        {
            return Err(ExecutionControlError::GrantBindingMismatch);
        }
        if claim
            .targets
            .iter()
            .any(|target| grant.targets.binary_search(target).is_err())
        {
            return Err(ExecutionControlError::GrantTargetExceeded);
        }
        Ok(())
    }

    fn grant_usage(
        &self,
        grant: &ApprovalGrant,
    ) -> Result<(u64, Option<Money>), ExecutionControlError> {
        let mut effects = 0_u64;
        let mut cost = grant
            .max_cost
            .as_ref()
            .map(|limit| Money::new(limit.currency(), 0))
            .transpose()?;
        for reservation in self
            .reservations
            .iter()
            .filter(|item| item.claim.grant_id == grant.id)
        {
            let charge = match reservation.status {
                ReservationStatus::Reserved => Some(&reservation.claim.budget_charge),
                ReservationStatus::Completed => reservation
                    .settlement
                    .as_ref()
                    .map(|settlement| &settlement.budget_charge),
                ReservationStatus::Cancelled => None,
            };
            if let Some(charge) = charge {
                effects = checked_add(effects, charge.side_effects)?;
                if charge.cost.minor_units > 0 {
                    cost = Some(
                        cost.ok_or(ExecutionControlError::GrantCostExceeded)?
                            .checked_add(&charge.cost)
                            .map_err(|_| ExecutionControlError::GrantCostExceeded)?,
                    );
                }
            }
        }
        Ok((effects, cost))
    }

    fn enforce_budget(&self, usage: &RunBudgetUsage) -> Result<(), ExecutionControlError> {
        let limits = &self.configured_budget;
        let dimension = if usage.inference_turns > limits.max_inference_turns {
            Some(RunBudgetDimension::InferenceTurns)
        } else if usage.tool_executions > limits.max_tool_executions {
            Some(RunBudgetDimension::ToolExecutions)
        } else if usage.tokens > limits.max_tokens {
            Some(RunBudgetDimension::Tokens)
        } else if !usage.cost.is_within(&limits.max_cost) {
            Some(RunBudgetDimension::Cost)
        } else if usage.wall_clock_millis > limits.max_wall_clock_millis {
            Some(RunBudgetDimension::WallClock)
        } else if usage.preview_renders > limits.max_preview_renders {
            Some(RunBudgetDimension::PreviewRenders)
        } else if usage.side_effects > limits.max_side_effects {
            Some(RunBudgetDimension::SideEffects)
        } else if usage.asset_bytes > limits.max_asset_bytes {
            Some(RunBudgetDimension::AssetBytes)
        } else if usage.concurrent_tools > limits.max_concurrent_tools
            || usage.peak_concurrent_tools > limits.max_concurrent_tools
        {
            Some(RunBudgetDimension::ConcurrentTools)
        } else {
            None
        };
        dimension.map_or(Ok(()), |dimension| {
            Err(ExecutionControlError::RunBudgetExceeded { dimension })
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionControlSnapshot {
    revision: u64,
    control: ExecutionControl,
}

impl ExecutionControlSnapshot {
    /// Creates and validates one durable read model.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlError`] when the restored ledger is corrupt.
    pub fn new(revision: u64, control: ExecutionControl) -> Result<Self, ExecutionControlError> {
        control.validate_restored()?;
        Ok(Self { revision, control })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn control(&self) -> &ExecutionControl {
        &self.control
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionControlCommand {
    IssueGrant(ApprovalGrant),
    RecordInference {
        turn_id: String,
        charge: InferenceBudgetCharge,
        recorded_at_unix_millis: u64,
    },
    AuthorizeTool {
        id: ExecutionReservationId,
        claim: ToolExecutionClaim,
        resource_limit: ToolResourceLimit,
        requested_at_unix_millis: u64,
    },
    SettleTool {
        id: ExecutionReservationId,
        settlement: ToolSettlement,
        completed_at_unix_millis: u64,
    },
    CancelTool {
        id: ExecutionReservationId,
        cancelled_at_unix_millis: u64,
    },
}

impl ExecutionControlCommand {
    fn apply(self, control: &mut ExecutionControl) -> Result<(), ExecutionControlError> {
        match self {
            Self::IssueGrant(grant) => control.issue_grant(grant),
            Self::RecordInference {
                turn_id,
                charge,
                recorded_at_unix_millis,
            } => control
                .record_inference(&turn_id, charge, recorded_at_unix_millis)
                .map(|_| ()),
            Self::AuthorizeTool {
                id,
                claim,
                resource_limit,
                requested_at_unix_millis,
            } => control
                .authorize_tool(id, claim, resource_limit, requested_at_unix_millis)
                .map(|_| ()),
            Self::SettleTool {
                id,
                settlement,
                completed_at_unix_millis,
            } => control
                .settle_tool(&id, settlement, completed_at_unix_millis)
                .map(|_| ()),
            Self::CancelTool {
                id,
                cancelled_at_unix_millis,
            } => control
                .cancel_tool(&id, cancelled_at_unix_millis)
                .map(|_| ()),
        }
    }
}

/// Durable storage seam for one Run's execution-control snapshot.
pub trait ExecutionControlStore: Send + Sync {
    /// Creates revision zero for one Run.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlStoreError`] when the Run exists or storage fails.
    fn create_execution_control(
        &self,
        control: &ExecutionControl,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlStoreError>;

    /// Loads one Run's latest execution-control revision.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlStoreError`] for unavailable or corrupt storage.
    fn load_execution_control(
        &self,
        run_id: &AgentRunId,
    ) -> Result<Option<ExecutionControlSnapshot>, ExecutionControlStoreError>;

    /// Commits a replacement only when the expected revision is current.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlStoreError`] for conflict, corruption, or failure.
    fn commit_execution_control(
        &self,
        expected_revision: u64,
        control: &ExecutionControl,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlStoreError>;
}

/// Deep application module for crash-safe Grant and Budget mutations.
pub struct ExecutionControlManager {
    store: Arc<dyn ExecutionControlStore>,
}

impl ExecutionControlManager {
    #[must_use]
    pub fn new(store: Arc<dyn ExecutionControlStore>) -> Self {
        Self { store }
    }

    /// Publishes the initial immutable system/configured budget binding.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlManagerError`] for invalid state or persistence.
    pub fn configure(
        &self,
        control: &ExecutionControl,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlManagerError> {
        control.validate_restored()?;
        self.store
            .create_execution_control(control)
            .map_err(Into::into)
    }

    /// Loads the latest durable Grant/Budget ledger for one Run.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlManagerError`] when absent, corrupt, or unavailable.
    pub fn inspect(
        &self,
        run_id: &AgentRunId,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlManagerError> {
        self.store
            .load_execution_control(run_id)?
            .ok_or(ExecutionControlStoreError::NotFound.into())
    }

    /// Applies one typed mutation through a CAS commit. An idempotent replay that
    /// produces no state change returns the current revision without another write.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionControlManagerError`] for stale revision, rejected
    /// control, corrupt restore, or unavailable persistence.
    pub fn apply(
        &self,
        run_id: &AgentRunId,
        expected_revision: u64,
        command: ExecutionControlCommand,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlManagerError> {
        let current = self.inspect(run_id)?;
        if current.revision != expected_revision {
            let mut replay = current.control.clone();
            if command.clone().apply(&mut replay).is_ok() && replay == current.control {
                return Ok(current);
            }
            return Err(ExecutionControlStoreError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            }
            .into());
        }
        let mut next = current.control.clone();
        command.apply(&mut next)?;
        next.validate_restored()?;
        if next == current.control {
            return Ok(current);
        }
        self.store
            .commit_execution_control(expected_revision, &next)
            .map_err(Into::into)
    }
}

fn validate_targets(targets: &[String]) -> Result<(), ExecutionControlError> {
    if targets.is_empty() || targets.len() > MAX_APPROVAL_TARGETS {
        return Err(ExecutionControlError::InvalidGrant);
    }
    let mut previous: Option<&str> = None;
    for target in targets {
        validate_bounded(target, MAX_APPROVAL_TARGET_CHARS, "Approval target")?;
        if previous.is_some_and(|value| value >= target.as_str()) {
            return Err(ExecutionControlError::InvalidGrant);
        }
        previous = Some(target);
    }
    Ok(())
}

fn validate_bounded(
    value: &str,
    max_chars: usize,
    field: &'static str,
) -> Result<(), ExecutionControlError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars {
        Err(ExecutionControlError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn validate_digest(value: &str) -> Result<(), ExecutionControlError> {
    if value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        Ok(())
    } else {
        Err(ExecutionControlError::InvalidDigest)
    }
}

fn checked_add(left: u64, right: u64) -> Result<u64, ExecutionControlError> {
    left.checked_add(right)
        .ok_or(ExecutionControlError::NumericOverflow)
}
