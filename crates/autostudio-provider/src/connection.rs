//! Durable local LLM Connection configuration and runtime adapter.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use autostudio_core::provider::{
    LlmConnectionConfiguration, LlmConnectionControl, LlmConnectionSource, LlmConnectionStatus,
    LlmModelCatalog, LlmModelCatalogFuture, LlmModelCatalogState, LlmModelDescriptor,
    LlmProviderDescriptor, ProviderConnectionError, ThinkingLevel,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::constants::{
    LLM_CONNECTION_SCHEMA, LLM_CONNECTION_SCHEMA_V1, LLM_CONNECTION_SCHEMA_V2,
    LLM_CONNECTION_SCHEMA_V3, MAX_LLM_CONNECTION_FILE_BYTES, PROVIDER_ANTHROPIC, PROVIDER_DEEPSEEK,
    PROVIDER_KIMI_CODE, PROVIDER_KIMI_OPEN, PROVIDER_OPENAI,
};
use crate::llm::{HttpInferenceAdapter, LlmProviderConfig, LlmProviderConnection};
use crate::thinking::model_capability;
use crate::{
    AdapterError, ConnectionStoreError, InferenceAdapter, InferenceFuture,
    InferenceProviderDescriptor, InferenceTurnRequest, ProviderConfigError,
};

pub struct FileLlmConnectionManager {
    path: PathBuf,
    environment_provider: Option<String>,
}

