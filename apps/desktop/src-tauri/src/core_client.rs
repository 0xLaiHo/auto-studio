use std::net::IpAddr;
use std::path::{Path, PathBuf};

use autostudio_api::discovery::{DiscoveryFile, DiscoveryRecord};
use autostudio_api::{PROTOCOL_VERSION, SCHEMA_VERSION};
use reqwest::{Client, Response, Url};
use serde::{Deserialize, Serialize};

use crate::constants::MAX_PREVIEW_BYTES;
use crate::error::ApiErrorResponse;
pub use crate::error::CoreClientError;

#[derive(Clone)]
pub struct CoreClient {
    discovery_path: PathBuf,
    http: Client,
}

impl CoreClient {
    #[must_use]
    pub fn new(discovery_path: impl AsRef<Path>) -> Self {
        Self {
            discovery_path: discovery_path.as_ref().to_path_buf(),
            http: Client::new(),
        }
    }

    /// Discovers Core and verifies its protocol before returning connection status.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when discovery is unsafe, Core is unreachable,
    /// or the Desktop and Core protocol versions differ.
    pub async fn status(&self) -> Result<CoreStatus, CoreClientError> {
        let connection = self.connect().await?;
        Ok(CoreStatus {
            core_instance_id: connection.record.core_instance_id().to_owned(),
            core_pid: connection.record.core_pid(),
            core_version: connection.core_version,
            protocol_version: PROTOCOL_VERSION.to_owned(),
            schema_version: SCHEMA_VERSION.to_owned(),
        })
    }

