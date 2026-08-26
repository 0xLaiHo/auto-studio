mod constants;

use std::env;
use std::error::Error;
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use autostudio_api::discovery::{DiscoveryFile, DiscoveryRecord};
use autostudio_api::router_with_runtime_media_backup_and_connections;
use autostudio_core::project::ProjectService;
use autostudio_core::provider::{LlmConnectionControl, LlmModelCatalogState};
use autostudio_media::ProjectMedia;
use autostudio_provider::connection::{ConnectionInferenceAdapter, FileLlmConnectionManager};
use autostudio_provider::constants::{
    CONTINUITY_JANITOR_INTERVAL, DEFAULT_CONTINUITY_TTL_MILLIS, ENV_CONTINUITY_KEY_FILE,
    ENV_CONTINUITY_ROOT, ENV_LLM_CONNECTION_FILE, ENV_LLM_PROVIDER,
};
use autostudio_provider::continuity::{ContinuityVault, FileContinuityVault};
use autostudio_provider::{AgentPlanner, ContinuityVaultError, LocalCreativeRuntime};
use autostudio_storage::{ProjectPackageBackup, SqliteProjectStore};
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::constants::{
    DEFAULT_BIND_ADDRESS, DEFAULT_LLM_PROVIDER, MAX_PARENT_HEARTBEAT_AGE,
    PARENT_HEARTBEAT_POLL_INTERVAL,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let project_package = required_path("AUTOSTUDIO_PROJECT_PACKAGE")?;
    let discovery_path = required_path("AUTOSTUDIO_DISCOVERY_FILE")?;
    let session_token =
        env::var("AUTOSTUDIO_SESSION_TOKEN").unwrap_or_else(|_| generate_session_token());
    let core_instance_id = Uuid::new_v4().to_string();
    let backup_root = env::var_os("AUTOSTUDIO_BACKUP_ROOT").map_or_else(
        || {
            project_package
                .parent()
                .unwrap_or(&project_package)
                .join("backups")
        },
        PathBuf::from,
    );

    let bind_address = env::var("AUTOSTUDIO_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.to_owned())
        .parse::<SocketAddr>()?;
    if !bind_address.ip().is_loopback() {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "Ship 0 Core only accepts a loopback bind address",
        )
        .into());
    }

    let store = Arc::new(SqliteProjectStore::open_with_owner(
        &project_package,
        &core_instance_id,
    )?);
    let projects = Arc::new(ProjectService::new(store.clone()));
    let contexts = Arc::new(autostudio_provider::context::ContextManager::new(store));
    let staging = project_package.join(".staging/provider-downloads");
    let llm_provider =
        env::var(ENV_LLM_PROVIDER).unwrap_or_else(|_| DEFAULT_LLM_PROVIDER.to_owned());
    let connection_path = env::var_os(ENV_LLM_CONNECTION_FILE).map_or_else(
        || {
            project_package
                .parent()
                .unwrap_or(&project_package)
                .join("llm-connection.json")
        },
        PathBuf::from,
    );
    let connections = Arc::new(FileLlmConnectionManager::new(
        connection_path.clone(),
        Some(llm_provider),
    ));
    if connections.status().is_ok_and(|status| {
        status.configured
            && matches!(
                status.catalog.state,
                LlmModelCatalogState::NotLoaded | LlmModelCatalogState::Refreshing
            )
    }) {
        let connections = Arc::clone(&connections);
        tokio::spawn(async move {
            let _ = connections.refresh_model_catalog().await;
        });
    }
    let continuity = continuity_vault(&project_package, &connection_path)?;
    let inference = ConnectionInferenceAdapter::new(connections.clone());
    let planner = AgentPlanner::with_continuity_vault(
        projects.clone(),
        contexts,
        Arc::new(inference),
        continuity,
    );
    let media = Arc::new(ProjectMedia::new(&project_package, &staging)?);
    let backup = Arc::new(ProjectPackageBackup::new(&project_package, &backup_root)?);
    let runtime = Arc::new(LocalCreativeRuntime::planning_only(planner));
    let app = router_with_runtime_media_backup_and_connections(
        projects,
        &session_token,
        runtime,
        media.clone(),
        media,
        backup,
        connections,
    );
    let listener = TcpListener::bind(bind_address).await?;
    let local_address = listener.local_addr()?;
    let discovery_record = DiscoveryRecord::new(
        &core_instance_id,
        std::process::id(),
        format!("http://{local_address}"),
        &session_token,
    );
    let _published_discovery = PublishedDiscovery::publish(discovery_path, &discovery_record)?;

    println!("Auto Studio Core listening on http://{local_address}");
    let parent_heartbeat = env::var_os("AUTOSTUDIO_PARENT_HEARTBEAT").map(PathBuf::from);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(parent_heartbeat))
        .await?;
    Ok(())
}

fn continuity_vault(
    project_package: &std::path::Path,
    connection_path: &std::path::Path,
) -> Result<Arc<FileContinuityVault>, ContinuityVaultError> {
    let private_root = connection_path
        .parent()
        .unwrap_or(project_package)
        .to_path_buf();
    let continuity_root = env::var_os(ENV_CONTINUITY_ROOT)
        .map_or_else(|| private_root.join("continuity"), PathBuf::from);
    let continuity_key = env::var_os(ENV_CONTINUITY_KEY_FILE)
        .map_or_else(|| private_root.join("continuity.key"), PathBuf::from);
    let continuity = Arc::new(FileContinuityVault::open_for_project(
        continuity_root,
        continuity_key,
        project_package,
        DEFAULT_CONTINUITY_TTL_MILLIS,
    )?);
    continuity.purge_expired(FileContinuityVault::now_unix_millis()?)?;
    let janitor = Arc::clone(&continuity);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CONTINUITY_JANITOR_INTERVAL).await;
            if let Ok(now) = FileContinuityVault::now_unix_millis() {
                let _ = janitor.purge_expired(now);
            }
        }
    });
    Ok(continuity)
}

async fn shutdown_signal(parent_heartbeat: Option<PathBuf>) {
    if let Some(parent_heartbeat) = parent_heartbeat {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    eprintln!("failed to install shutdown signal handler: {error}");
                }
            }
            () = parent_heartbeat_lost(parent_heartbeat) => {}
        }
    } else if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install shutdown signal handler: {error}");
    }
}

async fn parent_heartbeat_lost(path: PathBuf) {
    loop {
        tokio::time::sleep(PARENT_HEARTBEAT_POLL_INTERVAL).await;
        let fresh = path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .and_then(|modified| modified.elapsed().map_err(io::Error::other))
            .is_ok_and(|elapsed| elapsed <= MAX_PARENT_HEARTBEAT_AGE);
        if !fresh {
            return;
        }
    }
}

fn required_path(name: &str) -> Result<PathBuf, io::Error> {
    env::var_os(name).map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("{name} must point to a Project Package directory"),
        )
    })
}

fn generate_session_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

struct PublishedDiscovery {
    file: DiscoveryFile,
    core_instance_id: String,
}

impl PublishedDiscovery {
    fn publish(path: PathBuf, record: &DiscoveryRecord) -> Result<Self, Box<dyn Error>> {
        let file = DiscoveryFile::new(path);
        file.publish(record)?;
        Ok(Self {
            file,
            core_instance_id: record.core_instance_id().to_owned(),
        })
    }
}

impl Drop for PublishedDiscovery {
    fn drop(&mut self) {
        if let Err(error) = self.file.remove_if_owner(&self.core_instance_id) {
            eprintln!("failed to remove Core discovery record: {error}");
        }
    }
}
