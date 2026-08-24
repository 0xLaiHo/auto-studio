use std::net::IpAddr;
use std::path::PathBuf;

use autostudio_api::PROTOCOL_VERSION;
use autostudio_api::discovery::{DiscoveryFile, DiscoveryRecord};
use reqwest::{Client, RequestBuilder, Response, Url};
use serde_json::json;

use crate::error::{CoreErrorBody, TuiError};
use crate::model::{
    ApprovalInput, ConfigureLlmConnectionInput, CreativeBriefInput, HealthView,
    LlmConnectionStatusView, LlmModelCatalogView, LlmProviderView, ProjectView, ThinkingLevelView,
};

#[derive(Clone)]
pub struct TuiClient {
    discovery_path: PathBuf,
    http: Client,
}

impl TuiClient {
    #[must_use]
    pub fn new(discovery_path: impl Into<PathBuf>) -> Self {
        Self {
            discovery_path: discovery_path.into(),
            http: Client::new(),
        }
    }

    pub async fn health(&self) -> Result<HealthView, TuiError> {
        let connection = self.connect()?;
        let response = self.http.get(connection.url("/v1/health")?).send().await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        let health: HealthView = response.json().await?;
        if health.protocol_version != PROTOCOL_VERSION {
            return Err(TuiError::ProtocolMismatch {
                client: PROTOCOL_VERSION.to_owned(),
                core: health.protocol_version,
            });
        }
        Ok(health)
    }

    pub async fn llm_connection_status(&self) -> Result<LlmConnectionStatusView, TuiError> {
        let connection = self.connect()?;
        let response = self
            .http
            .get(connection.url("/v1/provider-connections/llm")?)
            .bearer_auth(connection.record.session_token())
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        Ok(response.json().await?)
    }

    pub async fn llm_providers(&self) -> Result<Vec<LlmProviderView>, TuiError> {
        let connection = self.connect()?;
        let response = self
            .http
            .get(connection.url("/v1/providers/llm")?)
            .bearer_auth(connection.record.session_token())
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        Ok(response.json().await?)
    }

    pub async fn configure_llm_connection(
        &self,
        configuration: &ConfigureLlmConnectionInput,
    ) -> Result<LlmConnectionStatusView, TuiError> {
        let connection = self.connect()?;
        let response = self
            .http
            .put(connection.url("/v1/provider-connections/llm")?)
            .bearer_auth(connection.record.session_token())
            .json(configuration)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        Ok(response.json().await?)
    }

    pub async fn refresh_llm_models(&self) -> Result<LlmModelCatalogView, TuiError> {
        let connection = self.connect()?;
        let response = self
            .http
            .post(connection.url("/v1/provider-connections/llm/models")?)
            .bearer_auth(connection.record.session_token())
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        Ok(response.json().await?)
    }

    pub async fn select_llm_model(
        &self,
        model: &str,
        thinking_level: ThinkingLevelView,
    ) -> Result<LlmConnectionStatusView, TuiError> {
        let connection = self.connect()?;
        let response = self
            .http
            .put(connection.url("/v1/provider-connections/llm/models")?)
            .bearer_auth(connection.record.session_token())
            .json(&json!({"model": model, "modelEffort": thinking_level}))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        Ok(response.json().await?)
    }

    pub async fn open_project(&self) -> Result<ProjectView, TuiError> {
        let connection = self.connect()?;
        self.project_response(
            self.http
                .get(connection.url("/v1/projects/current")?)
                .bearer_auth(connection.record.session_token()),
        )
        .await
    }

    pub async fn create_project(&self, name: &str) -> Result<ProjectView, TuiError> {
        let connection = self.connect()?;
        self.project_response(
            self.http
                .post(connection.url("/v1/projects")?)
                .bearer_auth(connection.record.session_token())
                .json(&json!({"name": name})),
        )
        .await
    }

    pub async fn set_brief(
        &self,
        expected_revision: u64,
        brief: CreativeBriefInput,
    ) -> Result<ProjectView, TuiError> {
        let connection = self.connect()?;
        self.project_response(
            self.http
                .put(connection.url("/v1/projects/current/brief")?)
                .bearer_auth(connection.record.session_token())
                .json(&json!({"expectedRevision": expected_revision, "brief": brief})),
        )
        .await
    }

