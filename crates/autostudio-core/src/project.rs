use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::{
    AgentPlanDraft, AgentRun, AgentRunError, AgentRunFailureDraft, AgentRunId, CostApproval,
    GenerationAttemptDraft, GenerationJobDraft,
};
use crate::production::{
    AssetVersion, AssetVersionId, AudioClipTimeline, Candidate, CandidateDraft, CandidateId,
    HandoffExport, HandoffExportDraft, HandoffRequest, HandoffSink, ProductionError, Selection,
};

use crate::constants::{MAX_BRIEF_SUMMARY_CHARS, MAX_PROJECT_NAME_CHARS};
pub use crate::error::{
    CreativeBriefError, ProjectError, ProjectNameError, ProjectRestoreError, ProjectStoreError,
};

/// Infrastructure seam for creating a consistent copy of one Project Package.
pub trait ProjectBackupSink: Send + Sync {
    /// Copies the supplied durable snapshot and its package files.
    ///
    /// # Errors
    ///
    /// Returns a redacted error when the backup cannot be published atomically.
    fn backup(&self, project: &Project) -> Result<ProjectBackupDraft, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectBackupDraft {
    pub backup_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackup {
    id: Uuid,
    source_project_id: ProjectId,
    source_project_revision: u64,
    backup_name: String,
}

impl ProjectBackup {
    fn parse(project: &Project, draft: &ProjectBackupDraft) -> Result<Self, ProjectRestoreError> {
        let name = draft.backup_name.trim();
        if name.is_empty()
            || !name.ends_with(".autostudio")
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            return Err(ProjectRestoreError::UnsafeBackupName);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            source_project_id: project.id.clone(),
            source_project_revision: project.revision,
            backup_name: name.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    id: ProjectId,
    name: ProjectName,
    revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brief: Option<CreativeBrief>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    agent_runs: Vec<AgentRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    candidates: Vec<Candidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selection: Option<Selection>,
    #[serde(default)]
    timeline: AudioClipTimeline,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exports: Vec<HandoffExport>,
}

impl Project {
    fn new(name: ProjectName) -> Self {
        Self {
            id: ProjectId::new(),
            name,
            revision: 0,
            brief: None,
            agent_runs: Vec::new(),
            candidates: Vec::new(),
            selection: None,
            timeline: AudioClipTimeline::default(),
            exports: Vec::new(),
        }
    }

    /// Restores a Project from a trusted storage adapter while rechecking its invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectRestoreError`] when persisted identity or name data violates
    /// the current domain invariants.
    pub fn restore(id: &str, name: &str, revision: u64) -> Result<Self, ProjectRestoreError> {
        Ok(Self {
            id: ProjectId::parse(id)?,
            name: ProjectName::parse(name)?,
            revision,
            brief: None,
            agent_runs: Vec::new(),
            candidates: Vec::new(),
            selection: None,
            timeline: AudioClipTimeline::default(),
            exports: Vec::new(),
        })
    }

    #[must_use]
    pub const fn id(&self) -> &ProjectId {
        &self.id
    }

    #[must_use]
    pub const fn name(&self) -> &ProjectName {
        &self.name
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn brief(&self) -> Option<&CreativeBrief> {
        self.brief.as_ref()
    }

    #[must_use]
    pub fn agent_runs(&self) -> &[AgentRun] {
        &self.agent_runs
    }

    #[must_use]
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    #[must_use]
    pub const fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    #[must_use]
    pub const fn timeline(&self) -> &AudioClipTimeline {
        &self.timeline
    }

    #[must_use]
    pub fn exports(&self) -> &[HandoffExport] {
        &self.exports
    }

    /// Revalidates a deserialized Project snapshot before it becomes authoritative.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectRestoreError`] when nested facts, references, or state
    /// transitions violate the current domain model.
    pub fn validate_restored(&self) -> Result<(), ProjectRestoreError> {
        ProjectId::parse(&self.id.as_str())?;
        let normalized_name = ProjectName::parse(self.name.as_str())?;
        if normalized_name != self.name {
            return Err(ProjectRestoreError::InvalidName(
                ProjectNameError::NotNormalized,
            ));
        }
        if let Some(brief) = &self.brief {
            CreativeBrief::parse(CreativeBriefDraft {
                summary: brief.summary.clone(),
                purpose: brief.purpose.clone(),
                style: brief.style.clone(),
                mood: brief.mood.clone(),
                instrumentation: brief.instrumentation.clone(),
                target_duration_seconds: brief.target_duration_seconds,
                lyrics: brief.lyrics.clone(),
                constraints: brief.constraints.clone(),
            })?;
        }

        let mut run_ids = HashSet::new();
        for run in &self.agent_runs {
            run.validate_restored()?;
            if run.context_revision() > self.revision || !run_ids.insert(run.id().as_str()) {
                return Err(ProjectRestoreError::InconsistentSnapshot);
            }
        }
        let mut candidate_ids = HashSet::new();
        let mut asset_ids = HashSet::new();
        for candidate in &self.candidates {
            candidate.validate_restored()?;
            if !run_ids.contains(&candidate.source_run_id().as_str())
                || !candidate_ids.insert(candidate.id().as_str())
                || !asset_ids.insert(candidate.asset().id().as_str())
            {
                return Err(ProjectRestoreError::InconsistentSnapshot);
            }
        }
        if let Some(selection) = &self.selection
            && (selection.project_revision() > self.revision
                || !candidate_ids.contains(&selection.candidate_id().as_str()))
        {
            return Err(ProjectRestoreError::InconsistentSnapshot);
        }
        self.timeline
            .validate_restored(&self.candidates, self.selection.as_ref())?;
        let mut export_ids = HashSet::new();
        for export in &self.exports {
            export.validate_restored(self.revision)?;
            if !export_ids.insert(export.id().as_str()) {
                return Err(ProjectRestoreError::InconsistentSnapshot);
            }
        }
        Ok(())
    }

    fn asset_version(&self, asset_version_id: &AssetVersionId) -> Option<&AssetVersion> {
        self.candidates
            .iter()
            .map(Candidate::asset)
            .find(|asset| asset.id() == asset_version_id)
    }

    fn set_brief(&mut self, brief: CreativeBrief) -> Result<(), ProjectError> {
        self.brief = Some(brief);
        self.increment_revision()
    }

    fn plan_agent_run(
        &mut self,
        run_id: AgentRunId,
        draft: AgentPlanDraft,
    ) -> Result<AgentRun, ProjectError> {
        if self
            .agent_runs
            .iter()
            .any(|run| !run.status().is_terminal())
        {
            return Err(AgentRunError::ActiveRunExists.into());
        }
        let run = AgentRun::plan(run_id, self.revision, draft)?;
        self.agent_runs.push(run.clone());
        self.increment_revision()?;
        Ok(run)
    }

    fn begin_agent_run(&mut self, run_id: AgentRunId) -> Result<AgentRun, ProjectError> {
        if self
            .agent_runs
            .iter()
            .any(|run| !run.status().is_terminal())
        {
            return Err(AgentRunError::ActiveRunExists.into());
        }
        let run = AgentRun::begin(run_id, self.revision);
        self.agent_runs.push(run.clone());
        self.increment_revision()?;
        Ok(run)
    }

    fn record_agent_plan(
        &mut self,
        run_id: &AgentRunId,
        draft: AgentPlanDraft,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.record_plan(draft)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn fail_agent_run(
        &mut self,
        run_id: &AgentRunId,
        failure: AgentRunFailureDraft,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.fail(failure)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn approve_agent_run(
        &mut self,
        run_id: &AgentRunId,
        approval: CostApproval,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.approve(approval)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn record_generation_submitted(
        &mut self,
        run_id: &AgentRunId,
        job: GenerationJobDraft,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.record_submitted(job)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn prepare_generation(
        &mut self,
        run_id: &AgentRunId,
        attempt: GenerationAttemptDraft,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.prepare_generation(attempt)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn mark_generation_unknown(&mut self, run_id: &AgentRunId) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.mark_unknown_outcome()?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn mark_generation_failed(
        &mut self,
        run_id: &AgentRunId,
        failure: AgentRunFailureDraft,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.fail(failure)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn record_reconciled_submission(
        &mut self,
        run_id: &AgentRunId,
        job: GenerationJobDraft,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.record_reconciled_submission(job)?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn reconcile_generation_not_found(
        &mut self,
        run_id: &AgentRunId,
    ) -> Result<AgentRun, ProjectError> {
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.reconcile_not_found()?;
        let run = run.clone();
        self.increment_revision()?;
        Ok(run)
    }

    fn commit_candidates(
        &mut self,
        run_id: &AgentRunId,
        drafts: Vec<CandidateDraft>,
    ) -> Result<Vec<Candidate>, ProjectError> {
        if drafts.is_empty() {
            return Err(ProductionError::EmptyCandidates.into());
        }
        let run = self
            .agent_runs
            .iter()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.validate_candidate_count(drafts.len())?;
        let candidates = drafts
            .into_iter()
            .map(|draft| Candidate::parse(run_id, draft))
            .collect::<Result<Vec<_>, _>>()?;
        let run = self
            .agent_runs
            .iter_mut()
            .find(|run| run.id() == run_id)
            .ok_or(AgentRunError::NotFound)?;
        run.complete()?;
        self.candidates.extend(candidates.iter().cloned());
        self.increment_revision()?;
        Ok(candidates)
    }

    fn select_candidate(
        &mut self,
        candidate_id: &CandidateId,
        start_micros: u64,
    ) -> Result<Selection, ProjectError> {
        let candidate = self
            .candidates
            .iter()
            .find(|candidate| candidate.id() == candidate_id)
            .cloned()
            .ok_or(ProductionError::CandidateNotFound)?;
        self.increment_revision()?;
        let selection = Selection::new(candidate_id.clone(), self.revision);
        self.timeline.select(candidate.asset(), start_micros);
        self.selection = Some(selection.clone());
        Ok(selection)
    }

    fn handoff_request(&self) -> Result<HandoffRequest, ProjectError> {
        let selection = self
            .selection
            .as_ref()
            .ok_or(ProductionError::MissingSelection)?;
        let candidate = self
            .candidates
            .iter()
            .find(|candidate| candidate.id() == selection.candidate_id())
            .ok_or(ProductionError::CandidateNotFound)?;
        let brief = self
            .brief
            .as_ref()
            .ok_or(ProductionError::MissingSelection)?;
        Ok(HandoffRequest::new(
            self.id.as_str(),
            self.name.as_str().to_owned(),
            self.revision,
            selection.id().clone(),
            candidate.id().clone(),
            candidate.label().to_owned(),
            brief.summary().to_owned(),
            candidate.asset().clone(),
            self.timeline.tempo_hint_bpm(),
            self.timeline.key_hint().map(ToOwned::to_owned),
            self.timeline.markers_micros().to_vec(),
        ))
    }

    fn record_handoff(
        &mut self,
        request: &HandoffRequest,
        draft: HandoffExportDraft,
    ) -> Result<HandoffExport, ProjectError> {
        let export = HandoffExport::parse(request, draft)?;
        self.exports.push(export.clone());
        self.increment_revision()?;
        Ok(export)
    }

    fn increment_revision(&mut self) -> Result<(), ProjectError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ProjectError::RevisionExhausted)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProjectId(Uuid);

impl ProjectId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }

    fn parse(value: &str) -> Result<Self, ProjectRestoreError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ProjectRestoreError::InvalidId)
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProjectName(String);

impl ProjectName {
    /// Validates and normalizes a creator-visible Project name.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectNameError`] when the trimmed name is empty or longer than
    /// the supported character limit.
    pub fn parse(value: &str) -> Result<Self, ProjectNameError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(ProjectNameError::Empty);
        }
        if value.chars().count() > MAX_PROJECT_NAME_CHARS {
            return Err(ProjectNameError::TooLong {
                max_chars: MAX_PROJECT_NAME_CHARS,
            });
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Storage seam for the Project application module.
pub trait ProjectStore: Send + Sync {
    /// Persists the first Project in a Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectStoreError`] for an existing Project or unavailable storage.
    fn create(&self, project: &Project) -> Result<(), ProjectStoreError>;

    /// Loads the Project currently stored in a Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectStoreError`] when no Project exists or storage is unavailable.
    fn open(&self) -> Result<Project, ProjectStoreError>;

    /// Atomically replaces a Project snapshot and appends its resulting event.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectStoreError`] when `expected_revision` is stale or the
    /// transaction cannot be committed.
    fn commit(
        &self,
        _expected_revision: u64,
        _project: &Project,
        _event: &ProjectEvent,
    ) -> Result<(), ProjectStoreError> {
        Err(ProjectStoreError::Unavailable(
            "project store does not support versioned commits".to_owned(),
        ))
    }

    /// Reads durable Project events after an exclusive sequence cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectStoreError`] when the event ledger is unavailable.
    fn events_after(
        &self,
        _after_sequence: u64,
    ) -> Result<Vec<ProjectEventEnvelope>, ProjectStoreError> {
        Err(ProjectStoreError::Unavailable(
            "project store does not support event replay".to_owned(),
        ))
    }
}

pub struct ProjectService {
    store: Arc<dyn ProjectStore>,
}

impl ProjectService {
    #[must_use]
    pub fn new(store: Arc<dyn ProjectStore>) -> Self {
        Self { store }
    }

    /// Creates the first Project in the configured Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the name is invalid or persistence fails.
    pub fn create_project(&self, name: &str) -> Result<Project, ProjectError> {
        let project = Project::new(ProjectName::parse(name)?);
        self.store.create(&project)?;
        Ok(project)
    }

    /// Opens the Project in the configured Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Project is absent or persistence fails.
    pub fn open_project(&self) -> Result<Project, ProjectError> {
        self.store.open().map_err(Into::into)
    }

    /// Creates a consistent external copy without changing the Project revision.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the caller is stale or backup publication fails.
    pub fn backup_project(
        &self,
        expected_revision: u64,
        sink: &dyn ProjectBackupSink,
    ) -> Result<ProjectBackup, ProjectError> {
        let project = self.open_at_revision(expected_revision)?;
        let draft = sink.backup(&project).map_err(ProjectError::Backup)?;
        ProjectBackup::parse(&project, &draft).map_err(ProjectError::Restore)
    }

    /// Replaces the Creative Brief when the caller holds the current Project revision.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the brief is invalid, the revision is stale,
    /// or the transaction cannot be committed.
    pub fn set_brief(
        &self,
        expected_revision: u64,
        draft: CreativeBriefDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.store.open()?;
        if project.revision() != expected_revision {
            return Err(ProjectStoreError::RevisionConflict {
                expected: expected_revision,
                actual: project.revision(),
            }
            .into());
        }
        let brief = CreativeBrief::parse(draft)?;
        project.set_brief(brief.clone())?;
        let event = ProjectEvent::brief_updated(&project, brief);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Persists a visible Agent Plan without granting permission to execute it.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the plan is invalid, the revision is stale,
    /// or the transaction cannot be committed.
    pub fn plan_agent_run(
        &self,
        expected_revision: u64,
        draft: AgentPlanDraft,
    ) -> Result<Project, ProjectError> {
        self.plan_agent_run_with_id(expected_revision, AgentRunId::new(), draft)
    }

    /// Persists a visible Agent Plan under an identity allocated before inference.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the plan or identity is invalid, the
    /// revision is stale, or the transaction cannot be committed.
    pub fn plan_agent_run_with_id(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
        draft: AgentPlanDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.plan_agent_run(run_id, draft)?;
        let event = ProjectEvent::agent_run_planned(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Starts a durable Agent Run before any Provider request is made.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when another Run is active, the revision is stale,
    /// or the transaction cannot be committed.
    pub fn begin_agent_run(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.begin_agent_run(run_id)?;
        let event = ProjectEvent::agent_run_started(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Attaches a validated Agent Plan to a durable planning Run.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Run is absent or no longer planning,
    /// the Plan is invalid, the revision is stale, or commit fails.
    pub fn record_agent_plan(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        draft: AgentPlanDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.record_agent_plan(run_id, draft)?;
        let event = ProjectEvent::agent_run_planned(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Terminates a planning or execution Run with a durable failure record.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Run cannot fail from its current state,
    /// the failure is invalid, the revision is stale, or commit fails.
    pub fn fail_agent_run(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        failure: AgentRunFailureDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.fail_agent_run(run_id, failure)?;
        let event = ProjectEvent::agent_run_failed(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Records a Creator Approval for one unchanged Agent Plan.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the run is absent, the Approval does not
    /// cover the planned input/cost, the revision is stale, or commit fails.
    pub fn approve_agent_run(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        approval: CostApproval,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.approve_agent_run(run_id, approval)?;
        let event = ProjectEvent::agent_run_approved(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Persists a Generation Attempt before making the external submit call.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the attempt does not match the Approval,
    /// the transition or revision is stale, or commit fails.
    pub fn prepare_generation(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        attempt: GenerationAttemptDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.prepare_generation(run_id, attempt)?;
        let event = ProjectEvent::generation_submit_started(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Records a Provider-accepted Generation Job for a persisted Attempt.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the job does not match the approved input,
    /// the transition or revision is stale, or commit fails.
    pub fn record_generation_submitted(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        job: GenerationJobDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.record_generation_submitted(run_id, job)?;
        let event = ProjectEvent::generation_submitted(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Marks an ambiguous external submit without retrying it.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Run is not currently submitting, the
    /// revision is stale, or commit fails.
    pub fn mark_generation_unknown(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.mark_generation_unknown(run_id)?;
        let event = ProjectEvent::generation_unknown(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Persists a known terminal Generation failure after an Attempt exists.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Run cannot fail from its current state,
    /// the failure record is invalid, the revision is stale, or commit fails.
    pub fn mark_generation_failed(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        failure: AgentRunFailureDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.mark_generation_failed(run_id, failure)?;
        let event = ProjectEvent::generation_failed(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Records a Provider Job found by reconciling an Unknown Outcome.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Run is not unknown, the Job does not
    /// match the persisted Attempt, the revision is stale, or commit fails.
    pub fn record_reconciled_submission(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        job: GenerationJobDraft,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.record_reconciled_submission(run_id, job)?;
        let event = ProjectEvent::generation_reconciled(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Closes an Unknown Outcome only after the Provider confirms no Job exists.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Run is not unknown, the revision is stale,
    /// or the reconciliation result cannot be committed.
    pub fn reconcile_generation_not_found(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let run = project.reconcile_generation_not_found(run_id)?;
        let event = ProjectEvent::generation_reconciled_not_found(&project, run);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Commits verified generated audio as Candidates and completes the Agent Run.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when Candidate metadata is unsafe, the Generation
    /// Job is not submitted, the revision is stale, or commit fails.
    pub fn commit_candidates(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        drafts: Vec<CandidateDraft>,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let candidates = project.commit_candidates(run_id, drafts)?;
        let event = ProjectEvent::candidates_committed(&project, run_id.clone(), candidates);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Applies an explicit Creator Selection to the Ship 0 Audio Clip Timeline.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Candidate is absent, the revision is
    /// stale, or commit fails.
    pub fn select_candidate(
        &self,
        expected_revision: u64,
        candidate_id: &CandidateId,
        start_micros: u64,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let selection = project.select_candidate(candidate_id, start_micros)?;
        let event = ProjectEvent::candidate_selected(&project, selection);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Creates and records a deterministic DAW Handoff from the current Selection.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when no Selection exists, the revision is stale,
    /// package materialization fails, the draft is unsafe, or commit fails.
    pub fn export_handoff(
        &self,
        expected_revision: u64,
        sink: &dyn HandoffSink,
    ) -> Result<Project, ProjectError> {
        let mut project = self.open_at_revision(expected_revision)?;
        let request = project.handoff_request()?;
        let draft = sink.export(&request).map_err(ProjectError::Handoff)?;
        let export = project.record_handoff(&request, draft)?;
        let event = ProjectEvent::handoff_exported(&project, export);
        self.store.commit(expected_revision, &project, &event)?;
        Ok(project)
    }

    /// Resolves an Asset Version ID against durable Project facts for Preview Playback.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the Project or Asset Version is absent.
    pub fn preview_asset(
        &self,
        asset_version_id: &AssetVersionId,
    ) -> Result<AssetVersion, ProjectError> {
        self.store
            .open()?
            .asset_version(asset_version_id)
            .cloned()
            .ok_or_else(|| ProductionError::AssetNotFound.into())
    }

    /// Returns durable Project events after an exclusive sequence cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the event ledger is unavailable.
    pub fn events_after(
        &self,
        after_sequence: u64,
    ) -> Result<Vec<ProjectEventEnvelope>, ProjectError> {
        self.store.events_after(after_sequence).map_err(Into::into)
    }

    fn open_at_revision(&self, expected_revision: u64) -> Result<Project, ProjectError> {
        let project = self.store.open()?;
        if project.revision() != expected_revision {
            return Err(ProjectStoreError::RevisionConflict {
                expected: expected_revision,
                actual: project.revision(),
            }
            .into());
        }
        Ok(project)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeBriefDraft {
    pub summary: String,
    pub purpose: Option<String>,
    pub style: Vec<String>,
    pub mood: Vec<String>,
    pub instrumentation: Vec<String>,
    pub target_duration_seconds: Option<u32>,
    pub lyrics: Option<String>,
    pub constraints: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeBrief {
    summary: String,
    purpose: Option<String>,
    style: Vec<String>,
    mood: Vec<String>,
    instrumentation: Vec<String>,
    target_duration_seconds: Option<u32>,
    lyrics: Option<String>,
    constraints: Vec<String>,
}

impl CreativeBrief {
    fn parse(draft: CreativeBriefDraft) -> Result<Self, CreativeBriefError> {
        let summary = draft.summary.trim();
        if summary.is_empty() {
            return Err(CreativeBriefError::EmptySummary);
        }
        if summary.chars().count() > MAX_BRIEF_SUMMARY_CHARS {
            return Err(CreativeBriefError::SummaryTooLong {
                max_chars: MAX_BRIEF_SUMMARY_CHARS,
            });
        }
        Ok(Self {
            summary: summary.to_owned(),
            purpose: normalize_optional(draft.purpose),
            style: normalize_list(draft.style),
            mood: normalize_list(draft.mood),
            instrumentation: normalize_list(draft.instrumentation),
            target_duration_seconds: draft.target_duration_seconds,
            lyrics: normalize_optional(draft.lyrics),
            constraints: normalize_list(draft.constraints),
        })
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    #[must_use]
    pub const fn target_duration_seconds(&self) -> Option<u32> {
        self.target_duration_seconds
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEvent {
    project_id: ProjectId,
    project_revision: u64,
    kind: ProjectEventKind,
}

impl ProjectEvent {
    #[must_use]
    pub fn created(project: &Project) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::ProjectCreated,
        }
    }

    fn brief_updated(project: &Project, brief: CreativeBrief) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::BriefUpdated { brief },
        }
    }

    fn agent_run_planned(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::AgentRunPlanned { run },
        }
    }

    fn agent_run_started(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::AgentRunStarted { run },
        }
    }

    fn agent_run_failed(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::AgentRunFailed { run },
        }
    }

    fn agent_run_approved(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::AgentRunApproved { run },
        }
    }

    fn generation_submitted(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::GenerationSubmitted { run },
        }
    }

    fn generation_submit_started(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::GenerationSubmitStarted { run },
        }
    }

    fn generation_unknown(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::GenerationUnknown { run },
        }
    }

    fn generation_failed(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::GenerationFailed { run },
        }
    }

    fn generation_reconciled(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::GenerationReconciled { run },
        }
    }

    fn generation_reconciled_not_found(project: &Project, run: AgentRun) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::GenerationReconciledNotFound { run },
        }
    }

    fn candidates_committed(
        project: &Project,
        run_id: AgentRunId,
        candidates: Vec<Candidate>,
    ) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::CandidatesCommitted { run_id, candidates },
        }
    }

    fn candidate_selected(project: &Project, selection: Selection) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::CandidateSelected { selection },
        }
    }

    fn handoff_exported(project: &Project, export: HandoffExport) -> Self {
        Self {
            project_id: project.id().clone(),
            project_revision: project.revision(),
            kind: ProjectEventKind::HandoffExported { export },
        }
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self.kind {
            ProjectEventKind::ProjectCreated => "project.created",
            ProjectEventKind::BriefUpdated { .. } => "brief.updated",
            ProjectEventKind::AgentRunStarted { .. } => "agent_run.started",
            ProjectEventKind::AgentRunPlanned { .. } => "agent_run.planned",
            ProjectEventKind::AgentRunFailed { .. } => "agent_run.failed",
            ProjectEventKind::AgentRunApproved { .. } => "agent_run.approved",
            ProjectEventKind::GenerationSubmitStarted { .. } => "generation.submit_started",
            ProjectEventKind::GenerationSubmitted { .. } => "generation.submitted",
            ProjectEventKind::GenerationUnknown { .. } => "generation.unknown",
            ProjectEventKind::GenerationFailed { .. } => "generation.failed",
            ProjectEventKind::GenerationReconciled { .. } => "generation.reconciled",
            ProjectEventKind::GenerationReconciledNotFound { .. } => {
                "generation.reconciled_not_found"
            }
            ProjectEventKind::CandidatesCommitted { .. } => "candidates.committed",
            ProjectEventKind::CandidateSelected { .. } => "selection.created",
            ProjectEventKind::HandoffExported { .. } => "handoff.exported",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProjectEventKind {
    ProjectCreated,
    BriefUpdated {
        brief: CreativeBrief,
    },
    AgentRunStarted {
        run: AgentRun,
    },
    AgentRunPlanned {
        run: AgentRun,
    },
    AgentRunFailed {
        run: AgentRun,
    },
    AgentRunApproved {
        run: AgentRun,
    },
    GenerationSubmitStarted {
        run: AgentRun,
    },
    GenerationSubmitted {
        run: AgentRun,
    },
    GenerationUnknown {
        run: AgentRun,
    },
    GenerationFailed {
        run: AgentRun,
    },
    GenerationReconciled {
        run: AgentRun,
    },
    GenerationReconciledNotFound {
        run: AgentRun,
    },
    CandidatesCommitted {
        run_id: AgentRunId,
        candidates: Vec<Candidate>,
    },
    CandidateSelected {
        selection: Selection,
    },
    HandoffExported {
        export: HandoffExport,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEventEnvelope {
    sequence: u64,
    event: ProjectEvent,
}

impl ProjectEventEnvelope {
    #[must_use]
    pub const fn new(sequence: u64, event: ProjectEvent) -> Self {
        Self { sequence, event }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn event(&self) -> &ProjectEvent {
        &self.event
    }
}
