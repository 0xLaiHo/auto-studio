use std::collections::VecDeque;
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroize;

use crate::client::TuiClient;
use crate::constants::{DEFAULT_APPROVAL_CURRENCY, DEFAULT_PROJECT_NAME, MAX_LOG_ENTRIES};
use crate::error::TuiError;
use crate::model::{
    AgentRunStatusView, ApprovalInput, ConfigureLlmConnectionInput, CostEstimateView,
    CreativeBriefInput, LlmConnectionStatusView, LlmModelCatalogStateView, LlmProviderView,
    ProjectView, ThinkingLevelView,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextInputKind {
    ProjectName,
    Brief,
    ApprovalBudget,
}

#[derive(Clone, Eq, PartialEq)]
pub enum Overlay {
    None,
    Commands {
        selected: usize,
    },
    Providers {
        query: String,
        selected: usize,
    },
    ApiKey {
        provider: LlmProviderView,
        value: String,
    },
    Models {
        query: String,
        selected: usize,
        thinking_level: ThinkingLevelView,
    },
    TextInput {
        kind: TextInputKind,
        value: String,
    },
    Help,
}

impl fmt::Debug for Overlay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("None"),
            Self::Commands { selected } => formatter
                .debug_struct("Commands")
                .field("selected", selected)
                .finish(),
            Self::Providers { query, selected } => formatter
                .debug_struct("Providers")
                .field("query", query)
                .field("selected", selected)
                .finish(),
            Self::ApiKey { provider, .. } => formatter
                .debug_struct("ApiKey")
                .field("provider", provider)
                .field("value", &"[REDACTED]")
                .finish(),
            Self::Models {
                query,
                selected,
                thinking_level,
            } => formatter
                .debug_struct("Models")
                .field("query", query)
                .field("selected", selected)
                .field("thinking_level", thinking_level)
                .finish(),
            Self::TextInput { kind, value } => formatter
                .debug_struct("TextInput")
                .field("kind", kind)
                .field("value", value)
                .finish(),
            Self::Help => formatter.write_str("Help"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Quit,
    Insert(char),
    Backspace,
    Submit,
    Cancel,
    Previous,
    Next,
    PreviousThinking,
    NextThinking,
    OpenCommands,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    None,
    Quit,
    Refresh,
    PollProvider,
    CreateProject(String),
    SubmitCreativeRequest(String),
    SaveBrief(String),
    Plan,
    Approve(u64),
    Execute,
    Recover,
    SelectCandidate,
    ExportHandoff,
    ConfigureProvider(ConfigureLlmConnectionInput),
    RefreshModels,
    SelectModel {
        model: String,
        thinking_level: ThinkingLevelView,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub const COMMANDS: [SlashCommand; 14] = [
    SlashCommand {
        name: "/connect",
        description: "Connect an LLM provider",
    },
    SlashCommand {
        name: "/model",
        description: "Select a fetched model",
    },
    SlashCommand {
        name: "/new",
        description: "Create a project",
    },
    SlashCommand {
        name: "/brief",
        description: "Write the Creative Brief",
    },
    SlashCommand {
        name: "/plan",
        description: "Create an Agent Plan",
    },
    SlashCommand {
        name: "/approve",
        description: "Approve the current Plan",
    },
    SlashCommand {
        name: "/generate",
        description: "Start generation",
    },
    SlashCommand {
        name: "/recover",
        description: "Reconcile or refresh a Run",
    },
    SlashCommand {
        name: "/select",
        description: "Adopt the current Candidate",
    },
    SlashCommand {
        name: "/export",
        description: "Export the DAW handoff",
    },
    SlashCommand {
        name: "/refresh",
        description: "Reload Core state",
    },
    SlashCommand {
        name: "/refresh-models",
        description: "Fetch the model catalog again",
    },
    SlashCommand {
        name: "/help",
        description: "Show keyboard help",
    },
    SlashCommand {
        name: "/exit",
        description: "Exit Auto Studio",
    },
];

pub struct App {
    pub project: Option<ProjectView>,
    pub provider_status: Option<LlmConnectionStatusView>,
    pub providers: Vec<LlmProviderView>,
    pub composer: String,
    pub overlay: Overlay,
    pub candidate_index: usize,
    pub should_quit: bool,
    pub logs: VecDeque<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            project: None,
            provider_status: None,
            providers: Vec::new(),
            composer: String::new(),
            overlay: Overlay::None,
            candidate_index: 0,
            should_quit: false,
            logs: VecDeque::new(),
        }
    }
}

impl App {
    #[must_use]
    pub fn action_for_key(key: KeyEvent) -> Action {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('c') => Action::Quit,
                KeyCode::Char('p') => Action::OpenCommands,
                _ => Action::None,
            };
        }
        match key.code {
            KeyCode::Esc => Action::Cancel,
            KeyCode::Enter => Action::Submit,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Up => Action::Previous,
            KeyCode::Down => Action::Next,
            KeyCode::Left => Action::PreviousThinking,
            KeyCode::Right => Action::NextThinking,
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::ALT) => {
                Action::Insert(character)
            }
            _ => Action::None,
        }
    }

    pub fn reduce(&mut self, action: Action) -> Effect {
        match action {
            Action::Quit => Effect::Quit,
            Action::OpenCommands => {
                self.composer.clear();
                self.composer.push('/');
                self.overlay = Overlay::Commands { selected: 0 };
                Effect::None
            }
            Action::Insert(character) => {
                self.insert(character);
                Effect::None
            }
            Action::Backspace => {
                self.backspace();
                Effect::None
            }
            Action::Submit => self.submit(),
            Action::Cancel => {
                self.cancel_overlay();
                Effect::None
            }
            Action::Previous => {
                self.move_selection(false);
                Effect::None
            }
            Action::Next => {
                self.move_selection(true);
                Effect::None
            }
            Action::PreviousThinking => {
                self.move_thinking(false);
                Effect::None
            }
            Action::NextThinking => {
                self.move_thinking(true);
                Effect::None
            }
            Action::None => Effect::None,
        }
    }

    fn insert(&mut self, character: char) {
        match &mut self.overlay {
            Overlay::Providers { query, selected }
            | Overlay::Models {
                query, selected, ..
            } => {
                query.push(character);
                *selected = 0;
            }
            Overlay::ApiKey { value, .. } | Overlay::TextInput { value, .. } => {
                value.push(character);
            }
            Overlay::Help => {}
            Overlay::None | Overlay::Commands { .. } => {
                self.composer.push(character);
                if self.composer.starts_with('/') {
                    self.overlay = Overlay::Commands { selected: 0 };
                }
            }
        }
        self.sync_thinking_level_to_selected_model();
    }

    fn backspace(&mut self) {
        match &mut self.overlay {
            Overlay::Providers { query, selected }
            | Overlay::Models {
                query, selected, ..
            } => {
                query.pop();
                *selected = 0;
            }
            Overlay::ApiKey { value, .. } | Overlay::TextInput { value, .. } => {
                value.pop();
            }
            Overlay::Help => {}
            Overlay::None | Overlay::Commands { .. } => {
                self.composer.pop();
                if self.composer.is_empty() {
                    self.overlay = Overlay::None;
                }
            }
        }
        self.sync_thinking_level_to_selected_model();
    }

    fn submit(&mut self) -> Effect {
        match std::mem::replace(&mut self.overlay, Overlay::None) {
            Overlay::None => {
                let input = std::mem::take(&mut self.composer);
                if input.trim().is_empty() {
                    Effect::None
                } else if input.trim_start().starts_with('/') {
                    self.invoke_command(input.trim())
                } else {
                    Effect::SubmitCreativeRequest(input)
                }
            }
            Overlay::Commands { selected } => {
                let command = self
                    .filtered_commands()
                    .get(selected)
                    .map_or_else(|| self.composer.clone(), |command| command.name.to_owned());
                self.composer.clear();
                self.invoke_command(&command)
            }
            Overlay::Providers { query, selected } => {
                let provider = self
                    .filtered_providers(&query)
                    .get(selected)
                    .map(|provider| (*provider).clone());
                provider.map_or(Effect::None, |provider| {
                    self.overlay = Overlay::ApiKey {
                        provider,
                        value: String::new(),
                    };
                    Effect::None
                })
            }
            Overlay::ApiKey {
                provider,
                mut value,
            } => {
                if value.trim().is_empty() {
                    self.overlay = Overlay::ApiKey { provider, value };
                    return Effect::None;
                }
                let key = std::mem::take(&mut value);
                value.zeroize();
                Effect::ConfigureProvider(ConfigureLlmConnectionInput {
                    provider_kind: provider.id,
                    model: None,
                    base_url: None,
                    api_key: key,
                })
            }
            Overlay::Models {
                query,
                selected,
                thinking_level,
            } => self
                .filtered_models(&query)
                .get(selected)
                .map_or(Effect::None, |model| Effect::SelectModel {
                    model: model.id.clone(),
                    thinking_level,
                }),
            Overlay::TextInput { kind, value } => match kind {
                TextInputKind::ProjectName if !value.trim().is_empty() => {
                    Effect::CreateProject(value)
                }
                TextInputKind::Brief if !value.trim().is_empty() => Effect::SaveBrief(value),
                TextInputKind::ApprovalBudget => value
                    .trim()
                    .parse::<u64>()
                    .map_or(Effect::None, Effect::Approve),
                _ => Effect::None,
            },
            Overlay::Help => Effect::None,
        }
    }

    fn invoke_command(&mut self, command: &str) -> Effect {
        match command.split_whitespace().next().unwrap_or_default() {
            "/connect" => {
                self.overlay = Overlay::Providers {
                    query: String::new(),
                    selected: 0,
                };
                Effect::None
            }
            "/model" | "/models" => {
                let selected = self
                    .provider_status
                    .as_ref()
                    .and_then(|status| {
                        let current = status.model.as_deref()?;
                        status
                            .catalog
                            .models
                            .iter()
                            .position(|model| model.id == current)
                    })
                    .unwrap_or(0);
                let thinking_level = self
                    .provider_status
                    .as_ref()
                    .and_then(|status| status.catalog.models.get(selected))
                    .map_or(ThinkingLevelView::ProviderDefault, |model| {
                        self.preferred_thinking_level(model)
                    });
                self.overlay = Overlay::Models {
                    query: String::new(),
                    selected,
                    thinking_level,
                };
                Effect::None
            }
            "/new" => {
                self.overlay = Overlay::TextInput {
                    kind: TextInputKind::ProjectName,
                    value: String::new(),
                };
                Effect::None
            }
            "/brief" => {
                self.overlay = Overlay::TextInput {
                    kind: TextInputKind::Brief,
                    value: String::new(),
                };
                Effect::None
            }
            "/plan" => Effect::Plan,
            "/approve" => self.begin_approval(),
            "/generate" => Effect::Execute,
            "/recover" => Effect::Recover,
            "/select" => Effect::SelectCandidate,
            "/export" => Effect::ExportHandoff,
            "/refresh" => Effect::Refresh,
            "/refresh-models" => Effect::RefreshModels,
            "/help" => {
                self.overlay = Overlay::Help;
                Effect::None
            }
            "/exit" | "/quit" => Effect::Quit,
            unknown => {
                self.log(format!("未知命令：{unknown}"));
                Effect::None
            }
        }
    }

    fn begin_approval(&mut self) -> Effect {
        if self
            .project
            .as_ref()
            .and_then(|project| project.agent_runs.last())
            .is_some_and(|run| matches!(run.plan.estimated_cost, CostEstimateView::Unknown))
        {
            self.overlay = Overlay::TextInput {
                kind: TextInputKind::ApprovalBudget,
                value: "100".to_owned(),
            };
            Effect::None
        } else {
            Effect::Approve(0)
        }
    }

    fn cancel_overlay(&mut self) {
        let overlay = std::mem::replace(&mut self.overlay, Overlay::None);
        if let Overlay::ApiKey { mut value, .. } = overlay {
            value.zeroize();
        }
        if self.composer.starts_with('/') {
            self.composer.clear();
        }
    }

    fn move_selection(&mut self, forward: bool) {
        let count = match &self.overlay {
            Overlay::Commands { .. } => self.filtered_commands().len(),
            Overlay::Providers { query, .. } => self.filtered_providers(query).len(),
            Overlay::Models { query, .. } => self.filtered_models(query).len(),
            _ => 0,
        };
        let (Overlay::Commands { selected }
        | Overlay::Providers { selected, .. }
        | Overlay::Models { selected, .. }) = &mut self.overlay
        else {
            return;
        };
        if count == 0 {
            *selected = 0;
        } else if forward {
            *selected = (*selected + 1).min(count - 1);
        } else {
            *selected = selected.saturating_sub(1);
        }
        self.sync_thinking_level_to_selected_model();
    }

    fn move_thinking(&mut self, forward: bool) {
        let (levels, current) = match &self.overlay {
            Overlay::Models {
                query,
                selected,
                thinking_level,
            } => {
                let models = self.filtered_models(query);
                let Some(model) = models.get(*selected) else {
                    return;
                };
                (model.thinking.levels.clone(), *thinking_level)
            }
            _ => return,
        };
        let Overlay::Models { thinking_level, .. } = &mut self.overlay else {
            return;
        };
        let index = levels
            .iter()
            .position(|level| *level == current)
            .unwrap_or(0);
        let next = if forward {
            (index + 1).min(levels.len().saturating_sub(1))
        } else {
            index.saturating_sub(1)
        };
        if let Some(level) = levels.get(next) {
            *thinking_level = *level;
        }
    }

    fn sync_thinking_level_to_selected_model(&mut self) {
        let level = match &self.overlay {
            Overlay::Models {
                query, selected, ..
            } => self
                .filtered_models(query)
                .get(*selected)
                .map(|model| self.preferred_thinking_level(model)),
            _ => None,
        };
        if let (Some(level), Overlay::Models { thinking_level, .. }) = (level, &mut self.overlay) {
            *thinking_level = level;
        }
    }

    fn preferred_thinking_level(&self, model: &crate::model::LlmModelView) -> ThinkingLevelView {
        let Some(status) = self.provider_status.as_ref() else {
            return model.thinking.default_level;
        };
        let preferred = status
            .model_thinking_levels
            .get(&model.id)
            .copied()
            .or_else(|| {
                (status.model.as_deref() == Some(model.id.as_str()))
                    .then_some(status.thinking_level)
            });
        preferred
            .filter(|level| model.thinking.levels.contains(level))
            .unwrap_or(model.thinking.default_level)
    }

    #[must_use]
    pub fn filtered_commands(&self) -> Vec<&'static SlashCommand> {
        let query = self.composer.trim().to_ascii_lowercase();
        COMMANDS
            .iter()
            .filter(|command| command.name.starts_with(&query))
            .collect()
    }

    #[must_use]
    pub fn filtered_providers(&self, query: &str) -> Vec<&LlmProviderView> {
        let query = query.trim().to_ascii_lowercase();
        self.providers
            .iter()
            .filter(|provider| {
                query.is_empty()
                    || provider.id.to_ascii_lowercase().contains(&query)
                    || provider.display_name.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }

    #[must_use]
    pub fn filtered_models(&self, query: &str) -> Vec<&crate::model::LlmModelView> {
        let query = query.trim().to_ascii_lowercase();
        self.provider_status
            .as_ref()
            .map(|status| &status.catalog.models)
            .into_iter()
            .flatten()
            .filter(|model| {
                query.is_empty()
                    || model.id.to_ascii_lowercase().contains(&query)
                    || model.display_name.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }

    #[must_use]
    pub fn catalog_is_refreshing(&self) -> bool {
        self.provider_status
            .as_ref()
            .is_some_and(|status| status.catalog.state == LlmModelCatalogStateView::Refreshing)
    }

    pub async fn execute_effect(&mut self, client: &TuiClient, effect: Effect) {
        let result = self.try_execute_effect(client, effect).await;
        if let Err(error) = result {
            if error.is_project_not_found() {
                self.project = None;
                self.log("Core 已连接；当前还没有 Project");
            } else {
                self.log(format!("阻塞：{error}"));
            }
        }
    }

    async fn try_execute_effect(
        &mut self,
        client: &TuiClient,
        effect: Effect,
    ) -> Result<(), TuiError> {
        match effect {
            Effect::None => return Ok(()),
            Effect::Quit => {
                self.should_quit = true;
                return Ok(());
            }
            Effect::Refresh => return self.refresh(client).await,
            Effect::PollProvider => {
                self.provider_status = Some(client.llm_connection_status().await?);
                return Ok(());
            }
            Effect::ConfigureProvider(configuration) => {
                let status = client.configure_llm_connection(&configuration).await?;
                let provider = status.provider_kind.as_deref().unwrap_or("provider");
                self.log(format!("已保存 {provider} 连接；正在获取模型目录"));
                self.provider_status = Some(status);
                return Ok(());
            }
            Effect::RefreshModels => {
                client.refresh_llm_models().await?;
                self.provider_status = Some(client.llm_connection_status().await?);
                return Ok(());
            }
            Effect::SelectModel {
                model,
                thinking_level,
            } => {
                self.provider_status = Some(client.select_llm_model(&model, thinking_level).await?);
                self.log(format!(
                    "已选择模型：{model} · Thinking {}",
                    thinking_level.label()
                ));
                return Ok(());
            }
            Effect::SubmitCreativeRequest(summary) => {
                return self.submit_creative_request(client, summary).await;
            }
            _ => {}
        }
        let project = match effect {
            Effect::CreateProject(name) => client.create_project(&name).await?,
            Effect::SaveBrief(summary) => {
                let revision = self.revision()?;
                client
                    .set_brief(revision, Self::brief_from_summary(summary))
                    .await?
            }
            Effect::Plan => client.plan(self.revision()?).await?,
            Effect::Approve(maximum) => self.approve(client, maximum).await?,
            Effect::Execute => self.execute_run(client).await?,
            Effect::Recover => self.recover_run(client).await?,
            Effect::SelectCandidate => {
                let project = self.project.as_ref().ok_or(TuiError::ProjectRequired)?;
                let candidate = project
                    .candidates
                    .get(self.candidate_index)
                    .ok_or(TuiError::CandidateRequired)?;
                client
                    .select_candidate(&candidate.id, project.revision)
                    .await?
            }
            Effect::ExportHandoff => client.export_handoff(self.revision()?).await?,
            Effect::None
            | Effect::Quit
            | Effect::Refresh
            | Effect::PollProvider
            | Effect::ConfigureProvider(_)
            | Effect::RefreshModels
            | Effect::SelectModel { .. }
            | Effect::SubmitCreativeRequest(_) => unreachable!("handled before project effects"),
        };
        let revision = project.revision;
        self.project = Some(project);
        self.log(format!("Project 状态已更新到 revision {revision}"));
        Ok(())
    }

    async fn submit_creative_request(
        &mut self,
        client: &TuiClient,
        summary: String,
    ) -> Result<(), TuiError> {
        if self.project.is_none() {
            self.project = Some(client.create_project(DEFAULT_PROJECT_NAME).await?);
            self.log(format!("已创建本地 Project：{DEFAULT_PROJECT_NAME}"));
        }
        let project = client
            .set_brief(self.revision()?, Self::brief_from_summary(summary))
            .await?;
        let revision = project.revision;
        self.project = Some(project);

        let project = client.plan(revision).await?;
        let revision = project.revision;
        self.project = Some(project);
        self.log(format!("Agent 已生成 Plan · revision {revision}"));
        Ok(())
    }

    fn brief_from_summary(summary: String) -> CreativeBriefInput {
        CreativeBriefInput {
            summary,
            purpose: None,
            style: Vec::new(),
            mood: Vec::new(),
            instrumentation: Vec::new(),
            target_duration_seconds: Some(30),
            lyrics: None,
            constraints: Vec::new(),
        }
    }

    async fn refresh(&mut self, client: &TuiClient) -> Result<(), TuiError> {
        self.providers = client.llm_providers().await?;
        self.provider_status = Some(client.llm_connection_status().await?);
        match client.open_project().await {
            Ok(project) => self.project = Some(project),
            Err(error) if error.is_project_not_found() => self.project = None,
            Err(error) => return Err(error),
        }
        Ok(())
    }

    async fn approve(&self, client: &TuiClient, maximum: u64) -> Result<ProjectView, TuiError> {
        let project = self.project.as_ref().ok_or(TuiError::ProjectRequired)?;
        let run = project.agent_runs.last().ok_or(TuiError::RunRequired)?;
        let (currency, maximum) = match &run.plan.estimated_cost {
            CostEstimateView::Known {
                currency,
                upper_minor_units,
                ..
            } => (currency.clone(), *upper_minor_units),
            CostEstimateView::Unknown => (DEFAULT_APPROVAL_CURRENCY.to_owned(), maximum),
        };
        client
            .approve(
                &run.id,
                project.revision,
                ApprovalInput {
                    currency,
                    max_minor_units: maximum,
                    input_hash: run.plan.input_hash.clone(),
                },
            )
            .await
    }

    async fn execute_run(&mut self, client: &TuiClient) -> Result<ProjectView, TuiError> {
        let (run_id, revision) = self.run_identity()?;
        match client.execute(&run_id, revision).await {
            Ok(project) => Ok(project),
            Err(error) => {
                self.reload_after_partial_failure(client).await;
                Err(error)
            }
        }
    }

    async fn recover_run(&mut self, client: &TuiClient) -> Result<ProjectView, TuiError> {
        let project = self.project.as_ref().ok_or(TuiError::ProjectRequired)?;
        let run = project.agent_runs.last().ok_or(TuiError::RunRequired)?;
        let result = match run.status {
            AgentRunStatusView::UnknownOutcome => client.reconcile(&run.id, project.revision).await,
            AgentRunStatusView::Submitted => client.refresh_run(&run.id, project.revision).await,
            _ => return Err(TuiError::RunRequired),
        };
        match result {
            Ok(project) => Ok(project),
            Err(error) => {
                self.reload_after_partial_failure(client).await;
                Err(error)
            }
        }
    }

    fn revision(&self) -> Result<u64, TuiError> {
        self.project
            .as_ref()
            .map(|project| project.revision)
            .ok_or(TuiError::ProjectRequired)
    }

    fn run_identity(&self) -> Result<(String, u64), TuiError> {
        let project = self.project.as_ref().ok_or(TuiError::ProjectRequired)?;
        let run = project.agent_runs.last().ok_or(TuiError::RunRequired)?;
        Ok((run.id.clone(), project.revision))
    }

    async fn reload_after_partial_failure(&mut self, client: &TuiClient) {
        if let Ok(project) = client.open_project().await {
            self.project = Some(project);
        }
    }

    pub fn log(&mut self, message: impl Into<String>) {
        self.logs.push_back(message.into());
        while self.logs.len() > MAX_LOG_ENTRIES {
            self.logs.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Action, App, COMMANDS, Effect, Overlay};
    use crate::model::{
        LlmConnectionSourceView, LlmConnectionStatusView, LlmModelCatalogStateView,
        LlmModelCatalogView, LlmModelView, LlmProviderView, ThinkingCapabilityView,
        ThinkingControlView, ThinkingLevelView,
    };
    use std::collections::BTreeMap;

    #[test]
    fn slash_opens_the_command_palette_and_filters_commands() {
        let mut app = App::default();
        assert_eq!(app.reduce(Action::Insert('/')), Effect::None);
        assert!(matches!(app.overlay, Overlay::Commands { .. }));
        app.reduce(Action::Insert('c'));
        assert_eq!(app.filtered_commands()[0].name, "/connect");
    }

    #[test]
    fn selecting_exit_from_the_command_menu_returns_the_quit_effect() {
        let mut app = App {
            composer: "/".to_owned(),
            overlay: Overlay::Commands {
                selected: COMMANDS.len() - 1,
            },
            ..App::default()
        };

        assert_eq!(COMMANDS.last().expect("exit command").name, "/exit");
        assert_eq!(app.reduce(Action::Submit), Effect::Quit);
    }

    #[test]
    fn plain_text_without_a_project_is_not_routed_to_project_dependent_save_brief() {
        let mut app = App::default();
        for character in "Create a cinematic piano cue".chars() {
            app.reduce(Action::Insert(character));
        }

        let effect = app.reduce(Action::Submit);

        assert_eq!(
            effect,
            Effect::SubmitCreativeRequest("Create a cinematic piano cue".to_owned())
        );
    }

    #[test]
    fn connect_flow_never_places_the_key_in_debug_output() {
        let mut app = App::default();
        app.providers.push(LlmProviderView {
            id: "deepseek".to_owned(),
            display_name: "DeepSeek".to_owned(),
        });
        app.reduce(Action::OpenCommands);
        for character in "connect".chars() {
            app.reduce(Action::Insert(character));
        }
        assert_eq!(app.reduce(Action::Submit), Effect::None);
        assert_eq!(app.reduce(Action::Submit), Effect::None);
        for character in "secret-value".chars() {
            app.reduce(Action::Insert(character));
        }
        let effect = app.reduce(Action::Submit);
        let debug = format!("{effect:?}");
        assert!(matches!(effect, Effect::ConfigureProvider(_)));
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret-value"));
    }

    #[test]
    fn model_palette_selects_only_a_fetched_model() {
        let mut app = App {
            provider_status: Some(LlmConnectionStatusView {
                configured: true,
                provider_kind: Some("deepseek".to_owned()),
                model: None,
                thinking_level: ThinkingLevelView::ProviderDefault,
                model_thinking_levels: BTreeMap::new(),
                source: Some(LlmConnectionSourceView::PrivateFile),
                catalog: LlmModelCatalogView {
                    state: LlmModelCatalogStateView::Ready,
                    models: vec![LlmModelView {
                        id: "deepseek-v4-pro".to_owned(),
                        display_name: "deepseek-v4-pro".to_owned(),
                        thinking: ThinkingCapabilityView {
                            control: ThinkingControlView::Effort,
                            levels: vec![
                                ThinkingLevelView::Off,
                                ThinkingLevelView::High,
                                ThinkingLevelView::Max,
                            ],
                            default_level: ThinkingLevelView::High,
                        },
                    }],
                    error: None,
                },
            }),
            composer: "/model".to_owned(),
            overlay: Overlay::Commands { selected: 0 },
            ..App::default()
        };
        assert_eq!(app.reduce(Action::Submit), Effect::None);
        assert_eq!(app.reduce(Action::NextThinking), Effect::None);
        assert_eq!(
            app.reduce(Action::Submit),
            Effect::SelectModel {
                model: "deepseek-v4-pro".to_owned(),
                thinking_level: ThinkingLevelView::Max,
            }
        );
    }

    #[test]
    fn model_thinking_uses_only_the_selected_models_levels_without_changing_its_row() {
        let mut app = App {
            provider_status: Some(LlmConnectionStatusView {
                configured: true,
                provider_kind: Some("deepseek".to_owned()),
                model: None,
                thinking_level: ThinkingLevelView::ProviderDefault,
                model_thinking_levels: BTreeMap::new(),
                source: None,
                catalog: LlmModelCatalogView {
                    state: LlmModelCatalogStateView::Ready,
                    models: (0..3)
                        .map(|index| LlmModelView {
                            id: format!("deepseek-v4-flash-{index}"),
                            display_name: format!("DeepSeek V4 Flash {index}"),
                            thinking: ThinkingCapabilityView {
                                control: ThinkingControlView::Effort,
                                levels: vec![
                                    ThinkingLevelView::Off,
                                    ThinkingLevelView::Low,
                                    ThinkingLevelView::High,
                                    ThinkingLevelView::Max,
                                ],
                                default_level: ThinkingLevelView::High,
                            },
                        })
                        .collect(),
                    error: None,
                },
            }),
            overlay: Overlay::Models {
                query: String::new(),
                selected: 2,
                thinking_level: ThinkingLevelView::High,
            },
            ..App::default()
        };

        assert_eq!(app.reduce(Action::PreviousThinking), Effect::None);
        assert!(matches!(
            app.overlay,
            Overlay::Models {
                selected: 2,
                thinking_level: ThinkingLevelView::Low,
                ..
            }
        ));
        assert_eq!(app.reduce(Action::PreviousThinking), Effect::None);
        assert!(matches!(
            app.overlay,
            Overlay::Models {
                thinking_level: ThinkingLevelView::Off,
                ..
            }
        ));
        app.reduce(Action::NextThinking);
        app.reduce(Action::NextThinking);
        app.reduce(Action::NextThinking);
        assert!(matches!(
            app.overlay,
            Overlay::Models {
                selected: 2,
                thinking_level: ThinkingLevelView::Max,
                ..
            }
        ));
    }

    #[test]
    fn moving_between_models_restores_each_models_own_thinking_preference() {
        let unsupported = LlmModelView {
            id: "deepseek-reasoner".to_owned(),
            display_name: "DeepSeek Reasoner".to_owned(),
            thinking: ThinkingCapabilityView::default(),
        };
        let adjustable = LlmModelView {
            id: "deepseek-v4-pro".to_owned(),
            display_name: "DeepSeek V4 Pro".to_owned(),
            thinking: ThinkingCapabilityView {
                control: ThinkingControlView::Effort,
                levels: vec![
                    ThinkingLevelView::Off,
                    ThinkingLevelView::High,
                    ThinkingLevelView::Max,
                ],
                default_level: ThinkingLevelView::High,
            },
        };
        let mut preferences = BTreeMap::new();
        preferences.insert("deepseek-v4-pro".to_owned(), ThinkingLevelView::Max);
        let mut app = App {
            provider_status: Some(LlmConnectionStatusView {
                configured: true,
                provider_kind: Some("deepseek".to_owned()),
                model: Some("deepseek-reasoner".to_owned()),
                thinking_level: ThinkingLevelView::ProviderDefault,
                model_thinking_levels: preferences,
                source: None,
                catalog: LlmModelCatalogView {
                    state: LlmModelCatalogStateView::Ready,
                    models: vec![unsupported, adjustable],
                    error: None,
                },
            }),
            overlay: Overlay::Models {
                query: String::new(),
                selected: 0,
                thinking_level: ThinkingLevelView::ProviderDefault,
            },
            ..App::default()
        };

        app.reduce(Action::Next);
        assert!(matches!(
            app.overlay,
            Overlay::Models {
                selected: 1,
                thinking_level: ThinkingLevelView::Max,
                ..
            }
        ));
        app.reduce(Action::Previous);
        assert!(matches!(
            app.overlay,
            Overlay::Models {
                selected: 0,
                thinking_level: ThinkingLevelView::ProviderDefault,
                ..
            }
        ));
    }

    #[test]
    fn cancelling_model_selection_discards_both_model_and_thinking_draft() {
        let mut app = App {
            provider_status: Some(LlmConnectionStatusView {
                configured: true,
                provider_kind: Some("deepseek".to_owned()),
                model: Some("deepseek-v4-pro".to_owned()),
                thinking_level: ThinkingLevelView::High,
                model_thinking_levels: BTreeMap::new(),
                source: None,
                catalog: LlmModelCatalogView {
                    state: LlmModelCatalogStateView::Ready,
                    models: Vec::new(),
                    error: None,
                },
            }),
            overlay: Overlay::Models {
                query: String::new(),
                selected: 0,
                thinking_level: ThinkingLevelView::Max,
            },
            ..App::default()
        };

        assert_eq!(app.reduce(Action::Cancel), Effect::None);
        assert_eq!(app.overlay, Overlay::None);
        assert_eq!(
            app.provider_status
                .as_ref()
                .map(|status| status.thinking_level),
            Some(ThinkingLevelView::High)
        );
    }

    #[test]
    fn escape_closes_a_modal_instead_of_quitting() {
        assert_eq!(
            App::action_for_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Action::Cancel
        );
    }
}
