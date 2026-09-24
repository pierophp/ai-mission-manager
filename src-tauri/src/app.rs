//! Stable Tauri command facade and the single shared application Runtime.
//!
//! Feature modules own workflow implementations. This module intentionally
//! keeps command names, argument shapes, and serialized return types stable
//! for the frontend and for Tauri's generated command wrappers.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::{AppHandle, State};

#[cfg(test)]
use crate::domain::Event;

use crate::{
    dependencies::resolve_executable,
    domain::{
        ActivityTabView, AgentKind, AuditEntry, Context, ContextAttentionDefault, DomainState,
        ExecutionMode, ExecutionProfile, ExternalChangePolicy, ExternalLinkView,
        ExternalObjectKind, ExternalSnapshot, GrillAnswer, GrillConfiguration,
        GrillContinuationAction, HomeView, Item, ItemRelation, ItemRelationKind, ItemStatus,
        ItemView, Machine, MachineTransport, Project, Repository, Run, RunCheckout,
        RunPromptSelection, RunState, RunSuggestion, Worktree,
    },
    persistence::SqliteStore,
    provider::resolve_gh_executable,
    terminal::{MachineReadiness, PaneSummary, TerminalRuntime, TmuxControlPane, TmuxRuntime},
};

pub(crate) use crate::features::deletion::{
    ExternalLinkDeletionResult, ExternalObjectDeletionPreview, ExternalObjectDeletionResult,
    ItemDeletionPreview, ItemDeletionResult, MachineDeletionPreview, MachineDeletionResult,
    ParentDeletionPreview, ParentDeletionResult, RepositoryDeletionPreview,
    RepositoryDeletionResult, ResetLocalDataPreview, ResetLocalDataResult, WorktreeRemovalReport,
    WorktreeRemovalResult,
};
pub use crate::features::setup::{HealthStatus, ProviderChoice, SetupState};
pub(crate) use crate::features::work::{ExternalLinkAction, PollResult};