impl FileLlmConnectionManager {
    #[must_use]
    pub fn new(path: impl AsRef<Path>, environment_provider: Option<String>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            environment_provider,
        }
    }

    fn resolve(&self) -> Result<(LlmProviderConfig, LlmConnectionSource), ProviderConnectionError> {
        if let Some(document) = self
            .read_document()
            .map_err(|error| storage_error(&error))?
        {
            return document
                .to_config()
                .map(|config| (config, LlmConnectionSource::PrivateFile))
                .map_err(|error| configuration_error(&error));
        }
        let provider = self
            .environment_provider
            .as_deref()
            .ok_or_else(unconfigured_error)?;
        LlmProviderConfig::from_environment(provider)
            .map(|config| (config, LlmConnectionSource::Environment))
            .map_err(|error| match error {
                ProviderConfigError::MissingSetting { .. } => unconfigured_error(),
                other => configuration_error(&other),
            })
    }

    fn read_document(&self) -> Result<Option<StoredConnectionDocument>, ConnectionStoreError> {
        if !self.path.exists() {
            return Ok(None);
        }
        ensure_regular_private_file(&self.path)?;
        let metadata = fs::metadata(&self.path)?;
        if metadata.len() > MAX_LLM_CONNECTION_FILE_BYTES {
            return Err(ConnectionStoreError::FileTooLarge);
        }
        let mut file = OpenOptions::new().read(true).open(&self.path)?;
        let capacity =
            usize::try_from(metadata.len()).map_err(|_| ConnectionStoreError::FileTooLarge)?;
        let mut bytes = Vec::with_capacity(capacity);
        file.read_to_end(&mut bytes)?;
        let mut document: StoredConnectionDocument = serde_json::from_slice(&bytes)?;
        if document.schema_version != LLM_CONNECTION_SCHEMA
            && document.schema_version != LLM_CONNECTION_SCHEMA_V1
            && document.schema_version != LLM_CONNECTION_SCHEMA_V2
            && document.schema_version != LLM_CONNECTION_SCHEMA_V3
        {
            return Err(ConnectionStoreError::UnsupportedSchema);
        }
        // The catalog is durable, but capabilities are executable policy owned
        // by this binary. Always recompile them so an old or manually edited
        // cache cannot make an unsupported Provider parameter selectable.
        for model in &mut document.catalog.models {
            model.thinking = model_capability(&document.provider_kind, &model.id);
        }
        document.model_thinking_levels.retain(|model, level| {
            document
                .catalog
                .models
                .iter()
                .find(|entry| entry.id == *model)
                .is_some_and(|entry| entry.thinking.supports(*level))
        });
        if let Some(model) = document.model.as_deref() {
            let capability = document
                .catalog
                .models
                .iter()
                .find(|entry| entry.id == model)
                .map_or_else(
                    || model_capability(&document.provider_kind, model),
                    |entry| entry.thinking.clone(),
                );
            if !capability.supports(document.thinking_level) {
                document.thinking_level = capability.default_level;
            }
            document
                .model_thinking_levels
                .insert(model.to_owned(), document.thinking_level);
        }
        Ok(Some(document))
    }

    fn write_document(
        &self,
        document: &StoredConnectionDocument,
    ) -> Result<(), ConnectionStoreError> {
        let parent = self
            .path
            .parent()
            .ok_or(ConnectionStoreError::MissingParent)?;
        fs::create_dir_all(parent)?;
        let lock_path = self.path.with_extension("lock");
        let lock = open_private_file(&lock_path, true, false)?;
        lock.lock_exclusive()?;
        let result = self.write_document_locked(document);
        lock.unlock()?;
        result
    }

    fn write_document_locked(
        &self,
        document: &StoredConnectionDocument,
    ) -> Result<(), ConnectionStoreError> {
        if self.path.exists() {
            ensure_regular_private_file(&self.path)?;
        }
        let temporary_path = self
            .path
            .with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
        let mut temporary = open_private_file(&temporary_path, false, true)?;
        let mut bytes = serde_json::to_vec(document)?;
        temporary.write_all(&bytes)?;
        temporary.sync_all()?;
        bytes.zeroize();
        drop(temporary);
        replace_file(&temporary_path, &self.path)?;
        set_private_permissions(&self.path)?;
        Ok(())
    }

    fn lock_file(&self) -> Result<std::fs::File, ConnectionStoreError> {
        let parent = self
            .path
            .parent()
            .ok_or(ConnectionStoreError::MissingParent)?;
        fs::create_dir_all(parent)?;
        let lock = open_private_file(&self.path.with_extension("lock"), false, false)?;
        lock.lock_exclusive()?;
        Ok(lock)
    }

    fn prepare_catalog_refresh(
        &self,
    ) -> Result<(Uuid, LlmProviderConnection), ProviderConnectionError> {
        let lock = self.lock_file().map_err(|error| storage_error(&error))?;
        let result = (|| {
            let mut document = self
                .read_document()
                .map_err(|error| storage_error(&error))?
                .ok_or_else(unconfigured_error)?;
            let connection_id = document.connection_id.unwrap_or_else(Uuid::new_v4);
            let connection = document
                .to_connection()
                .map_err(|error| configuration_error(&error))?;
            LLM_CONNECTION_SCHEMA.clone_into(&mut document.schema_version);
            document.connection_id = Some(connection_id);
            document.catalog.state = LlmModelCatalogState::Refreshing;
            document.catalog.error = None;
            self.write_document_locked(&document)
                .map_err(|error| storage_error(&error))?;
            Ok((connection_id, connection))
        })();
        lock.unlock()
            .map_err(|error| storage_error(&ConnectionStoreError::Io(error)))?;
        result
    }

    fn finish_catalog_refresh(
        &self,
        connection_id: Uuid,
        result: Result<Vec<LlmModelDescriptor>, AdapterError>,
    ) -> Result<LlmModelCatalog, ProviderConnectionError> {
        let lock = self.lock_file().map_err(|error| storage_error(&error))?;
        let update = (|| {
            let mut document = self
                .read_document()
                .map_err(|error| storage_error(&error))?
                .ok_or_else(unconfigured_error)?;
            if document.connection_id != Some(connection_id) {
                return Ok(document.catalog.clone());
            }
            match result {
                Ok(models) => {
                    document.catalog = LlmModelCatalog {
                        state: LlmModelCatalogState::Ready,
                        models,
                        error: None,
                    };
                }
                Err(error) => {
                    document.catalog.state = LlmModelCatalogState::Failed;
                    document.catalog.error = Some(error.to_string());
                }
            }
            self.write_document_locked(&document)
                .map_err(|error| storage_error(&error))?;
            Ok(document.catalog.clone())
        })();
        lock.unlock()
            .map_err(|error| storage_error(&ConnectionStoreError::Io(error)))?;
        update
    }
}

impl LlmConnectionControl for FileLlmConnectionManager {
    fn providers(&self) -> Vec<LlmProviderDescriptor> {
        [
            (PROVIDER_DEEPSEEK, "DeepSeek"),
            (PROVIDER_OPENAI, "OpenAI"),
            (PROVIDER_ANTHROPIC, "Anthropic"),
            (PROVIDER_KIMI_OPEN, "Kimi Open Platform"),
            (PROVIDER_KIMI_CODE, "Kimi Code"),
        ]
        .into_iter()
        .map(|(id, display_name)| LlmProviderDescriptor {
            id: id.to_owned(),
            display_name: display_name.to_owned(),
        })
        .collect()
    }

