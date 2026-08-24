mod constants;
pub mod core_client;
mod error;
mod managed_core;

use std::path::PathBuf;

use core_client::{CoreClient, CoreStatus, CreativeBriefInput, ProjectBackupView, ProjectView};
use tauri::{Manager, State};

use crate::constants::{ENV_DISCOVERY_FILE, ENV_MANAGE_CORE, ENV_PROJECT_PACKAGE};
use crate::error::CommandError;

struct DesktopState {
    core: CoreClient,
    _managed_core: Option<managed_core::ManagedCore>,
}

#[tauri::command]
async fn core_status(state: State<'_, DesktopState>) -> Result<CoreStatus, CommandError> {
    state.core.clone().status().await.map_err(Into::into)
}

#[tauri::command]
async fn create_project(
    name: String,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .create_project(&name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn open_project(state: State<'_, DesktopState>) -> Result<ProjectView, CommandError> {
    state.core.clone().open_project().await.map_err(Into::into)
}

#[tauri::command]
async fn backup_project(
    expected_revision: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectBackupView, CommandError> {
    state
        .core
        .clone()
        .backup_project(expected_revision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn set_brief(
    expected_revision: u64,
    brief: CreativeBriefInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .set_brief(expected_revision, brief)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn plan_agent_run(
    expected_revision: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .plan_agent_run(expected_revision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn approve_agent_run(
    run_id: String,
    expected_revision: u64,
    currency: String,
    max_minor_units: u64,
    input_hash: String,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .approve_agent_run(
            &run_id,
            expected_revision,
            &currency,
            max_minor_units,
            &input_hash,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn execute_agent_run(
    run_id: String,
    expected_revision: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .execute_agent_run(&run_id, expected_revision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn reconcile_agent_run(
    run_id: String,
    expected_revision: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .reconcile_agent_run(&run_id, expected_revision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn refresh_agent_run(
    run_id: String,
    expected_revision: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .refresh_agent_run(&run_id, expected_revision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn select_candidate(
    candidate_id: String,
    expected_revision: u64,
    start_micros: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .select_candidate(&candidate_id, expected_revision, start_micros)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn export_handoff(
    expected_revision: u64,
    state: State<'_, DesktopState>,
) -> Result<ProjectView, CommandError> {
    state
        .core
        .clone()
        .export_handoff(expected_revision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn preview_asset(
    asset_version_id: String,
    state: State<'_, DesktopState>,
) -> Result<tauri::ipc::Response, CommandError> {
    let bytes = state
        .core
        .clone()
        .preview_asset(&asset_version_id)
        .await
        .map_err(CommandError::from)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Auto Studio Desktop application.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the desktop process.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app.path().app_local_data_dir()?;
            let discovery_path = std::env::var_os(ENV_DISCOVERY_FILE)
                .map_or_else(|| app_data.join("runtime/core.json"), PathBuf::from);
            let project_package = std::env::var_os(ENV_PROJECT_PACKAGE).map_or_else(
                || app_data.join("projects/ship-zero.autostudio"),
                PathBuf::from,
            );
            let managed_core = if std::env::var_os(ENV_MANAGE_CORE).as_deref()
                == Some(std::ffi::OsStr::new("0"))
            {
                None
            } else {
                Some(managed_core::ManagedCore::launch(
                    &project_package,
                    &discovery_path,
                )?)
            };
            app.manage(DesktopState {
                core: CoreClient::new(discovery_path),
                _managed_core: managed_core,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            create_project,
            open_project,
            backup_project,
            set_brief,
            plan_agent_run,
            approve_agent_run,
            execute_agent_run,
            reconcile_agent_run,
            refresh_agent_run,
            select_candidate,
            export_handoff,
            preview_asset
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Auto Studio Desktop");
}