pub struct Runtime {
    pub(crate) store: SqliteStore,
    pub(crate) state: DomainState,
    pub(crate) gh_executable_path: Option<PathBuf>,
    pub(crate) pending_worktree_removals: HashMap<i64, WorktreeRemovalReport>,
    pub(crate) pending_item_deletion: Option<ItemDeletionPreview>,
    pub(crate) pending_external_object_deletion: Option<ExternalObjectDeletionPreview>,
    pub(crate) pending_repository_deletion: Option<RepositoryDeletionPreview>,
    pub(crate) pending_machine_deletion: Option<MachineDeletionPreview>,
    pub(crate) pending_parent_deletion: Option<ParentDeletionPreview>,
    pub(crate) pending_reset_local_data: Option<ResetLocalDataPreview>,
    pub(crate) terminal_connections: HashMap<String, TmuxControlPane>,
    pub(crate) terminal_runtime: Arc<dyn TerminalRuntime>,
    pub(crate) reconciliation_in_progress: Arc<AtomicBool>,
    pub(crate) machine_readiness: HashMap<i64, MachineReadiness>,
    pub(crate) legacy_agent_state_directory: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MachineSettingsView {
    #[serde(flatten)]
    pub machine: Machine,
    pub readiness: Option<MachineReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectRunCheckoutPreview {
    pub repository_id: i64,
    pub repository_name: String,
    pub path: String,
    pub branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectRunSharedRun {
    pub run_id: i64,
    pub item_id: i64,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectRunPreview {
    pub workspace_id: i64,
    pub machine_id: i64,
    pub machine_name: String,
    pub working_directory: String,
    pub checkouts: Vec<RunCheckout>,
    pub checkout_details: Vec<DirectRunCheckoutPreview>,
    pub current_branches: Vec<String>,
    pub dirty_repository_ids: Vec<i64>,
    pub shared_runs: Vec<DirectRunSharedRun>,
    pub shared_paths: Vec<String>,
}

impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open_with_terminal_runtime(path, TmuxRuntime)
    }

    pub(crate) fn open_with_terminal_runtime(
        path: impl AsRef<Path>,
        terminal_runtime: impl TerminalRuntime + 'static,
    ) -> Result<Self, String> {
        let database_path = path.as_ref().to_path_buf();
        let legacy_agent_state_directory = database_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("agent-state");
        let mut store = SqliteStore::open(&database_path).map_err(|error| error.to_string())?;
        let state = store.load_state().map_err(|error| error.to_string())?;
        let configured_gh_path = store
            .gh_executable_path()
            .map_err(|error| error.to_string())?;
        let gh_executable_path = resolve_gh_executable(configured_gh_path.as_deref()).ok();
        if gh_executable_path != configured_gh_path {
            if let Some(path) = gh_executable_path.as_deref() {
                store
                    .set_gh_executable_path(path)
                    .map_err(|error| error.to_string())?;
            }
        }
        let mut runtime = Self {
            store,
            state,
            gh_executable_path,
            pending_worktree_removals: HashMap::new(),
            pending_item_deletion: None,
            pending_external_object_deletion: None,
            pending_repository_deletion: None,
            pending_machine_deletion: None,
            pending_parent_deletion: None,
            pending_reset_local_data: None,
            terminal_connections: HashMap::new(),
            terminal_runtime: Arc::new(terminal_runtime),
            reconciliation_in_progress: Arc::new(AtomicBool::new(false)),
            machine_readiness: HashMap::new(),
            legacy_agent_state_directory,
        };
        runtime.ensure_project_workspaces()?;
        runtime.recover_run_states()?;
        Ok(runtime)
    }

    pub(crate) fn resolve_and_store_executable(
        &mut self,
        name: &str,
        setting_key: &str,
    ) -> Result<Option<PathBuf>, String> {
        let stored = self
            .store
            .executable_path(setting_key)
            .map_err(|error| error.to_string())?;
        let path = resolve_executable(name, stored.as_deref());
        if let Some(path) = path.as_deref() {
            if stored.as_deref() != Some(path) {
                self.store
                    .set_executable_path(setting_key, path)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(path)
    }

    pub(crate) fn machine_settings_view(&self, machine: &Machine) -> MachineSettingsView {
        MachineSettingsView {
            machine: machine.clone(),
            readiness: self.machine_readiness.get(&machine.id).cloned(),
        }
    }

    pub(crate) fn preferred_agent_executable(
        &mut self,
        machine: &Machine,
        agent: AgentKind,
    ) -> Result<Option<PathBuf>, String> {
        let name = agent.slug();
        let setting_key = if matches!(&machine.transport, MachineTransport::Local) {
            format!("{name}_executable_path")
        } else {
            format!("machine_{}_{}_executable_path", machine.id, name)
        };
        if matches!(&machine.transport, MachineTransport::Local) {
            return self.resolve_and_store_executable(name, &setting_key);
        }
        Ok(None)
    }

    pub(crate) fn store_agent_executable(
        &mut self,
        machine: &Machine,
        agent: AgentKind,
        executable: &Path,
    ) -> Result<(), String> {
        let name = agent.slug();
        let setting_key = if matches!(&machine.transport, MachineTransport::Local) {
            format!("{name}_executable_path")
        } else {
            format!("machine_{}_{}_executable_path", machine.id, name)
        };
        if !matches!(&machine.transport, MachineTransport::Local) && !executable.is_absolute() {
            return Err(format!(
                "Machine {} returned a non-absolute {name} executable path",
                machine.name
            ));
        }
        self.store
            .set_executable_path(&setting_key, executable)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn machine_for_item(
        &mut self,
        item_id: i64,
        machine_id: Option<i64>,
    ) -> Result<Machine, String> {
        let context_id = self.item_context_id(item_id)?;
        let context = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        let configured_machine_id = context.execution_machine_id.ok_or_else(|| {
            format!(
                "Context {} has no execution Machine configured. Choose one in Settings → Machines before running this Item.",
                context.name
            )
        })?;
        if machine_id.is_some_and(|machine_id| machine_id != configured_machine_id) {
            return Err(format!(
                "Runs in Context {} must use its configured execution Machine",
                context.name
            ));
        }
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == configured_machine_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "The execution Machine configured for Context {} no longer exists",
                    context.name
                )
            })?;
        if machine.context_id != context_id {
            return Err(format!(
                "The execution Machine configured for Context {} is not owned by that Context",
                context.name,
            ));
        }
        Ok(machine)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDeletionResult {
    pub run_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneTab {
    pub pane_id: String,
    pub session_name: String,
    pub run_id: i64,
    pub label: String,
    pub available: bool,
    pub pane_index: u32,
    pub pid: u32,
    pub columns: u16,
    pub rows: u16,
    pub title: String,
    pub current_command: String,
    pub current_path: String,
}

impl PaneTab {
    pub(crate) fn from_summary(
        run: &Run,
        session_name: &str,
        pane: PaneSummary,
        available: bool,
    ) -> Self {
        let label = if pane.pane_id == run.pane_id {
            format!("Run #{}", run.id)
        } else {
            format!("Pane {}", pane.pane_index)
        };
        Self {
            pane_id: pane.pane_id,
            session_name: session_name.to_owned(),
            run_id: run.id,
            label,
            available,
            pane_index: pane.pane_index,
            pid: pane.pid,
            columns: pane.columns,
            rows: pane.rows,
            title: pane.title,
            current_command: pane.current_command,
            current_path: pane.current_path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachment {
    pub terminal_id: String,
    pub session_name: String,
    pub pane_id: String,
    pub snapshot: Vec<u8>,
    pub panes: Vec<PaneTab>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    pub terminal_id: String,
    pub pane_id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitEvent {
    pub terminal_id: String,
    pub pane_id: String,
    pub code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStateChangedEvent {
    pub run_id: i64,
    pub state: RunState,
}

pub(crate) fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_contexts(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Context>, String> {
    crate::features::structure::list_contexts(state)
}

#[tauri::command]
pub fn list_grill_model_catalog() -> Vec<crate::domain::GrillAgentCatalog> {
    crate::features::setup::list_grill_model_catalog()
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_context_grill_defaults(
    context_id: i64,
    defaults: GrillConfiguration,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    crate::features::structure::set_context_grill_defaults(context_id, defaults, state)
}

#[tauri::command]
pub fn list_projects(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Project>, String> {
    crate::features::structure::list_projects(state)
}

#[tauri::command]
pub fn list_repositories(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Repository>, String> {
    crate::features::structure::list_repositories(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_repository_locations(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<crate::domain::RepositoryLocation>, String> {
    crate::features::structure::list_repository_locations(state)
}

#[tauri::command]
pub fn list_machines(state: State<'_, Mutex<Runtime>>) -> Result<Vec<MachineSettingsView>, String> {
    crate::features::structure::list_machines(state)
}

#[tauri::command]
pub fn get_setup_state(state: State<'_, Mutex<Runtime>>) -> Result<SetupState, String> {
    crate::features::setup::get_setup_state(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn complete_setup(
    context_name: String,
    provider: ProviderChoice,
    state: State<'_, Mutex<Runtime>>,
) -> Result<SetupState, String> {
    crate::features::setup::complete_setup(state, context_name, provider)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_health_status(
    provider: Option<ProviderChoice>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HealthStatus, String> {
    crate::features::setup::get_health_status(state, provider)
}

#[tauri::command(rename_all = "camelCase")]
pub fn compose_run_prompt(
    item_id: i64,
    execution_profile: ExecutionProfile,
    prompt_selection: RunPromptSelection,
    custom_prompt: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<String, String> {
    crate::features::work::compose_run_prompt(
        item_id,
        execution_profile,
        prompt_selection,
        custom_prompt,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn compose_grill_prompt(
    item_id: i64,
    configuration: GrillConfiguration,
    initial_prompt: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<String, String> {
    crate::features::work::compose_grill_prompt(item_id, configuration, initial_prompt, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_direct_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    crate::features::work::prepare_direct_run(item_id, workspace_id, machine_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_grill_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    crate::features::work::prepare_grill_run(item_id, workspace_id, machine_id, state)
}

#[tauri::command(rename_all = "camelCase")]
#[allow(clippy::too_many_arguments)]
pub fn start_direct_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    crate::features::work::start_direct_run(
        item_id,
        workspace_id,
        machine_id,
        primary_repository_id,
        agent,
        execution_profile,
        prompt,
        prompt_selection,
        expected_checkouts,
        allow_dirty,
        allow_shared_checkouts,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
#[allow(clippy::too_many_arguments)]
pub fn start_grill_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    configuration: GrillConfiguration,
    initial_prompt: String,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    crate::features::work::start_grill_run(
        item_id,
        workspace_id,
        machine_id,
        primary_repository_id,
        configuration,
        initial_prompt,
        expected_checkouts,
        allow_dirty,
        allow_shared_checkouts,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
#[allow(clippy::too_many_arguments)]
pub fn start_worktree_run(
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    crate::features::work::start_worktree_run(
        item_id,
        workspace_id,
        worktree_id,
        agent,
        execution_profile,
        prompt,
        prompt_selection,
        state,
    )
}

#[tauri::command]
pub fn list_run_suggestions(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<RunSuggestion>, String> {
    crate::features::work::list_run_suggestions(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn attach_run(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    crate::features::work::attach_run(suggestion, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn stop_untracked_agent(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    crate::features::work::stop_untracked_agent(suggestion, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_untracked_agent(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    crate::features::work::delete_untracked_agent(suggestion, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn register_machine(
    context_id: i64,
    name: String,
    socket_name: String,
    transport: MachineTransport,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Machine, String> {
    crate::features::structure::register_machine(context_id, name, socket_name, transport, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn update_machine(
    machine_id: i64,
    name: String,
    socket_name: String,
    transport: MachineTransport,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Machine, String> {
    crate::features::structure::update_machine(machine_id, name, socket_name, transport, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn check_machine(
    machine_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineSettingsView, String> {
    crate::features::structure::check_machine(machine_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn open_terminal(
    run_id: i64,
    terminal_id: String,
    session_name: String,
    pane_id: String,
    app: AppHandle,
    state: State<'_, Mutex<Runtime>>,
) -> Result<TerminalAttachment, String> {
    crate::features::work::open_terminal(&app, run_id, terminal_id, session_name, pane_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn terminal_input(
    terminal_id: String,
    input: Vec<u8>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    crate::features::work::terminal_input(terminal_id, input, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn submit_grill_answers(
    run_id: i64,
    answers: Vec<GrillAnswer>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    crate::features::work::submit_grill_answers(run_id, answers, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn continue_grill(
    run_id: i64,
    action: GrillContinuationAction,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    crate::features::work::continue_grill(run_id, action, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn terminal_resize(
    terminal_id: String,
    columns: u16,
    rows: u16,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    crate::features::work::terminal_resize(terminal_id, columns, rows, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn close_terminal(terminal_id: String, state: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    crate::features::work::close_terminal(terminal_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn open_external_terminal(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    crate::features::work::open_external_terminal(run_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn stop_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    crate::features::work::stop_run(run_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn finish_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    crate::features::work::finish_run(run_id, state)
}

#[tauri::command]
pub fn list_audit_history(state: State<'_, Mutex<Runtime>>) -> Result<Vec<AuditEntry>, String> {
    crate::features::activity::list_audit_history(state)
}

#[tauri::command]
pub fn get_activity_tab(state: State<'_, Mutex<Runtime>>) -> Result<ActivityTabView, String> {
    crate::features::activity::get_activity_tab(state)
}

#[tauri::command]
pub fn list_context_attention_defaults(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ContextAttentionDefault>, String> {
    crate::features::structure::list_context_attention_defaults(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_home(
    context_id: Option<i64>,
    now: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HomeView, String> {
    crate::features::work::get_home(context_id, now, state)
}

#[tauri::command]
pub async fn reconcile_runs(
    app: AppHandle,
    state: State<'_, Mutex<Runtime>>,
) -> Result<crate::terminal::RunReconciliationResult, String> {
    crate::features::work::reconcile_runs_with_state(state.inner(), Some(app)).await
}

#[tauri::command(rename_all = "camelCase")]
pub fn search_items_command(
    query: String,
    context_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ItemView>, String> {
    crate::features::work::search_items_command(query, context_id, state)
}

#[tauri::command]
pub fn create_context(name: String, state: State<'_, Mutex<Runtime>>) -> Result<Context, String> {
    crate::features::structure::create_context(name, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn update_context(
    context_id: i64,
    name: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    crate::features::structure::update_context(context_id, name, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_context_execution_machine(
    context_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    crate::features::structure::set_context_execution_machine(context_id, machine_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_project(
    name: String,
    context_id: i64,
    default_item_status: ItemStatus,
    execution_mode: Option<ExecutionMode>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    crate::features::structure::create_project(
        name,
        context_id,
        default_item_status,
        execution_mode,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn update_project(
    project_id: i64,
    name: String,
    default_item_status: ItemStatus,
    execution_mode: Option<ExecutionMode>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    crate::features::structure::update_project(
        project_id,
        name,
        default_item_status,
        execution_mode,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn register_repository(
    project_id: i64,
    name: String,
    remote_url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    crate::features::structure::register_repository(project_id, name, remote_url, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn update_repository(
    repository_id: i64,
    name: String,
    remote_url: String,
    base_branch: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    crate::features::structure::update_repository(
        repository_id,
        name,
        remote_url,
        base_branch,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
#[allow(clippy::too_many_arguments)]
pub fn register_repository_at_location(
    project_id: i64,
    name: String,
    remote_url: Option<String>,
    base_branch: String,
    machine_id: i64,
    checkout_path: String,
    worktree_root: Option<String>,
    clone_into_destination: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    crate::features::structure::register_repository_at_location(
        project_id,
        name,
        remote_url,
        base_branch,
        machine_id,
        checkout_path,
        worktree_root,
        clone_into_destination,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn update_repository_location(
    repository_id: i64,
    previous_machine_id: Option<i64>,
    machine_id: i64,
    checkout_path: String,
    worktree_root: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<crate::domain::RepositoryLocation, String> {
    crate::features::structure::update_repository_location(
        repository_id,
        previous_machine_id,
        machine_id,
        checkout_path,
        worktree_root,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_project_deletion(
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionPreview, String> {
    crate::features::structure::prepare_project_deletion(project_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_project(
    project_id: i64,
    item_ids: Vec<i64>,
    repository_ids: Vec<i64>,
    workspace_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionResult, String> {
    crate::features::structure::delete_project(
        project_id,
        item_ids,
        repository_ids,
        workspace_ids,
        confirmed,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_context_deletion(
    context_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionPreview, String> {
    crate::features::structure::prepare_context_deletion(context_id, state)
}

#[tauri::command(rename_all = "camelCase")]
#[allow(clippy::too_many_arguments)]
pub fn delete_context(
    context_id: i64,
    project_ids: Vec<i64>,
    item_ids: Vec<i64>,
    repository_ids: Vec<i64>,
    workspace_ids: Vec<i64>,
    machine_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionResult, String> {
    crate::features::structure::delete_context(
        context_id,
        project_ids,
        item_ids,
        repository_ids,
        workspace_ids,
        machine_ids,
        confirmed,
        state,
    )
}

#[tauri::command]
pub fn prepare_reset_local_data(
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataPreview, String> {
    crate::features::structure::prepare_reset_local_data(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn reset_all_local_data(
    confirmation: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataResult, String> {
    crate::features::structure::reset_all_local_data(confirmation, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    path: String,
    branch: String,
    base_branch: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    crate::features::work::create_worktree(
        workspace_id,
        repository_id,
        machine_id,
        path,
        branch,
        base_branch,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    reuse_existing_branch: bool,
    confirm_dirty_attachment: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    crate::features::work::prepare_worktree(
        workspace_id,
        repository_id,
        machine_id,
        reuse_existing_branch,
        confirm_dirty_attachment,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_worktree_removal(
    worktree_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalReport, String> {
    crate::features::work::prepare_worktree_removal(worktree_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_worktree(
    worktree_id: i64,
    confirmed: bool,
    destructive_confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalResult, String> {
    crate::features::work::remove_worktree(worktree_id, confirmed, destructive_confirmed, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn attach_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    path: String,
    confirm_dirty_attachment: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    crate::features::work::attach_worktree(
        workspace_id,
        repository_id,
        machine_id,
        path,
        confirm_dirty_attachment,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_repository_deletion(
    repository_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionPreview, String> {
    crate::features::structure::prepare_repository_deletion(repository_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_repository(
    repository_id: i64,
    workspace_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionResult, String> {
    crate::features::structure::delete_repository(repository_id, workspace_ids, confirmed, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_machine_deletion(
    machine_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineDeletionPreview, String> {
    crate::features::structure::prepare_machine_deletion(machine_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_machine(
    machine_id: i64,
    run_ids: Vec<i64>,
    worktree_ids: Vec<i64>,
    repository_location_repository_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineDeletionResult, String> {
    crate::features::structure::delete_machine(
        machine_id,
        run_ids,
        worktree_ids,
        repository_location_repository_ids,
        confirmed,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_item_deletion(
    item_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionPreview, String> {
    crate::features::work::prepare_item_deletion(item_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_item(
    item_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionResult, String> {
    crate::features::work::delete_item(item_id, confirmed, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_external_object_deletion(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalObjectDeletionPreview, String> {
    crate::features::work::prepare_external_object_deletion(external_object_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_external_object(
    external_object_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalObjectDeletionResult, String> {
    crate::features::work::delete_external_object(external_object_id, confirmed, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn unlink_external_link(
    link_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkDeletionResult, String> {
    crate::features::work::unlink_external_link(link_id, confirmed, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_run(
    run_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RunDeletionResult, String> {
    crate::features::work::delete_run(run_id, confirmed, state)
}

#[tauri::command]
pub fn list_inbox_items(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Item>, String> {
    crate::features::work::list_inbox_items(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_item(
    title: String,
    context_id: i64,
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    crate::features::work::create_item(title, context_id, project_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_status(
    item_id: i64,
    status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    crate::features::work::set_item_status(item_id, status, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_title(
    item_id: i64,
    title: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    crate::features::work::set_item_title(item_id, title, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_notes(
    item_id: i64,
    notes: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    crate::features::work::set_item_notes(item_id, notes, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_item_reminder(
    item_id: i64,
    remind_at: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    crate::features::work::add_item_reminder(item_id, remind_at, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_item_reminder(
    item_id: i64,
    reminder_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    crate::features::work::remove_item_reminder(item_id, reminder_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_relation(
    from_item_id: i64,
    to_item_id: i64,
    kind: ItemRelationKind,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemRelation, String> {
    crate::features::work::set_item_relation(from_item_id, to_item_id, kind, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn link_external_object(
    item_id: i64,
    url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    crate::features::work::link_external_object(item_id, url, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_github_issue(
    item_id: i64,
    repository_id: i64,
    title: String,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    crate::features::work::create_github_issue(item_id, repository_id, title, body, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_external_comment(
    link_id: i64,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    crate::features::work::add_external_comment(link_id, body, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn refresh_external_object(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalSnapshot, String> {
    crate::features::work::refresh_external_object(external_object_id, state)
}

#[tauri::command]
pub fn poll_external_objects(state: State<'_, Mutex<Runtime>>) -> Result<PollResult, String> {
    crate::features::work::poll_external_objects(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_attention_policy(
    link_id: i64,
    policy: Option<ExternalChangePolicy>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    crate::features::work::set_link_attention_policy(link_id, policy, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_watch_until(
    link_id: i64,
    watch_until: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    crate::features::work::set_link_watch_until(link_id, watch_until, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_review_at(
    link_id: i64,
    review_at: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    crate::features::work::set_link_review_at(link_id, review_at, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn clear_link_review_at(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    crate::features::work::clear_link_review_at(link_id, state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_context_attention_default(
    context_id: i64,
    object_kind: ExternalObjectKind,
    policy: ExternalChangePolicy,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ContextAttentionDefault, String> {
    crate::features::structure::set_context_attention_default(
        context_id,
        object_kind,
        policy,
        state,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn mark_link_reviewed(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    crate::features::work::mark_link_reviewed(link_id, state)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::Path,
        process::Command,
        sync::{mpsc, Arc, Mutex},
        thread,
        time::Duration,
    };

    use tempfile::tempdir;

    use super::*;
    use crate::domain::{
        parse_grill_question_group, search_items, AuditAction, GrillPhase, MachineObservation,
        ProjectDefaults, RunPaneStatus, GRILL_SKILL_SNAPSHOT,
    };
    use crate::features::deletion::RESET_CONFIRMATION_PHRASE;
    use crate::persistence::SqliteStore;
    use crate::terminal::{
        FakeMachineOutcome, FakeTerminalCommand, FakeTerminalRuntime, MachineObservationFailureKind,
    };

    fn runtime_with_single_reconciliation_run(
        database: &Path,
        terminal: FakeTerminalRuntime,
    ) -> Runtime {
        let directory = database
            .parent()
            .expect("the test database should have a parent directory");
        let mut runtime = Runtime::open_with_terminal_runtime(database, terminal)
            .expect("runtime should open with the fake terminal runtime");
        runtime
            .create_item("Reconcile this Run".into(), 1, 1)
            .expect("the Run Item should be created");
        runtime.state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Machine".into(),
            socket_name: "local-machine".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        runtime.state.runs.push(Run {
            id: 1,
            item_id: 1,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Implement,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: "Implement the change".into(),
            working_directory: directory.to_string_lossy().into_owned(),
            session_name: "session-1".into(),
            pane_id: "%1".into(),
            started_at: 1,
            state: RunState::Working,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Unknown,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
        });
        runtime
    }

    #[test]
    fn project_repositories_are_implicitly_available_to_items_and_persist() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .register_repository(
                1,
                "service-a".into(),
                "https://example.com/service-a.git".into(),
            )
            .expect("first Repository should register");
        runtime
            .register_repository(
                1,
                "service-b".into(),
                "https://example.com/service-b.git".into(),
            )
            .expect("second Repository should register");
        runtime
            .create_item("Keep two lines of work reusable".into(), 1, 1)
            .expect("Item should be created");

        assert_eq!(runtime.state.workspaces.len(), 1);
        let workspace = &runtime.state.workspaces[0];
        assert_eq!(workspace.repositories.len(), 2);
        assert_eq!(workspace.repositories[0].repository_id, 1);
        assert_eq!(workspace.repositories[1].repository_id, 2);
        assert!(workspace.repositories.iter().all(|repository| {
            repository.branch == "mission-MC-1" && repository.base_branch == "main"
        }));
        assert!(runtime
            .state
            .workspaces
            .iter()
            .all(|workspace| workspace.item_id == 1));

        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.workspaces, runtime.state.workspaces);
        let item_view = search_items(&reopened.state, "two lines", None)
            .into_iter()
            .next()
            .expect("the Item view should be searchable after reopening");
        assert_eq!(item_view.workspaces, reopened.state.workspaces);
        assert_eq!(item_view.workspaces[0].repositories.len(), 2);
    }

    #[test]
    fn reset_requires_the_typed_confirmation_and_keeps_the_model_when_rejected() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .complete_setup("Personal".into(), ProviderChoice::GitHub)
            .expect("setup should complete");
        let before = runtime.state.clone();

        let preview = runtime
            .prepare_reset_local_data()
            .expect("reset preview should be available");
        assert_eq!(preview.confirmation_phrase, RESET_CONFIRMATION_PHRASE);
        let error = runtime
            .reset_all_local_data("RESET".into())
            .expect_err("a weaker confirmation should be rejected");

        assert!(error.contains(RESET_CONFIRMATION_PHRASE));
        assert_eq!(runtime.state, before);
        assert!(runtime.pending_reset_local_data.is_some());
    }

    #[test]
    fn reset_recreates_a_usable_personal_context_and_preserves_setup_configuration() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .complete_setup("Personal".into(), ProviderChoice::GitHub)
            .expect("setup should complete");
        runtime
            .store
            .set_executable_path("tmux_executable_path", Path::new("/custom/tmux"))
            .expect("dependency configuration should persist");
        runtime
            .create_context("Work".into())
            .expect("extra Context should be created");

        let preview = runtime
            .prepare_reset_local_data()
            .expect("reset preview should be available");
        assert_eq!(preview.plan.summary.context_count, 2);
        assert_eq!(preview.plan.summary.project_count, 2);
        assert_eq!(preview.audit_entry_count, 2);
        let result = runtime
            .reset_all_local_data(RESET_CONFIRMATION_PHRASE.into())
            .expect("reset should succeed");

        assert_eq!(result.audit_entry_count, 2);
        assert_eq!(runtime.state.contexts.len(), 1);
        assert_eq!(runtime.state.contexts[0].name, "Personal");
        assert_eq!(runtime.state.contexts[0].id, 3);
        assert_eq!(runtime.state.projects.len(), 1);
        assert_eq!(runtime.state.projects[0].name, "Default");
        assert_eq!(runtime.state.projects[0].id, 3);
        assert_eq!(
            runtime
                .store
                .executable_path("tmux_executable_path")
                .expect("dependency configuration should remain readable")
                .unwrap(),
            Path::new("/custom/tmux")
        );
        let setup = runtime
            .setup_state()
            .expect("setup state should remain readable");
        assert_eq!(setup.provider, ProviderChoice::GitHub);
        assert!(setup.completed);
        let history = runtime
            .list_audit_history()
            .expect("reset boundary should be readable");
        assert!(matches!(
            history.as_slice(),
            [AuditEntry {
                action: AuditAction::ResetBoundary {
                    context_id: 3,
                    project_id: 3,
                },
                ..
            }]
        ));
    }

    #[test]
    fn completing_an_item_leaves_active_runs_running_and_records_the_action() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Keep the agent running".into(), 1, 1)
            .expect("Item should be created");
        runtime.state.runs.push(Run {
            id: 1,
            item_id: 1,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Implement,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: "private prompt that must not enter the audit history".into(),
            working_directory: "/private/working-directory".into(),
            session_name: "private-session".into(),
            pane_id: "%1".into(),
            started_at: 1,
            state: RunState::Working,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Available,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
        });

        let item = runtime
            .update_item(
                Event::SetItemStatus {
                    item_id: 1,
                    status: ItemStatus::Done,
                },
                1,
            )
            .expect("explicit completion should succeed");

        assert_eq!(item.status, ItemStatus::Done);
        assert_eq!(runtime.state.runs[0].state, RunState::Working);
        assert_eq!(runtime.state.runs[0].pane_status, RunPaneStatus::Available);
        let history = runtime
            .list_audit_history()
            .expect("audit history should be available");
        assert!(history.iter().any(|entry| {
            matches!(
                entry.action,
                AuditAction::ItemStatusChanged {
                    item_id: 1,
                    to: ItemStatus::Done,
                    ..
                }
            )
        }));
        let serialized = serde_json::to_string(&history).expect("audit history should serialize");
        assert!(!serialized.contains("private prompt"));
        assert!(!serialized.contains("private-session"));
        assert!(!serialized.contains("/private/working-directory"));
    }

    #[test]
    fn stopping_a_run_is_explicit_and_only_marks_its_pane_missing() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let socket = format!("mission-manager-stop-{}", std::process::id());
        let session = format!("stop-run-{}", std::process::id());
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "new-session",
            "-d",
            "-s",
            &session,
        ]);
        let pane_id = run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "display-message",
            "-p",
            "-t",
            &session,
            "#{pane_id}",
        ]);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime.state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: socket,
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        runtime.state.runs.push(Run {
            id: 1,
            item_id: 1,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Implement,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: "private prompt".into(),
            working_directory: "/tmp".into(),
            session_name: session,
            pane_id,
            started_at: 1,
            state: RunState::Working,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Available,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
        });

        let stopped = runtime
            .stop_run(1)
            .expect("the explicit stop should succeed");

        assert_eq!(stopped.state, RunState::Working);
        assert_eq!(stopped.pane_status, RunPaneStatus::Missing);
        assert!(runtime
            .list_audit_history()
            .expect("audit history should be available")
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::RunStopped { run_id: 1 })));
    }

    #[test]
    fn opening_external_terminal_reports_a_missing_run_pane_before_launching_anything() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime.state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: format!("mission-manager-missing-pane-{}", std::process::id()),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        runtime.state.runs.push(Run {
            id: 1,
            item_id: 1,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Implement,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: "Open the missing Pane".into(),
            working_directory: directory.path().to_string_lossy().into_owned(),
            session_name: "missing-session".into(),
            pane_id: "%99".into(),
            started_at: 1,
            state: RunState::Unknown,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Unknown,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
        });

        let error = runtime
            .open_external_terminal(1)
            .expect_err("a missing Pane must stop before Terminal.app launches");

        assert!(error.contains("Pane %99 is not available in session missing-session"));
    }

    #[test]
    fn startup_reconciliation_marks_known_and_missing_panes_without_creating_runs() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let socket = format!("mission-manager-reconcile-{}", std::process::id());
        let session = format!("reconcile-{}", std::process::id());
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "new-session",
            "-d",
            "-s",
            &session,
            "-c",
            directory
                .path()
                .to_str()
                .expect("temporary path should be valid"),
        ]);
        let pane_id = run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "display-message",
            "-p",
            "-t",
            &session,
            "#{pane_id}",
        ]);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime.state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: socket.clone(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        runtime.state.runs.extend([
            Run {
                id: 1,
                item_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Implement,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt: "Keep the known Run".into(),
                working_directory: directory.path().to_string_lossy().into_owned(),
                session_name: session.clone(),
                pane_id: pane_id.clone(),
                started_at: 1,
                state: RunState::Working,
                last_applied_agent_state_sequence: None,
                pane_status: RunPaneStatus::Unknown,
                workspace_id: None,
                repository_id: None,
                worktree_id: None,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            },
            Run {
                id: 2,
                item_id: 1,
                machine_id: 1,
                agent: AgentKind::Codex,
                execution_profile: ExecutionProfile::Review,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt: "Keep the missing Run".into(),
                working_directory: directory.path().to_string_lossy().into_owned(),
                session_name: session.clone(),
                pane_id: "%999".into(),
                started_at: 2,
                state: RunState::Blocked,
                last_applied_agent_state_sequence: None,
                pane_status: RunPaneStatus::Unknown,
                workspace_id: None,
                repository_id: None,
                worktree_id: None,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            },
            Run {
                id: 3,
                item_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Investigate,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt: "Keep the missing session Run".into(),
                working_directory: directory.path().to_string_lossy().into_owned(),
                session_name: "gone-session".into(),
                pane_id: "%1".into(),
                started_at: 3,
                state: RunState::Finished,
                last_applied_agent_state_sequence: None,
                pane_status: RunPaneStatus::Unknown,
                workspace_id: None,
                repository_id: None,
                worktree_id: None,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            },
        ]);

        runtime
            .reconcile_runs()
            .expect("startup reconciliation should complete");

        assert_eq!(runtime.state.runs.len(), 3);
        assert_eq!(runtime.state.runs[0].pane_status, RunPaneStatus::Available);
        assert_eq!(runtime.state.runs[1].pane_status, RunPaneStatus::Missing);
        assert_eq!(runtime.state.runs[1].state, RunState::Blocked);
        assert_eq!(runtime.state.runs[2].pane_status, RunPaneStatus::Missing);
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "kill-session",
            "-t",
            &session,
        ]);
    }

    #[test]
    fn reconciliation_uses_the_injected_terminal_runtime_per_machine() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let terminal = FakeTerminalRuntime::new([
            (1, FakeMachineOutcome::Available),
            (2, FakeMachineOutcome::Unreachable),
            (3, FakeMachineOutcome::TmuxQueryFailed),
        ]);
        let commands = terminal.command_log();
        let mut runtime = Runtime::open_with_terminal_runtime(&database, terminal)
            .expect("runtime should open with the fake terminal runtime");

        runtime.state.machines.extend((1..=3).map(|id| Machine {
            id,
            context_id: 1,
            name: format!("Machine {id}"),
            socket_name: format!("machine-{id}"),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        }));
        let make_run = |id, machine_id, pane_id: &str| Run {
            id,
            item_id: 1,
            machine_id,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Implement,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: format!("Run {id}"),
            working_directory: directory.path().to_string_lossy().into_owned(),
            session_name: format!("session-{machine_id}"),
            pane_id: pane_id.into(),
            started_at: id,
            state: RunState::Working,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Unknown,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
        };
        runtime.state.runs.extend([
            make_run(1, 1, "%1"),
            make_run(2, 1, "%missing"),
            make_run(3, 2, "%1"),
            make_run(4, 3, "%1"),
        ]);

        let result = runtime
            .reconcile_runs()
            .expect("Run reconciliation should complete through the fake");

        assert_eq!(runtime.state.runs[0].pane_status, RunPaneStatus::Available);
        assert_eq!(runtime.state.runs[1].pane_status, RunPaneStatus::Missing);
        assert_eq!(runtime.state.runs[2].pane_status, RunPaneStatus::Unknown);
        assert_eq!(runtime.state.runs[3].pane_status, RunPaneStatus::Unknown);
        assert!(runtime
            .state
            .runs
            .iter()
            .all(|run| run.state == RunState::Working));
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures[0].machine_id, 2);
        assert_eq!(
            result.failures[0].kind,
            MachineObservationFailureKind::Unreachable
        );
        assert_eq!(result.failures[1].machine_id, 3);
        assert_eq!(
            result.failures[1].kind,
            MachineObservationFailureKind::TmuxQueryFailed
        );
        let recorded_commands = commands
            .lock()
            .expect("fake terminal command log should remain available")
            .clone();
        assert_eq!(
            recorded_commands,
            vec![
                FakeTerminalCommand::ObserveMachine { machine_id: 1 },
                FakeTerminalCommand::ObserveMachine { machine_id: 2 },
                FakeTerminalCommand::ObserveMachine { machine_id: 3 },
            ]
        );
    }

    #[test]
    fn creating_an_item_does_not_wait_for_run_machine_observation() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let terminal = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
        let observation = terminal.block_observations();
        let runtime = Arc::new(Mutex::new(runtime_with_single_reconciliation_run(
            &database, terminal,
        )));
        let reconciliation_runtime = Arc::clone(&runtime);
        let reconciliation = thread::spawn(move || {
            tauri::async_runtime::block_on(crate::features::work::reconcile_runs_with_state(
                &reconciliation_runtime,
                None,
            ))
            .expect("Run reconciliation should complete")
        });

        assert!(observation.wait_until_started(Duration::from_secs(2)));
        let (created_tx, created_rx) = mpsc::channel();
        let create_runtime = Arc::clone(&runtime);
        let create_item = thread::spawn(move || {
            let mut runtime = create_runtime
                .lock()
                .expect("runtime should remain available");
            let result = runtime
                .update_item(
                    Event::SetItemNotes {
                        item_id: 1,
                        notes: "Updated while reconciliation was observing".into(),
                    },
                    1,
                )
                .and_then(|_| runtime.create_item("Concurrent Item".into(), 1, 1));
            created_tx
                .send(result.is_ok())
                .expect("the Item creation result should be received");
        });

        let created_while_observation_was_blocked = created_rx
            .recv_timeout(Duration::from_millis(250))
            .unwrap_or(false);
        observation.release();
        reconciliation
            .join()
            .expect("reconciliation thread should finish");
        create_item
            .join()
            .expect("Item creation thread should finish");

        assert!(
            created_while_observation_was_blocked,
            "creating an Item should not wait for the Machine observation"
        );
    }

    #[test]
    fn reconciliation_discards_observations_for_deleted_or_retargeted_runs() {
        let cases = ["deleted", "machine", "session", "pane"];
        for case in cases {
            let directory = tempdir().expect("temporary app directory should exist");
            let database = directory.path().join("mission-manager.sqlite");
            let terminal = FakeTerminalRuntime::new([
                (1, FakeMachineOutcome::Available),
                (2, FakeMachineOutcome::Available),
            ]);
            let mut runtime = runtime_with_single_reconciliation_run(&database, terminal);
            if case == "machine" {
                runtime.state.machines.push(Machine {
                    id: 2,
                    context_id: 1,
                    name: "Other Machine".into(),
                    socket_name: "other-machine".into(),
                    transport: MachineTransport::Local,
                    last_observed: MachineObservation::Unknown,
                    last_observed_at: None,
                });
            }
            let observations = runtime.reconciliation_snapshot().observe();
            match case {
                "deleted" => runtime.state.runs.clear(),
                "machine" => runtime.state.runs[0].machine_id = 2,
                "session" => runtime.state.runs[0].session_name = "session-now".into(),
                "pane" => runtime.state.runs[0].pane_id = "%2".into(),
                _ => unreachable!(),
            }

            let applied = runtime
                .apply_reconciliation(observations)
                .expect("stale reconciliation observations should be discarded");

            if case == "deleted" {
                assert!(runtime.state.runs.is_empty());
            } else {
                assert_eq!(runtime.state.runs[0].pane_status, RunPaneStatus::Unknown);
            }
            assert!(
                !applied.result.changed,
                "stale {case} observation must not apply"
            );
        }
    }

    #[test]
    fn pane_id_reported_under_another_session_is_discarded_without_grill_capture() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let terminal = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
        let commands = terminal.command_log();
        terminal.set_observed_panes(
            1,
            vec![crate::terminal::ObservedPane {
                session_name: "session-2".into(),
                pane_id: "%1".into(),
            }],
        );
        let mut runtime = runtime_with_single_reconciliation_run(&database, terminal);
        runtime.state.runs[0].execution_profile = ExecutionProfile::Grill;
        runtime.state.runs[0].pane_status = RunPaneStatus::Available;
        let observations = runtime.reconciliation_snapshot().observe();

        let applied = runtime
            .apply_reconciliation(observations)
            .expect("pane identity should apply through the observation seam");

        assert!(!applied.result.changed);
        assert!(applied.changed_runs.is_empty());
        assert_eq!(runtime.state.runs[0].pane_status, RunPaneStatus::Available);
        assert_eq!(
            commands
                .lock()
                .expect("fake command log should remain available")
                .as_slice(),
            &[FakeTerminalCommand::ObserveMachine { machine_id: 1 }],
            "the stale pane must not be used to capture the Grill transcript"
        );
    }

    #[test]
    fn reconciliation_is_single_flight_and_idempotent() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let terminal = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
        let observation = terminal.block_observations();
        let runtime = Arc::new(Mutex::new(runtime_with_single_reconciliation_run(
            &database, terminal,
        )));

        let first_runtime = Arc::clone(&runtime);
        let first = thread::spawn(move || {
            tauri::async_runtime::block_on(crate::features::work::reconcile_runs_with_state(
                &first_runtime,
                None,
            ))
            .expect("the first reconciliation should finish")
        });
        assert!(observation.wait_until_started(Duration::from_secs(2)));

        let overlapping = tauri::async_runtime::block_on(
            crate::features::work::reconcile_runs_with_state(&runtime, None),
        )
        .expect("an overlapping reconciliation should return immediately");
        assert!(!overlapping.changed);

        observation.release();
        let first = first
            .join()
            .expect("the first reconciliation thread should join");
        assert!(first.changed, "the first pass should update pane_status");
        assert_eq!(
            runtime
                .lock()
                .expect("runtime should remain available")
                .state
                .runs[0]
                .pane_status,
            RunPaneStatus::Available
        );

        let settled = tauri::async_runtime::block_on(
            crate::features::work::reconcile_runs_with_state(&runtime, None),
        )
        .expect("a settled reconciliation should complete");
        assert!(
            !settled.changed,
            "an idempotent pass should report no changes"
        );
    }

    #[test]
    fn grill_reconciliation_restores_transcript_and_keeps_answers_across_pane_loss() {
        let directory = tempdir().expect("temporary directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let socket = format!("mission-manager-grill-reconcile-{}", std::process::id());
        let session = format!("grill-reconcile-{}", std::process::id());
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "new-session",
            "-d",
            "-s",
            &session,
            "-c",
            directory
                .path()
                .to_str()
                .expect("temporary path should be valid"),
        ]);
        let pane_id = run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "display-message",
            "-p",
            "-t",
            &session,
            "#{pane_id}",
        ]);
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "send-keys",
            "-t",
            &pane_id,
            "printf '\\342\\235\\223 Q1: Keep this decision?\\n'",
            "Enter",
        ]);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime.state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: socket.clone(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        let question_group = parse_grill_question_group("❓ Q1: Keep this decision?")
            .expect("the saved Grill question should parse");
        runtime.state.runs.push(Run {
            id: 1,
            item_id: 1,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Grill,
            model: Some("claude-sonnet-4-5".into()),
            effort: Some("high".into()),
            skill_snapshot: Some(GRILL_SKILL_SNAPSHOT.into()),
            prompt: "Recover this Grill".into(),
            working_directory: directory.path().to_string_lossy().into_owned(),
            session_name: session.clone(),
            pane_id: pane_id.clone(),
            started_at: 1,
            state: RunState::Working,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Unknown,
            workspace_id: Some(1),
            repository_id: Some(1),
            worktree_id: None,
            direct_checkouts: vec![RunCheckout {
                repository_id: 1,
                path: directory.path().to_string_lossy().into_owned(),
                branch: "main".into(),
                is_dirty: false,
            }],
            transcript: "saved transcript".into(),
            grill_question_group: Some(question_group),
            grill_answers: vec![GrillAnswer {
                question_number: 1,
                answer: "Keep it".into(),
            }],
            grill_decisions: vec![GrillAnswer {
                question_number: 1,
                answer: "Keep it".into(),
            }],
            grill_response: Some("1. Keep it".into()),
            grill_phase: Some(GrillPhase::Working),
            grill_action: None,
        });

        runtime
            .reconcile_runs()
            .expect("the active Grill should reconcile");
        let recovered = &runtime.state.runs[0];
        assert_eq!(recovered.pane_status, RunPaneStatus::Available);
        assert_eq!(recovered.grill_phase, Some(GrillPhase::Working));
        assert!(recovered.transcript.contains("Keep this decision?"));
        assert_eq!(recovered.grill_answers.len(), 1);
        assert_eq!(recovered.grill_response.as_deref(), Some("1. Keep it"));

        let survivor_session = format!("grill-reconcile-survivor-{}", std::process::id());
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "new-session",
            "-d",
            "-s",
            &survivor_session,
            "-c",
            directory
                .path()
                .to_str()
                .expect("temporary path should be valid"),
        ]);
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "kill-session",
            "-t",
            &session,
        ]);
        runtime
            .reconcile_runs()
            .expect("Pane loss should remain recoverable while another session is alive");
        let missing = &runtime.state.runs[0];
        assert_eq!(missing.pane_status, RunPaneStatus::Missing);
        assert_eq!(missing.grill_phase, Some(GrillPhase::RecoverablePaneLoss));
        assert_eq!(missing.grill_answers.len(), 1);
        assert!(missing.transcript.contains("Keep this decision?"));
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "kill-session",
            "-t",
            &survivor_session,
        ]);
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_github_issue_links_it_without_replacing_item_context() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let script = r#"#!/bin/sh
if [ "$1" = api ] && [ "$2" = repos/acme/app/issues ]; then
  printf '%s' '{"html_url":"https://github.com/acme/app/issues/42"}'
elif [ "$1" = issue ] && [ "$2" = view ]; then
  printf '%s' '{"number":42,"title":"Created from Mission Manager","state":"OPEN","author":null,"labels":[],"milestone":null,"updatedAt":null}'
fi
"#;
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");
        let mut store = SqliteStore::open(&database).expect("database should open");
        store
            .set_gh_executable_path(&executable)
            .expect("fake gh path should persist");
        drop(store);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Keep this Item".into(), 1, 1)
            .expect("Item should be created");
        runtime
            .update_item(
                Event::SetItemNotes {
                    item_id: 1,
                    notes: "Private handoff context".into(),
                },
                1,
            )
            .expect("Item notes should be saved");
        runtime
            .create_item("Related Item".into(), 1, 1)
            .expect("related Item should be created");
        runtime
            .set_item_relation(1, 2, ItemRelationKind::Blocks)
            .expect("Item relationship should be saved");
        let repository = runtime
            .register_repository(
                1,
                "service-a".into(),
                "https://github.com/acme/app.git".into(),
            )
            .expect("the Project Repository should be registered");

        let result = runtime
            .create_github_issue(
                1,
                repository.id,
                "Public title".into(),
                "Public body".into(),
            )
            .expect("GitHub Issue should be created and linked");

        assert_eq!(
            result.link.object.canonical_url,
            "https://github.com/acme/app/issues/42"
        );
        assert_eq!(runtime.state.items[0].human_identifier, "MC-1");
        assert_eq!(runtime.state.items[0].title, "Keep this Item");
        assert_eq!(runtime.state.items[0].notes, "Private handoff context");
        assert_eq!(runtime.state.relationships.len(), 1);
        assert_eq!(runtime.state.links[0].item_id, 1);
        assert_eq!(
            runtime.state.snapshots[0].title,
            "Created from Mission Manager"
        );
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_github_issue_rejects_items_and_repositories_outside_the_same_project_before_gh() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let calls = directory.path().join("gh-calls");
        let quoted_calls_path = calls.to_string_lossy().replace('\'', "'\\''");
        let script = format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{quoted_calls_path}'\nexit 1\n");
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");
        let mut store = SqliteStore::open(&database).expect("database should open");
        store
            .set_gh_executable_path(&executable)
            .expect("fake gh path should persist");
        drop(store);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Item in the original Project".into(), 1, 1)
            .expect("Item should be created");
        let other_project = runtime
            .create_project("Another Project".into(), 1, ProjectDefaults::default())
            .expect("another Project should be created");
        let foreign_context = runtime
            .create_context("Another Context".into())
            .expect("another Context should be created");
        let foreign_project_id = runtime
            .state
            .projects
            .iter()
            .find(|project| project.context_id == foreign_context.id)
            .expect("the Context should own its default Project")
            .id;
        let project_repository = runtime
            .register_repository(
                other_project.id,
                "other-project".into(),
                "https://github.com/acme/other-project.git".into(),
            )
            .expect("Repository should register in another Project");
        let context_repository = runtime
            .register_repository(
                foreign_project_id,
                "other-context".into(),
                "https://github.com/acme/other-context.git".into(),
            )
            .expect("Repository should register in another Context");
        let current_repository = runtime
            .register_repository(
                1,
                "current".into(),
                "https://github.com/acme/current.git".into(),
            )
            .expect("Repository should register in the Item's Project");

        let attempts = [
            runtime.create_github_issue(404, current_repository.id, "Title".into(), String::new()),
            runtime.create_github_issue(1, 404, "Title".into(), String::new()),
            runtime.create_github_issue(1, project_repository.id, "Title".into(), String::new()),
            runtime.create_github_issue(1, context_repository.id, "Title".into(), String::new()),
        ];

        let errors = attempts
            .into_iter()
            .map(|attempt| attempt.expect_err("invalid scope should be rejected"))
            .collect::<Vec<_>>();
        assert!(errors.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(
            errors[0],
            "The Item or Repository is not available in the Item's Project"
        );
        assert!(!calls.exists(), "invalid scope must not call GitHub CLI");
    }

    #[cfg(unix)]
    #[test]
    fn failed_link_recording_after_issue_creation_returns_the_created_url() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let script = r#"#!/bin/sh
if [ "$1" = api ] && [ "$2" = repos/acme/app/issues ]; then
  printf '%s' '{"html_url":"https://github.com/acme/app/issues/42"}'
elif [ "$1" = issue ] && [ "$2" = view ]; then
  printf '%s' '{"number":42,"title":"Created from Mission Manager","state":"OPEN","author":null,"labels":[],"milestone":null,"updatedAt":null}'
fi
"#;
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");
        let mut store = SqliteStore::open(&database).expect("database should open");
        store
            .set_gh_executable_path(&executable)
            .expect("fake gh path should persist");
        drop(store);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Keep this Item".into(), 1, 1)
            .expect("Item should be created");
        let repository = runtime
            .register_repository(
                1,
                "service-a".into(),
                "https://github.com/acme/app.git".into(),
            )
            .expect("the Project Repository should be registered");
        runtime.state.next_link_id = i64::MAX;

        let error = runtime
            .create_github_issue(1, repository.id, "Title".into(), String::new())
            .expect_err("an exhausted Link sequence should reject Link recording");

        assert!(error.contains("https://github.com/acme/app/issues/42"));
    }

    #[cfg(unix)]
    #[test]
    fn downstream_grill_output_is_confirmed_before_it_becomes_an_item_link() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let script = r#"#!/bin/sh
if [ "$1" = issue ] && [ "$2" = view ] && [ "$3" = "https://github.com/acme/app/issues/7" ]; then
  printf '%s' '{"number":7,"title":"Captured downstream Issue","state":"OPEN","author":null,"labels":[],"milestone":null,"updatedAt":null}'
  exit 0
fi
exit 1
"#;
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");
        let mut store = SqliteStore::open(&database).expect("database should open");
        store
            .set_gh_executable_path(&executable)
            .expect("fake gh path should persist");
        drop(store);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Capture downstream work".into(), 1, 1)
            .expect("Item should be created");
        runtime.state.runs.push(Run {
            id: 1,
            item_id: 1,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Grill,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: "Continue to-tickets".into(),
            working_directory: "/tmp".into(),
            session_name: "downstream".into(),
            pane_id: "%1".into(),
            started_at: 1,
            state: RunState::Finished,
            last_applied_agent_state_sequence: None,
            pane_status: RunPaneStatus::Available,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: Some(GrillPhase::Working),
            grill_action: Some(GrillContinuationAction::ToTickets),
        });

        let output = r#"AI_MISSION_MANAGER_EVENT {"event":"github.issue.created","url":"https://github.com/acme/app/issues/7","run_id":1,"action":"to-tickets"}
https://github.com/acme/app/issues/8
https://example.com/unrelated"#;
        runtime
            .capture_downstream_issues(1, output)
            .expect("capture should tolerate unconfirmable and unrelated URLs");
        runtime
            .capture_downstream_issues(1, output)
            .expect("replaying output should be idempotent");

        assert_eq!(runtime.state.external_objects.len(), 1);
        assert_eq!(runtime.state.links.len(), 1);
        assert_eq!(
            runtime.state.snapshots[0].title,
            "Captured downstream Issue"
        );
        assert_eq!(
            runtime.state.links[0].provenance,
            Some(crate::domain::LinkProvenance {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                discovery: crate::domain::DownstreamIssueDiscovery::StructuredEvent,
            })
        );
    }

    #[test]
    fn item_deletion_requires_a_fresh_preview_and_records_a_summary() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Delete after review".into(), 1, 1)
            .expect("Item should be created");

        let preview = runtime
            .prepare_item_deletion(1)
            .expect("Item deletion preview should be available");
        assert_eq!(preview.plan.human_identifier, "MC-1");
        assert!(preview.blockers.is_empty());
        assert_eq!(preview.plan.reminder_count, 0);
        runtime
            .update_item(
                Event::SetItemNotes {
                    item_id: 1,
                    notes: "changed after preview".into(),
                },
                1,
            )
            .expect("Item changes should persist");
        assert!(runtime.delete_item(1, true).is_err());
        assert_eq!(runtime.state.items.len(), 1);

        runtime
            .prepare_item_deletion(1)
            .expect("a fresh deletion preview should be available");
        let result = runtime
            .delete_item(1, true)
            .expect("the confirmed Item deletion should succeed");
        assert_eq!(result.summary.item_id, 1);
        assert_eq!(result.summary.item_id, 1);
        assert!(runtime.state.items.is_empty());
        assert!(runtime
            .list_audit_history()
            .expect("audit history should load")
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::ItemDeleted { .. })));
    }

    #[test]
    fn parent_deletion_requires_a_fresh_preview_and_keeps_an_empty_context_usable() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_context("Work".into())
            .expect("Context should be created");
        runtime
            .create_project(
                "Billing".into(),
                2,
                ProjectDefaults {
                    item_status: ItemStatus::Inbox,
                    execution_mode: crate::domain::ExecutionMode::Worktree,
                },
            )
            .expect("Project should be created");
        runtime
            .create_item("Delete this Project graph".into(), 2, 3)
            .expect("Item should be created");

        let preview = runtime
            .prepare_project_deletion(3)
            .expect("Project deletion preview should be available");
        assert_eq!(preview.plan.project_id, Some(3));
        assert_eq!(preview.plan.items.len(), 1);
        assert!(preview.blockers.is_empty());
        runtime
            .update_item(
                Event::SetItemNotes {
                    item_id: 1,
                    notes: "changed after preview".into(),
                },
                1,
            )
            .expect("Item should update");
        assert!(runtime
            .delete_project(3, vec![1], Vec::new(), Vec::new(), true)
            .is_err());
        assert!(runtime.state.projects.iter().any(|project| project.id == 3));

        runtime
            .prepare_project_deletion(3)
            .expect("fresh Project preview should be available");
        let result = runtime
            .delete_project(3, vec![1], Vec::new(), Vec::new(), true)
            .expect("Project deletion should succeed");
        assert_eq!(result.summary.item_count, 1);
        assert!(runtime.state.items.is_empty());
        assert!(runtime.state.projects.iter().all(|project| project.id != 3));
        assert!(runtime.state.contexts.iter().any(|context| context.id == 2));

        runtime
            .prepare_project_deletion(2)
            .expect("the remaining Project preview should be available");
        runtime
            .delete_project(2, Vec::new(), Vec::new(), Vec::new(), true)
            .expect("the last Project in a Context should be deletable");
        assert!(runtime
            .state
            .projects
            .iter()
            .all(|project| project.context_id != 2));

        let empty_context = runtime
            .prepare_context_deletion(2)
            .expect("an empty Context should have a deletion preview");
        assert!(empty_context.blockers.is_empty());
        runtime
            .delete_context(
                2,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                true,
            )
            .expect("the non-last empty Context should be deletable");
        let last_context = runtime
            .prepare_context_deletion(1)
            .expect("the last Context should still produce a preview");
        assert!(last_context
            .blockers
            .iter()
            .any(|blocker| blocker.contains("last Context")));
        assert!(runtime
            .delete_context(
                1,
                vec![1],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                true,
            )
            .is_err());

        let history = runtime
            .list_audit_history()
            .expect("audit history should load");
        assert!(history
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::ProjectDeleted { .. })));
        assert!(history
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::ContextDeleted { .. })));
    }

    #[test]
    fn repository_registration_adopts_a_checkout_or_clones_an_empty_destination() {
        let directory = tempdir().expect("temporary repository directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "safe\n").expect("README should be written");
        run_git(&seed, &["add", "README.md"]);
        run_git(&seed, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service.git");
        run_git(directory.path(), &["init", "--bare", "service.git"]);
        let origin_url = origin.to_string_lossy().into_owned();
        run_git(&seed, &["remote", "add", "origin", &origin_url]);
        run_git(&seed, &["push", "origin", "main"]);

        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .register_machine(
                1,
                "Local Mac".into(),
                "mission".into(),
                MachineTransport::Local,
            )
            .expect("Machine should register");
        let adopted = runtime
            .register_repository_at_location(
                1,
                "service".into(),
                None,
                "main".into(),
                1,
                seed.to_string_lossy().into_owned(),
                None,
                false,
            )
            .expect("existing checkout should be adopted");
        assert_eq!(adopted.remote_url, origin_url);
        assert_eq!(adopted.base_branch, "main");
        assert!(runtime.state.repository_locations[0]
            .checkout_path
            .ends_with("/seed"));

        let clone_destination = directory.path().join("clone");
        let cloned = runtime
            .register_repository_at_location(
                1,
                "cloned-service".into(),
                Some(origin_url.clone()),
                "main".into(),
                1,
                clone_destination.to_string_lossy().into_owned(),
                Some("worktrees".into()),
                true,
            )
            .expect("clone destination should be prepared");
        assert_eq!(cloned.remote_url, origin_url);
        assert!(clone_destination.join(".git").is_dir());
    }

    #[test]
    fn worktree_run_event_targets_the_registered_worktree_location() {
        let event = crate::features::work::worktree_run_event(
            7,
            11,
            13,
            17,
            AgentKind::Codex,
            ExecutionProfile::Implement,
            "Implement the change".into(),
            "~/worktrees/item-13/repository".into(),
            "mission-item-7-run-1".into(),
            "%42".into(),
            123,
            RunPromptSelection {
                include_objective: true,
                include_notes: false,
                external_object_ids: vec![19],
            },
        );

        assert!(matches!(
            event,
            Event::StartWorktreeRun {
                item_id: 7,
                workspace_id: 11,
                worktree_id: 13,
                machine_id: 17,
                working_directory,
                session_name,
                pane_id,
                started_at: 123,
                ..
            } if working_directory == "~/worktrees/item-13/repository"
                && session_name == "mission-item-7-run-1"
                && pane_id == "%42"
        ));
    }

    fn run_git(directory: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .expect("Git should start");
        assert!(
            output.status.success(),
            "Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_tmux(args: &[&str]) -> String {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .expect("tmux should start");
        assert!(
            output.status.success(),
            "tmux failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}