    /// Creates the first Project through the authenticated loopback interface.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when connection, authorization, validation, or
    /// persistence fails.
    pub async fn create_project(&self, name: &str) -> Result<ProjectView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .post(connection.url("/v1/projects")?)
            .bearer_auth(connection.record.session_token())
            .json(&CreateProjectRequest { name })
            .send()
            .await?;
        parse_project(response).await
    }

    /// Reopens the Project currently owned by Core.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when connection, authorization, or persistence
    /// fails.
    pub async fn open_project(&self) -> Result<ProjectView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .get(connection.url("/v1/projects/current")?)
            .bearer_auth(connection.record.session_token())
            .send()
            .await?;
        parse_project(response).await
    }

    /// Creates an application-local consistent Project Package backup.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the revision is stale or Core cannot
    /// validate and atomically publish the copy.
    pub async fn backup_project(
        &self,
        expected_revision: u64,
    ) -> Result<ProjectBackupView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .post(connection.url("/v1/projects/current/backup")?)
            .bearer_auth(connection.record.session_token())
            .json(&ExpectedRevisionRequest { expected_revision })
            .send()
            .await?;
        if response.status().is_success() {
            return response.json().await.map_err(Into::into);
        }
        Err(parse_core_error(response).await)
    }

    /// Saves a versioned Creative Brief through Core.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when connection, validation, revision, or persistence fails.
    pub async fn set_brief(
        &self,
        expected_revision: u64,
        brief: CreativeBriefInput,
    ) -> Result<ProjectView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .put(connection.url("/v1/projects/current/brief")?)
            .bearer_auth(connection.record.session_token())
            .json(&SetBriefRequest {
                expected_revision,
                brief,
            })
            .send()
            .await?;
        parse_project(response).await
    }

    /// Asks the Creative Agent to persist a visible plan for the current Brief.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when connection, inference, revision, or persistence fails.
    pub async fn plan_agent_run(
        &self,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        self.post_revision("/v1/agent-runs", expected_revision)
            .await
    }

    /// Resumes a durable Planning Run from Project and transcript facts.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the Run cannot be resumed safely.
    pub async fn resume_planning_run(
        &self,
        run_id: &str,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        self.post_revision(
            &format!("/v1/agent-runs/{run_id}/resume"),
            expected_revision,
        )
        .await
    }

    /// Records Creator Approval for one unchanged plan and cost ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the plan changed, the budget is insufficient,
    /// the revision is stale, or Core is unavailable.
    pub async fn approve_agent_run(
        &self,
        run_id: &str,
        expected_revision: u64,
        currency: &str,
        max_minor_units: u64,
        input_hash: &str,
    ) -> Result<ProjectView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .post(connection.url(&format!("/v1/agent-runs/{run_id}/approval"))?)
            .bearer_auth(connection.record.session_token())
            .json(&ApproveAgentRunRequest {
                expected_revision,
                approval: CostApprovalInput {
                    currency,
                    max_minor_units,
                    input_hash,
                },
            })
            .send()
            .await?;
        parse_project(response).await
    }

    /// Executes one approved Agent Run through the configured Music Provider.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] for an invalid transition, unknown Provider
    /// outcome, media failure, stale revision, or unavailable Core.
    pub async fn execute_agent_run(
        &self,
        run_id: &str,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        self.post_revision(
            &format!("/v1/agent-runs/{run_id}/execute"),
            expected_revision,
        )
        .await
    }

    /// Reconciles an Unknown Outcome without issuing a second Provider submit.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the Run is not unknown, Provider evidence
    /// cannot be obtained, the revision is stale, or recovery persistence fails.
    pub async fn reconcile_agent_run(
        &self,
        run_id: &str,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        self.post_revision(
            &format!("/v1/agent-runs/{run_id}/reconcile"),
            expected_revision,
        )
        .await
    }

    /// Polls a submitted Provider Job and commits ready results.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the Run is not submitted, Provider polling
    /// fails, its adapter changed, or result persistence fails.
    pub async fn refresh_agent_run(
        &self,
        run_id: &str,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        self.post_revision(
            &format!("/v1/agent-runs/{run_id}/refresh"),
            expected_revision,
        )
        .await
    }

    /// Applies an explicit Candidate Selection to the Audio Clip Timeline.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the Candidate is absent, the revision is
    /// stale, or Core cannot persist the Selection.
    pub async fn select_candidate(
        &self,
        candidate_id: &str,
        expected_revision: u64,
        start_micros: u64,
    ) -> Result<ProjectView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .post(connection.url(&format!("/v1/candidates/{candidate_id}/selection"))?)
            .bearer_auth(connection.record.session_token())
            .json(&SelectCandidateRequest {
                expected_revision,
                start_micros,
            })
            .send()
            .await?;
        parse_project(response).await
    }

    /// Exports the current Selection as a DAW Handoff Package through Core.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when connection, authorization, revision,
    /// selection, media validation, or publication fails.
    pub async fn export_handoff(
        &self,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        self.post_revision("/v1/handoffs", expected_revision).await
    }

    /// Reads a verified Audio Asset through Core without exposing its token or path.
    ///
    /// # Errors
    ///
    /// Returns [`CoreClientError`] when the Asset is absent, changed, too large,
    /// is not WAV audio, or Core rejects the request.
    pub async fn preview_asset(&self, asset_version_id: &str) -> Result<Vec<u8>, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .get(connection.url(&format!("/v1/assets/{asset_version_id}/preview"))?)
            .bearer_auth(connection.record.session_token())
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(parse_core_error(response).await);
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            != Some("audio/wav")
        {
            return Err(CoreClientError::InvalidPreviewMediaType);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PREVIEW_BYTES)
        {
            return Err(CoreClientError::PreviewTooLarge);
        }
        let bytes = response.bytes().await?;
        if bytes.len() as u64 > MAX_PREVIEW_BYTES {
            return Err(CoreClientError::PreviewTooLarge);
        }
        Ok(bytes.to_vec())
    }

    async fn post_revision(
        &self,
        path: &str,
        expected_revision: u64,
    ) -> Result<ProjectView, CoreClientError> {
        let connection = self.connect().await?;
        let response = self
            .http
            .post(connection.url(path)?)
            .bearer_auth(connection.record.session_token())
            .json(&ExpectedRevisionRequest { expected_revision })
            .send()
            .await?;
        parse_project(response).await
    }

    async fn connect(&self) -> Result<CoreConnection, CoreClientError> {
        let record = DiscoveryFile::new(&self.discovery_path).read()?;
        if record.protocol_version() != PROTOCOL_VERSION {
            return Err(CoreClientError::ProtocolMismatch {
                desktop: PROTOCOL_VERSION.to_owned(),
                core: record.protocol_version().to_owned(),
            });
        }

        let endpoint = validate_loopback_endpoint(record.endpoint())?;
        let health = self
            .http
            .get(endpoint.join("/v1/health").map_err(invalid_endpoint)?)
            .send()
            .await?;
        if !health.status().is_success() {
            return Err(CoreClientError::UnexpectedHealthStatus(
                health.status().as_u16(),
            ));
        }
        let health: HealthResponse = health.json().await?;
        if health.status != "ok" {
            return Err(CoreClientError::UnhealthyCore);
        }
        if health.protocol_version != PROTOCOL_VERSION {
            return Err(CoreClientError::ProtocolMismatch {
                desktop: PROTOCOL_VERSION.to_owned(),
                core: health.protocol_version,
            });
        }
        if health.schema_version != SCHEMA_VERSION {
            return Err(CoreClientError::SchemaMismatch {
                desktop: SCHEMA_VERSION.to_owned(),
                core: health.schema_version,
            });
        }

        Ok(CoreConnection {
            record,
            endpoint,
            core_version: health.core_version,
        })
    }
}