    fn status(&self) -> Result<LlmConnectionStatus, ProviderConnectionError> {
        if let Some(document) = self
            .read_document()
            .map_err(|error| storage_error(&error))?
        {
            return Ok(document.status());
        }
        match self.environment_provider.as_deref() {
            Some(provider) => match LlmProviderConfig::from_environment(provider) {
                Ok(config) => Ok(LlmConnectionStatus {
                    configured: true,
                    provider_kind: Some(config.provider_kind().to_owned()),
                    model: Some(config.model().to_owned()),
                    thinking_level: config.thinking_level(),
                    model_thinking_levels: BTreeMap::new(),
                    source: Some(LlmConnectionSource::Environment),
                    catalog: LlmModelCatalog {
                        state: LlmModelCatalogState::Ready,
                        models: vec![LlmModelDescriptor {
                            id: config.model().to_owned(),
                            display_name: config.model().to_owned(),
                            thinking: model_capability(provider, config.model()),
                        }],
                        error: None,
                    },
                }),
                Err(ProviderConfigError::MissingSetting { .. }) => {
                    Ok(LlmConnectionStatus::unconfigured())
                }
                Err(error) => Err(configuration_error(&error)),
            },
            None => Ok(LlmConnectionStatus::unconfigured()),
        }
    }

    fn configure(
        &self,
        configuration: LlmConnectionConfiguration,
    ) -> Result<LlmConnectionStatus, ProviderConnectionError> {
        let connection = LlmProviderConnection::from_connection(
            configuration.provider_kind(),
            configuration.api_key().to_owned(),
            configuration.base_url(),
        )
        .map_err(|error| configuration_error(&error))?;
        let model = configuration
            .model()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(model) = model.as_deref() {
            connection
                .with_model(model)
                .map_err(|error| configuration_error(&error))?;
        }
        let document = StoredConnectionDocument {
            schema_version: LLM_CONNECTION_SCHEMA.to_owned(),
            connection_id: Some(Uuid::new_v4()),
            provider_kind: connection.provider_kind().to_owned(),
            model,
            thinking_level: ThinkingLevel::default(),
            model_thinking_levels: BTreeMap::new(),
            base_url: connection
                .base_url()
                .as_str()
                .trim_end_matches('/')
                .to_owned(),
            api_key: configuration.api_key().to_owned(),
            catalog: LlmModelCatalog {
                state: LlmModelCatalogState::Refreshing,
                models: Vec::new(),
                error: None,
            },
        };
        self.write_document(&document)
            .map_err(|error| storage_error(&error))?;
        Ok(document.status())
    }

    fn model_catalog(&self) -> Result<LlmModelCatalog, ProviderConnectionError> {
        self.status().map(|status| status.catalog)
    }

    fn refresh_model_catalog(&self) -> LlmModelCatalogFuture<'_> {
        Box::pin(async move {
            let (connection_id, connection) = self.prepare_catalog_refresh()?;
            let result = connection.list_models().await;
            self.finish_catalog_refresh(connection_id, result)
        })
    }

    fn select_model(
        &self,
        model: &str,
        thinking_level: ThinkingLevel,
    ) -> Result<LlmConnectionStatus, ProviderConnectionError> {
        let model = model.trim();
        if model.is_empty() {
            return Err(ProviderConnectionError::ModelNotAvailable(model.to_owned()));
        }
        let lock = self.lock_file().map_err(|error| storage_error(&error))?;
        let result: Result<LlmConnectionStatus, ProviderConnectionError> = (|| {
            let mut document: StoredConnectionDocument = self
                .read_document()
                .map_err(|error| storage_error(&error))?
                .ok_or_else(unconfigured_error)?;
            let Some(descriptor) = document
                .catalog
                .models
                .iter()
                .find(|entry| entry.id == model)
            else {
                return Err(ProviderConnectionError::ModelNotAvailable(model.to_owned()));
            };
            if !descriptor.thinking.supports(thinking_level) {
                return Err(ProviderConnectionError::ThinkingLevelNotAvailable {
                    model: model.to_owned(),
                    level: thinking_level.as_str().to_owned(),
                });
            }
            LLM_CONNECTION_SCHEMA.clone_into(&mut document.schema_version);
            document.model = Some(model.to_owned());
            document.thinking_level = thinking_level;
            document
                .model_thinking_levels
                .insert(model.to_owned(), thinking_level);
            self.write_document_locked(&document)
                .map_err(|error| storage_error(&error))?;
            Ok(document.status())
        })();
        lock.unlock()
            .map_err(|error| storage_error(&ConnectionStoreError::Io(error)))?;
        result
    }
}

pub struct ConnectionInferenceAdapter {
    connections: Arc<FileLlmConnectionManager>,
}

impl ConnectionInferenceAdapter {
    #[must_use]
    pub fn new(connections: Arc<FileLlmConnectionManager>) -> Self {
        Self { connections }
    }
}

