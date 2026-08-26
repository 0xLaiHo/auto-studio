use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::{
    MAX_GENERATION_CANDIDATES, MAX_GENERATION_DURATION_SECONDS, MIN_GENERATION_CANDIDATES,
    MIN_GENERATION_DURATION_SECONDS,
};
pub use crate::error::AgentRunError;
use crate::provider::{ThinkingControl, ThinkingLevel};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AgentRunId(Uuid);

impl AgentRunId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    /// Parses an Agent Run identity received through a transport.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRunError::InvalidId`] for a malformed UUID.
    pub fn parse(value: &str) -> Result<Self, AgentRunError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| AgentRunError::InvalidId)
    }
}

impl Default for AgentRunId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationIntent {
    pub prompt: String,
    pub duration_seconds: u32,
    pub candidate_count: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "input", rename_all = "snake_case")]
pub enum AgentDecision {
    GenerateMusic(GenerationIntent),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum CostEstimate {
    Known {
        currency: String,
        lower_minor_units: u64,
        upper_minor_units: u64,
    },
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub actual_cost_minor_units: Option<u64>,
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceProvenance {
    pub provider_kind: String,
    pub model: String,
    #[serde(default, rename = "modelEffort")]
    pub thinking_level: ThinkingLevel,
    #[serde(default)]
    pub thinking_control: ThinkingControl,
    #[serde(default)]
    pub thinking_budget_tokens: Option<u32>,
    #[serde(default)]
    pub capability_revision: String,
    #[serde(default)]
    pub mapping_revision: String,
    pub protocol: String,
    pub response_id: Option<String>,
}

impl Default for InferenceProvenance {
    fn default() -> Self {
        Self {
            provider_kind: "legacy_unknown".to_owned(),
            model: "legacy_unknown".to_owned(),
            thinking_level: ThinkingLevel::default(),
            thinking_control: ThinkingControl::Unsupported,
            thinking_budget_tokens: None,
            capability_revision: "legacy_unknown".to_owned(),
            mapping_revision: "legacy_unknown".to_owned(),
            protocol: "legacy_unknown".to_owned(),
            response_id: None,
        }
    }
}

impl InferenceProvenance {
    fn validate(&self) -> Result<(), AgentRunError> {
        if self.provider_kind.trim().is_empty()
            || self.model.trim().is_empty()
            || self.protocol.trim().is_empty()
            || self
                .response_id
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(AgentRunError::InvalidInferenceProvenance);
        }
        Ok(())
    }
}

impl InferenceUsage {
    fn validate(&self) -> Result<(), AgentRunError> {
        if self.actual_cost_minor_units.is_some() != self.currency.is_some()
            || self
                .currency
                .as_ref()
                .is_some_and(|currency| currency.trim().is_empty())
        {
            return Err(AgentRunError::InvalidUsage);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPlanDraft {
    pub visible_summary: String,
    pub decision: AgentDecision,
    pub estimated_cost: CostEstimate,
    pub usage: InferenceUsage,
    pub inference: InferenceProvenance,
    pub input_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPlan {
    visible_summary: String,
    decision: AgentDecision,
    estimated_cost: CostEstimate,
    #[serde(default)]
    usage: InferenceUsage,
    #[serde(default)]
    inference: InferenceProvenance,
    input_hash: String,
}

impl AgentPlan {
    fn parse(draft: AgentPlanDraft) -> Result<Self, AgentRunError> {
        let visible_summary = draft.visible_summary.trim();
        if visible_summary.is_empty() {
            return Err(AgentRunError::EmptyPlanSummary);
        }
        let input_hash = draft.input_hash.trim();
        if input_hash.is_empty() {
            return Err(AgentRunError::EmptyInputHash);
        }
        match &draft.decision {
            AgentDecision::GenerateMusic(intent) => {
                if intent.prompt.trim().is_empty() {
                    return Err(AgentRunError::EmptyGenerationPrompt);
                }
                if !(MIN_GENERATION_DURATION_SECONDS..=MAX_GENERATION_DURATION_SECONDS)
                    .contains(&intent.duration_seconds)
                {
                    return Err(AgentRunError::InvalidDuration);
                }
                if !(MIN_GENERATION_CANDIDATES..=MAX_GENERATION_CANDIDATES)
                    .contains(&intent.candidate_count)
                {
                    return Err(AgentRunError::InvalidCandidateCount);
                }
            }
        }
        if let CostEstimate::Known {
            currency,
            lower_minor_units,
            upper_minor_units,
        } = &draft.estimated_cost
            && (currency.trim().is_empty() || lower_minor_units > upper_minor_units)
        {
            return Err(AgentRunError::InvalidCostEstimate);
        }
        draft.usage.validate()?;
        draft.inference.validate()?;
        Ok(Self {
            visible_summary: visible_summary.to_owned(),
            decision: draft.decision,
            estimated_cost: draft.estimated_cost,
            usage: draft.usage,
            inference: draft.inference,
            input_hash: input_hash.to_owned(),
        })
    }

    #[must_use]
    pub fn visible_summary(&self) -> &str {
        &self.visible_summary
    }

    #[must_use]
    pub const fn decision(&self) -> &AgentDecision {
        &self.decision
    }

    #[must_use]
    pub const fn estimated_cost(&self) -> &CostEstimate {
        &self.estimated_cost
    }

    #[must_use]
    pub fn input_hash(&self) -> &str {
        &self.input_hash
    }

    #[must_use]
    pub const fn usage(&self) -> &InferenceUsage {
        &self.usage
    }

    #[must_use]
    pub const fn inference(&self) -> &InferenceProvenance {
        &self.inference
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostApproval {
    pub currency: String,
    pub max_minor_units: u64,
    pub input_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
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

impl AgentRunStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunFailureKind {
    HarnessUnavailable,
    InferenceInterrupted,
    ProviderRejected,
    ProviderUnavailable,
    InvalidProviderResponse,
    ProviderConfirmedNotFound,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunFailureDraft {
    pub kind: AgentRunFailureKind,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunFailure {
    kind: AgentRunFailureKind,
    message: String,
}

impl AgentRunFailure {
    fn parse(draft: AgentRunFailureDraft) -> Result<Self, AgentRunError> {
        let AgentRunFailureDraft { kind, message } = draft;
        let message = message.trim();
        if message.is_empty() {
            return Err(AgentRunError::InvalidFailure);
        }
        Ok(Self {
            kind,
            message: message.to_owned(),
        })
    }

    #[must_use]
    pub const fn kind(&self) -> AgentRunFailureKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationAttemptDraft {
    pub attempt_id: String,
    pub provider_kind: String,
    pub model: String,
    pub request_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationAttempt {
    attempt_id: String,
    provider_kind: String,
    model: String,
    request_hash: String,
}

impl GenerationAttempt {
    #[must_use]
    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    #[must_use]
    pub fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn request_hash(&self) -> &str {
        &self.request_hash
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationJobDraft {
    pub attempt_id: String,
    pub external_job_id: String,
    pub provider_kind: String,
    pub model: String,
    pub request_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationJob {
    attempt_id: String,
    external_job_id: String,
    provider_kind: String,
    model: String,
    request_hash: String,
}

impl GenerationJob {
    #[must_use]
    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    #[must_use]
    pub fn external_job_id(&self) -> &str {
        &self.external_job_id
    }

    #[must_use]
    pub fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn request_hash(&self) -> &str {
        &self.request_hash
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    id: AgentRunId,
    context_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan: Option<AgentPlan>,
    status: AgentRunStatus,
    approval: Option<CostApproval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation_job: Option<GenerationJob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation_attempt: Option<GenerationAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failure: Option<AgentRunFailure>,
}

impl AgentRun {
    #[must_use]
    pub(crate) fn begin(id: AgentRunId, context_revision: u64) -> Self {
        Self {
            id,
            context_revision,
            plan: None,
            status: AgentRunStatus::Planning,
            approval: None,
            generation_job: None,
            generation_attempt: None,
            failure: None,
        }
    }

    pub(crate) fn plan(
        id: AgentRunId,
        context_revision: u64,
        draft: AgentPlanDraft,
    ) -> Result<Self, AgentRunError> {
        let mut run = Self::begin(id, context_revision);
        run.record_plan(draft)?;
        Ok(run)
    }

    pub(crate) fn record_plan(&mut self, draft: AgentPlanDraft) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::Planning || self.plan.is_some() {
            return Err(AgentRunError::InvalidTransition);
        }
        self.plan = Some(AgentPlan::parse(draft)?);
        self.status = AgentRunStatus::AwaitingApproval;
        Ok(())
    }

    pub(crate) fn approve(&mut self, approval: CostApproval) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::AwaitingApproval {
            return Err(AgentRunError::InvalidTransition);
        }
        let plan = self.required_plan()?;
        if approval.input_hash != plan.input_hash {
            return Err(AgentRunError::ApprovalInputChanged);
        }
        if let CostEstimate::Known {
            currency,
            upper_minor_units,
            ..
        } = &plan.estimated_cost
            && (approval.currency != *currency || approval.max_minor_units < *upper_minor_units)
        {
            return Err(AgentRunError::ApprovalBudgetTooLow);
        }
        self.approval = Some(approval);
        self.status = AgentRunStatus::ReadyToSubmit;
        Ok(())
    }

    pub(crate) fn record_submitted(
        &mut self,
        draft: GenerationJobDraft,
    ) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::Submitting {
            return Err(AgentRunError::InvalidTransition);
        }
        self.record_job(draft)
    }

    pub(crate) fn record_reconciled_submission(
        &mut self,
        draft: GenerationJobDraft,
    ) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::UnknownOutcome {
            return Err(AgentRunError::InvalidTransition);
        }
        self.record_job(draft)
    }

    fn record_job(&mut self, draft: GenerationJobDraft) -> Result<(), AgentRunError> {
        let plan = self.required_plan()?;
        let attempt = self
            .generation_attempt
            .as_ref()
            .ok_or(AgentRunError::InvalidGenerationJob)?;
        if draft.request_hash != plan.input_hash
            || draft.attempt_id != attempt.attempt_id
            || draft.provider_kind != attempt.provider_kind
            || draft.model != attempt.model
            || draft.external_job_id.trim().is_empty()
        {
            return Err(AgentRunError::InvalidGenerationJob);
        }
        self.generation_job = Some(GenerationJob {
            attempt_id: draft.attempt_id,
            external_job_id: draft.external_job_id,
            provider_kind: draft.provider_kind,
            model: draft.model,
            request_hash: draft.request_hash,
        });
        self.status = AgentRunStatus::Submitted;
        Ok(())
    }

    pub(crate) fn prepare_generation(
        &mut self,
        draft: GenerationAttemptDraft,
    ) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::ReadyToSubmit {
            return Err(AgentRunError::InvalidTransition);
        }
        let plan = self.required_plan()?;
        if draft.request_hash != plan.input_hash
            || draft.attempt_id.trim().is_empty()
            || draft.provider_kind.trim().is_empty()
            || draft.model.trim().is_empty()
        {
            return Err(AgentRunError::InvalidGenerationJob);
        }
        self.generation_attempt = Some(GenerationAttempt {
            attempt_id: draft.attempt_id,
            provider_kind: draft.provider_kind,
            model: draft.model,
            request_hash: draft.request_hash,
        });
        self.status = AgentRunStatus::Submitting;
        Ok(())
    }

    pub(crate) fn mark_unknown_outcome(&mut self) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::Submitting {
            return Err(AgentRunError::InvalidTransition);
        }
        self.status = AgentRunStatus::UnknownOutcome;
        Ok(())
    }

    pub(crate) fn reconcile_not_found(&mut self) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::UnknownOutcome {
            return Err(AgentRunError::InvalidTransition);
        }
        self.failure = Some(AgentRunFailure::parse(AgentRunFailureDraft {
            kind: AgentRunFailureKind::ProviderConfirmedNotFound,
            message: "Provider confirmed that no Generation Job exists".to_owned(),
        })?);
        self.status = AgentRunStatus::Failed;
        Ok(())
    }

    pub(crate) fn fail(&mut self, draft: AgentRunFailureDraft) -> Result<(), AgentRunError> {
        if !matches!(
            self.status,
            AgentRunStatus::Planning | AgentRunStatus::Submitting | AgentRunStatus::Submitted
        ) {
            return Err(AgentRunError::InvalidTransition);
        }
        self.failure = Some(AgentRunFailure::parse(draft)?);
        self.status = AgentRunStatus::Failed;
        Ok(())
    }

    pub(crate) fn validate_candidate_count(&self, actual: usize) -> Result<(), AgentRunError> {
        let expected = match self.required_plan()?.decision() {
            AgentDecision::GenerateMusic(intent) => usize::from(intent.candidate_count),
        };
        if actual == expected {
            Ok(())
        } else {
            Err(AgentRunError::CandidateCountMismatch { expected, actual })
        }
    }

    pub(crate) fn complete(&mut self) -> Result<(), AgentRunError> {
        if self.status != AgentRunStatus::Submitted {
            return Err(AgentRunError::InvalidTransition);
        }
        self.status = AgentRunStatus::Completed;
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> &AgentRunId {
        &self.id
    }

    #[must_use]
    pub(crate) const fn context_revision(&self) -> u64 {
        self.context_revision
    }

    #[must_use]
    pub const fn status(&self) -> AgentRunStatus {
        self.status
    }

    #[must_use]
    pub const fn approval(&self) -> Option<&CostApproval> {
        self.approval.as_ref()
    }

    #[must_use]
    pub const fn plan_value(&self) -> Option<&AgentPlan> {
        self.plan.as_ref()
    }

    #[must_use]
    pub const fn generation_job(&self) -> Option<&GenerationJob> {
        self.generation_job.as_ref()
    }

    #[must_use]
    pub const fn generation_attempt(&self) -> Option<&GenerationAttempt> {
        self.generation_attempt.as_ref()
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&AgentRunFailure> {
        self.failure.as_ref()
    }

    pub(crate) fn validate_restored(&self) -> Result<(), AgentRunError> {
        AgentRunId::parse(&self.id.as_str())?;
        self.validate_restored_plan()?;
        self.validate_restored_generation()?;
        if let Some(failure) = &self.failure {
            AgentRunFailure::parse(AgentRunFailureDraft {
                kind: failure.kind,
                message: failure.message.clone(),
            })?;
        }
        if self.has_valid_state_shape() {
            Ok(())
        } else {
            Err(AgentRunError::InvalidTransition)
        }
    }

    fn validate_restored_plan(&self) -> Result<(), AgentRunError> {
        if let Some(plan) = &self.plan {
            AgentPlan::parse(AgentPlanDraft {
                visible_summary: plan.visible_summary.clone(),
                decision: plan.decision.clone(),
                estimated_cost: plan.estimated_cost.clone(),
                usage: plan.usage.clone(),
                inference: plan.inference.clone(),
                input_hash: plan.input_hash.clone(),
            })?;
        }

        if let Some(approval) = &self.approval {
            let plan = self.required_plan()?;
            if approval.currency.trim().is_empty() || approval.input_hash != plan.input_hash {
                return Err(AgentRunError::ApprovalInputChanged);
            }
            if let CostEstimate::Known {
                currency,
                upper_minor_units,
                ..
            } = &plan.estimated_cost
                && (approval.currency != *currency || approval.max_minor_units < *upper_minor_units)
            {
                return Err(AgentRunError::ApprovalBudgetTooLow);
            }
        }
        Ok(())
    }

    fn validate_restored_generation(&self) -> Result<(), AgentRunError> {
        if let Some(attempt) = &self.generation_attempt
            && (attempt.attempt_id.trim().is_empty()
                || attempt.provider_kind.trim().is_empty()
                || attempt.model.trim().is_empty()
                || attempt.request_hash != self.required_plan()?.input_hash)
        {
            return Err(AgentRunError::InvalidGenerationJob);
        }
        if let Some(job) = &self.generation_job {
            let attempt = self
                .generation_attempt
                .as_ref()
                .ok_or(AgentRunError::InvalidGenerationJob)?;
            if job.external_job_id.trim().is_empty()
                || job.attempt_id != attempt.attempt_id
                || job.provider_kind != attempt.provider_kind
                || job.model != attempt.model
                || job.request_hash != attempt.request_hash
            {
                return Err(AgentRunError::InvalidGenerationJob);
            }
        }
        Ok(())
    }

    fn has_valid_state_shape(&self) -> bool {
        match self.status {
            AgentRunStatus::Planning => {
                self.plan.is_none()
                    && self.approval.is_none()
                    && self.generation_attempt.is_none()
                    && self.generation_job.is_none()
                    && self.failure.is_none()
            }
            AgentRunStatus::AwaitingApproval => {
                self.plan.is_some()
                    && self.approval.is_none()
                    && self.generation_attempt.is_none()
                    && self.generation_job.is_none()
                    && self.failure.is_none()
            }
            AgentRunStatus::ReadyToSubmit => {
                self.plan.is_some()
                    && self.approval.is_some()
                    && self.generation_attempt.is_none()
                    && self.generation_job.is_none()
                    && self.failure.is_none()
            }
            AgentRunStatus::Submitting | AgentRunStatus::UnknownOutcome => {
                self.plan.is_some()
                    && self.approval.is_some()
                    && self.generation_attempt.is_some()
                    && self.generation_job.is_none()
                    && self.failure.is_none()
            }
            AgentRunStatus::Failed => {
                self.failure.is_some()
                    && ((self.plan.is_none()
                        && self.approval.is_none()
                        && self.generation_attempt.is_none()
                        && self.generation_job.is_none())
                        || (self.plan.is_some()
                            && self.approval.is_some()
                            && self.generation_attempt.is_some()))
            }
            AgentRunStatus::Submitted | AgentRunStatus::Completed => {
                self.plan.is_some()
                    && self.approval.is_some()
                    && self.generation_attempt.is_some()
                    && self.generation_job.is_some()
                    && self.failure.is_none()
            }
            AgentRunStatus::Cancelled => self.plan.is_some() && self.approval.is_some(),
        }
    }

    fn required_plan(&self) -> Result<&AgentPlan, AgentRunError> {
        self.plan.as_ref().ok_or(AgentRunError::InvalidTransition)
    }
}