    pub async fn plan(&self, expected_revision: u64) -> Result<ProjectView, TuiError> {
        self.post_revision("/v1/agent-runs", expected_revision)
            .await
    }

    pub async fn approve(
        &self,
        run_id: &str,
        expected_revision: u64,
        approval: ApprovalInput,
    ) -> Result<ProjectView, TuiError> {
        let connection = self.connect()?;
        self.project_response(
            self.http
                .post(connection.url(&format!("/v1/agent-runs/{run_id}/approval"))?)
                .bearer_auth(connection.record.session_token())
                .json(&json!({
                    "expectedRevision": expected_revision,
                    "approval": approval
                })),
        )
        .await
    }

    pub async fn execute(&self, run_id: &str, revision: u64) -> Result<ProjectView, TuiError> {
        self.post_revision(&format!("/v1/agent-runs/{run_id}/execute"), revision)
            .await
    }

    pub async fn reconcile(&self, run_id: &str, revision: u64) -> Result<ProjectView, TuiError> {
        self.post_revision(&format!("/v1/agent-runs/{run_id}/reconcile"), revision)
            .await
    }

    pub async fn refresh_run(&self, run_id: &str, revision: u64) -> Result<ProjectView, TuiError> {
        self.post_revision(&format!("/v1/agent-runs/{run_id}/refresh"), revision)
            .await
    }

    pub async fn select_candidate(
        &self,
        candidate_id: &str,
        revision: u64,
    ) -> Result<ProjectView, TuiError> {
        let connection = self.connect()?;
        self.project_response(
            self.http
                .post(connection.url(&format!("/v1/candidates/{candidate_id}/selection"))?)
                .bearer_auth(connection.record.session_token())
                .json(&json!({"expectedRevision": revision, "startMicros": 0})),
        )
        .await
    }

    pub async fn export_handoff(&self, revision: u64) -> Result<ProjectView, TuiError> {
        self.post_revision("/v1/handoffs", revision).await
    }

    async fn post_revision(&self, path: &str, revision: u64) -> Result<ProjectView, TuiError> {
        let connection = self.connect()?;
        self.project_response(
            self.http
                .post(connection.url(path)?)
                .bearer_auth(connection.record.session_token())
                .json(&json!({"expectedRevision": revision})),
        )
        .await
    }

    async fn project_response(&self, request: RequestBuilder) -> Result<ProjectView, TuiError> {
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(core_error(response).await);
        }
        Ok(response.json().await?)
    }

    fn connect(&self) -> Result<Connection, TuiError> {
        let record = DiscoveryFile::new(&self.discovery_path).read()?;
        if record.protocol_version() != PROTOCOL_VERSION {
            return Err(TuiError::ProtocolMismatch {
                client: PROTOCOL_VERSION.to_owned(),
                core: record.protocol_version().to_owned(),
            });
        }
        let base_url = Url::parse(record.endpoint()).map_err(|_| TuiError::InvalidEndpoint)?;
        let loopback = base_url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if base_url.scheme() != "http" || !loopback || base_url.cannot_be_a_base() {
            return Err(TuiError::InvalidEndpoint);
        }
        Ok(Connection { record, base_url })
    }
}

struct Connection {
    record: DiscoveryRecord,
    base_url: Url,
}

impl Connection {
    fn url(&self, path: &str) -> Result<Url, TuiError> {
        self.base_url
            .join(path)
            .map_err(|_| TuiError::InvalidEndpoint)
    }
}

async fn core_error(response: Response) -> TuiError {
    let status = response.status().as_u16();
    match response.json::<CoreErrorBody>().await {
        Ok(error) => TuiError::CoreRejected {
            status,
            code: error.code,
            message: error.message,
        },
        Err(_) => TuiError::CoreRejected {
            status,
            code: "invalid_core_error".to_owned(),
            message: "Core returned an unreadable error response".to_owned(),
        },
    }
}