impl InferenceAdapter for ConnectionInferenceAdapter {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        self.connections.resolve().map_or_else(
            |_| InferenceProviderDescriptor {
                provider_kind: "unconfigured".to_owned(),
                model: "unconfigured".to_owned(),
                thinking_level: ThinkingLevel::default(),
                thinking_control: autostudio_core::provider::ThinkingControl::Unsupported,
                thinking_budget_tokens: None,
                capability_revision: "unconfigured".to_owned(),
                mapping_revision: "unconfigured".to_owned(),
                protocol: "unconfigured".to_owned(),
            },
            |(config, _)| {
                HttpInferenceAdapter::new(config).map_or_else(
                    |_| InferenceProviderDescriptor {
                        provider_kind: "unavailable".to_owned(),
                        model: "unavailable".to_owned(),
                        thinking_level: ThinkingLevel::default(),
                        thinking_control: autostudio_core::provider::ThinkingControl::Unsupported,
                        thinking_budget_tokens: None,
                        capability_revision: "unavailable".to_owned(),
                        mapping_revision: "unavailable".to_owned(),
                        protocol: "unavailable".to_owned(),
                    },
                    |adapter| adapter.descriptor(),
                )
            },
        )
    }

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        Box::pin(async move {
            let (config, _) = self
                .connections
                .resolve()
                .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
            let adapter = HttpInferenceAdapter::new(config)
                .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
            adapter.infer(request).await
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConnectionDocument {
    schema_version: String,
    #[serde(default)]
    connection_id: Option<Uuid>,
    provider_kind: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default, rename = "modelEffort")]
    thinking_level: ThinkingLevel,
    #[serde(default)]
    model_thinking_levels: BTreeMap<String, ThinkingLevel>,
    base_url: String,
    api_key: String,
    #[serde(default)]
    catalog: LlmModelCatalog,
}

impl StoredConnectionDocument {
    fn to_config(&self) -> Result<LlmProviderConfig, ProviderConfigError> {
        let model = self
            .model
            .as_deref()
            .ok_or_else(|| ProviderConfigError::MissingSetting {
                provider: self.provider_kind.clone(),
                variable: "model",
            })?;
        LlmProviderConfig::from_connection(
            &self.provider_kind,
            self.api_key.clone(),
            Some(model),
            Some(&self.base_url),
        )
        .map(|config| config.with_thinking_level(self.thinking_level))
    }

    fn to_connection(&self) -> Result<LlmProviderConnection, ProviderConfigError> {
        LlmProviderConnection::from_connection(
            &self.provider_kind,
            self.api_key.clone(),
            Some(&self.base_url),
        )
    }

    fn status(&self) -> LlmConnectionStatus {
        LlmConnectionStatus {
            configured: true,
            provider_kind: Some(self.provider_kind.clone()),
            model: self.model.clone(),
            thinking_level: self.thinking_level,
            model_thinking_levels: self.model_thinking_levels.clone(),
            source: Some(LlmConnectionSource::PrivateFile),
            catalog: self.catalog.clone(),
        }
    }
}

impl Drop for StoredConnectionDocument {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

fn unconfigured_error() -> ProviderConnectionError {
    ProviderConnectionError::NotConfigured
}

fn configuration_error(error: &ProviderConfigError) -> ProviderConnectionError {
    ProviderConnectionError::InvalidConfiguration(error.to_string())
}

fn storage_error(error: &ConnectionStoreError) -> ProviderConnectionError {
    ProviderConnectionError::StorageUnavailable(error.to_string())
}

fn open_private_file(
    path: &Path,
    truncate: bool,
    create_new: bool,
) -> Result<std::fs::File, ConnectionStoreError> {
    if path.exists() {
        ensure_regular_private_file(path)?;
    }
    let mut options = OpenOptions::new();
    options
        .read(!truncate)
        .write(true)
        .create(!create_new)
        .create_new(create_new)
        .truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    set_private_permissions(path)?;
    Ok(file)
}

fn ensure_regular_private_file(path: &Path) -> Result<(), ConnectionStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(ConnectionStoreError::InsecurePermissions);
    }
    ensure_private_permissions(path)
}

#[cfg(unix)]
fn ensure_private_permissions(path: &Path) -> Result<(), ConnectionStoreError> {
    use std::os::unix::fs::PermissionsExt;
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(ConnectionStoreError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_permissions(_path: &Path) -> Result<(), ConnectionStoreError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), ConnectionStoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), ConnectionStoreError> {
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), ConnectionStoreError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), ConnectionStoreError> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(source, destination)?;
    Ok(())
}