fn validate_loopback_endpoint(value: &str) -> Result<Url, CoreClientError> {
    let endpoint = Url::parse(value).map_err(invalid_endpoint)?;
    let is_plain_loopback = endpoint.scheme() == "http"
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.path() == "/"
        && endpoint.query().is_none()
        && endpoint.fragment().is_none()
        && endpoint.port().is_some()
        && endpoint
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());

    if is_plain_loopback {
        Ok(endpoint)
    } else {
        Err(CoreClientError::InvalidEndpoint)
    }
}

fn invalid_endpoint<T>(_error: T) -> CoreClientError {
    CoreClientError::InvalidEndpoint
}

async fn parse_project(response: Response) -> Result<ProjectView, CoreClientError> {
    let status = response.status();
    if status.is_success() {
        return response.json().await.map_err(Into::into);
    }

    Err(parse_core_error(response).await)
}

async fn parse_core_error(response: Response) -> CoreClientError {
    let status = response.status();
    let body = response.json::<ApiErrorResponse>().await.ok();
    CoreClientError::CoreRejected {
        status: status.as_u16(),
        code: body.as_ref().map_or_else(
            || "core_request_failed".to_owned(),
            |body| body.code.clone(),
        ),
        message: body.map_or_else(
            || "Core rejected the request".to_owned(),
            |body| body.message,
        ),
    }
}

struct CoreConnection {
    record: DiscoveryRecord,
    endpoint: Url,
    core_version: String,
}

impl CoreConnection {
    fn url(&self, path: &str) -> Result<Url, CoreClientError> {
        self.endpoint.join(path).map_err(invalid_endpoint)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreStatus {
    pub core_instance_id: String,
    pub core_pid: u32,
    pub core_version: String,
    pub protocol_version: String,
    pub schema_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: String,
    pub name: String,
    pub revision: u64,
    #[serde(default)]
    pub brief: Option<CreativeBriefInput>,
    #[serde(default)]
    pub agent_runs: Vec<AgentRunView>,
    #[serde(default)]
    pub candidates: Vec<CandidateView>,
    #[serde(default)]
    pub selection: Option<SelectionView>,
    #[serde(default)]
    pub timeline: TimelineView,
    #[serde(default)]
    pub exports: Vec<HandoffExportView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackupView {
    pub id: String,
    pub source_project_id: String,
    pub source_project_revision: u64,
    pub backup_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunView {
    pub id: String,
    pub status: String,
    pub plan: Option<AgentPlanView>,
    pub approval: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPlanView {
    pub visible_summary: String,
    pub decision: serde_json::Value,
    pub estimated_cost: serde_json::Value,
    pub usage: serde_json::Value,
    pub input_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateView {
    pub id: String,
    pub label: String,
    pub asset: AssetVersionView,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersionView {
    pub id: String,
    pub relative_path: String,
    pub sha256: String,
    pub media_type: String,
    pub audio: AudioMetadataView,
    pub provenance: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioMetadataView {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub duration_micros: u64,
    pub bit_depth: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionView {
    pub id: String,
    pub candidate_id: String,
    pub project_revision: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineView {
    pub clips: Vec<AudioClipView>,
    pub tempo_hint_bpm: Option<u16>,
    pub key_hint: Option<String>,
    pub markers_micros: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioClipView {
    pub id: String,
    pub asset_version_id: String,
    pub start_micros: u64,
    pub source_in_micros: u64,
    pub duration_micros: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffExportView {
    pub id: String,
    pub source_project_revision: u64,
    pub selection_id: String,
    pub relative_path: String,
    pub manifest_sha256: String,
    pub files: Vec<HandoffFileView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffFileView {
    pub relative_path: String,
    pub sha256: String,
    pub media_type: String,
}

#[derive(Serialize)]
struct CreateProjectRequest<'a> {
    name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetBriefRequest {
    expected_revision: u64,
    brief: CreativeBriefInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedRevisionRequest {
    expected_revision: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApproveAgentRunRequest<'a> {
    expected_revision: u64,
    approval: CostApprovalInput<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CostApprovalInput<'a> {
    currency: &'a str,
    max_minor_units: u64,
    input_hash: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectCandidateRequest {
    expected_revision: u64,
    start_micros: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: String,
    core_version: String,
    protocol_version: String,
    schema_version: String,
}
