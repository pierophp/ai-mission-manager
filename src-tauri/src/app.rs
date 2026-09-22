use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    agent_state::{provision_hooks, read_state_file, state_file_path, AgentStateRecord},
    dependencies::{check_command, resolve_executable, DependencyState, DependencyStatus},
    domain::{
        activity_tab_view, compose_run_prompt as build_run_prompt, decide, external_link_view,
        home_view, normalize_machine_path, plan_context_deletion, plan_external_object_deletion,
        plan_item_deletion, plan_machine_deletion, plan_project_deletion, plan_repository_deletion,
        plan_reset_local_data, search_items, suggest_untracked_runs, worktree_path,
        ActivityTabView, AgentKind, AgentPaneObservation, AuditAction, AuditEntry, Context,
        ContextAttentionDefault, DomainState, Effect, Event, ExecutionMode, ExecutionProfile,
        ExternalChangePolicy, ExternalLinkView, ExternalObjectDeletionPlan,
        ExternalObjectDeletionSummary, ExternalObjectInput, ExternalObjectKind, ExternalProvider,
        ExternalSnapshot, HomeView, Item, ItemDeletionPlan, ItemDeletionSummary, ItemRelation,
        ItemRelationKind, ItemStatus, ItemView, Machine, MachineDeletionPlan, MachineObservation,
        MachineTransport, ParentDeletionPlan, Project, ProjectDefaults, Repository,
        RepositoryDeletionPlan, ResetLocalDataPlan, ResetLocalDataSummary, Run, RunCheckout,
        RunPaneStatus, RunPromptSelection, RunState, RunSuggestion, Workspace,
        WorkspaceRepositoryInput, Worktree,
    },
    git::GitCli,
    persistence::SqliteStore,
    provider::{classify_url, resolve_gh_executable, GithubCli},
    terminal::{
        capture_pane, find_agent_executable, list_agent_panes, list_panes, open_pane_in_terminal,
        probe_local_runtime, probe_machine, terminal_transport, AgentLaunchContext,
        ExternalPaneIdentity, PaneSummary, TerminalRuntime, TmuxControlPane, TmuxRuntime,
    },
};

pub const RESET_CONFIRMATION_PHRASE: &str = "RESET ALL LOCAL DATA";

fn machine_home_directory(machine: &Machine) -> String {
    match &machine.transport {
        MachineTransport::Local => env::var("HOME").unwrap_or_else(|_| "/".into()),
        MachineTransport::Ssh { .. } => "~".into(),
    }
}

fn resolve_machine_path(path: &str, machine_home: &str) -> PathBuf {
    if path == "~" {
        return PathBuf::from(machine_home);
    }
    if let Some(relative) = path.strip_prefix("~/") {
        return Path::new(machine_home).join(relative);
    }
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else {
        Path::new(machine_home).join(path)
    }
}

pub struct Runtime {
    store: SqliteStore,
    state: DomainState,
    gh_executable_path: Option<PathBuf>,
    pending_worktree_removals: HashMap<i64, WorktreeRemovalReport>,
    pending_workspace_removal: Option<WorkspaceRemovalReport>,
    pending_item_deletion: Option<ItemDeletionPreview>,
    pending_external_object_deletion: Option<ExternalObjectDeletionPreview>,
    pending_repository_deletion: Option<RepositoryDeletionPreview>,
    pending_machine_deletion: Option<MachineDeletionPreview>,
    pending_parent_deletion: Option<ParentDeletionPreview>,
    pending_reset_local_data: Option<ResetLocalDataPreview>,
    terminal_connections: HashMap<String, TmuxControlPane>,
    agent_state_directory: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderChoice {
    GitHub,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub completed: bool,
    pub provider: ProviderChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub runtime: DependencyStatus,
    pub provider: DependencyStatus,
    pub agents: Vec<DependencyStatus>,
    pub checked_at: i64,
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
        let database_path = path.as_ref().to_path_buf();
        let agent_state_directory = database_path
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
            pending_workspace_removal: None,
            pending_item_deletion: None,
            pending_external_object_deletion: None,
            pending_repository_deletion: None,
            pending_machine_deletion: None,
            pending_parent_deletion: None,
            pending_reset_local_data: None,
            terminal_connections: HashMap::new(),
            agent_state_directory,
        };
        runtime.recover_run_states()?;
        Ok(runtime)
    }

    fn create_context(&mut self, name: String) -> Result<Context, String> {
        let decision = decide(self.state.clone(), Event::CreateContext { name })
            .map_err(|error| error.to_string())?;
        let context = decision
            .state
            .contexts
            .last()
            .cloned()
            .ok_or_else(|| "Context creation produced no Context".to_owned())?;
        self.commit(decision)?;
        Ok(context)
    }

    fn setup_state(&self) -> Result<SetupState, String> {
        let completed = self
            .store
            .setting("setup_completed")
            .map_err(|error| error.to_string())?
            .as_deref()
            == Some("true");
        let provider = match self
            .store
            .setting("provider_choice")
            .map_err(|error| error.to_string())?
            .as_deref()
        {
            Some("github") | None => ProviderChoice::GitHub,
            Some("none") => ProviderChoice::None,
            Some(other) => return Err(format!("Unknown provider choice: {other}")),
        };
        Ok(SetupState {
            completed,
            provider,
        })
    }

    fn complete_setup(
        &mut self,
        context_name: String,
        provider: ProviderChoice,
    ) -> Result<SetupState, String> {
        let context_name = context_name.trim();
        if context_name.is_empty() {
            return Err("A Context name is required to finish setup".into());
        }
        if !self
            .state
            .contexts
            .iter()
            .any(|context| context.name == context_name)
        {
            self.create_context(context_name.to_owned())?;
        }
        self.store
            .set_setting("setup_completed", "true")
            .and_then(|_| {
                self.store.set_setting(
                    "provider_choice",
                    match provider {
                        ProviderChoice::GitHub => "github",
                        ProviderChoice::None => "none",
                    },
                )
            })
            .map_err(|error| error.to_string())?;
        Ok(SetupState {
            completed: true,
            provider,
        })
    }

    fn health_status(
        &mut self,
        provider_override: Option<ProviderChoice>,
    ) -> Result<HealthStatus, String> {
        let runtime = self.check_runtime_dependency()?;
        let setup = self.setup_state()?;
        let provider = match provider_override.unwrap_or(setup.provider) {
            ProviderChoice::GitHub => self.check_github_dependency()?,
            ProviderChoice::None => DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::NotConfigured,
                executable_path: None,
                message: "No provider selected; local Items remain available.".into(),
                action: Some(
                    "Choose GitHub in setup when you are ready to link external work.".into(),
                ),
            },
        };
        let agents = [
            ("claude", "Claude Code", "claude_executable_path"),
            ("codex", "Codex", "codex_executable_path"),
        ]
        .into_iter()
        .map(|(key, label, setting_key)| {
            self.check_local_dependency(
                key,
                label,
                setting_key,
                &[],
                &format!("Install {label} before starting a Run with it."),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
        Ok(HealthStatus {
            runtime,
            provider,
            agents,
            checked_at: current_unix_seconds(),
        })
    }

    fn check_runtime_dependency(&mut self) -> Result<DependencyStatus, String> {
        let path = self.resolve_and_store_executable("tmux", "tmux_executable_path")?;
        let Some(path) = path else {
            return Ok(DependencyStatus {
                key: "tmux".into(),
                label: "tmux runtime".into(),
                state: DependencyState::Missing,
                executable_path: None,
                message: "tmux was not found.".into(),
                action: Some(
                    "Install tmux (for example, with `brew install tmux`) and check again.".into(),
                ),
            });
        };
        match probe_local_runtime(&path) {
            Ok(()) => Ok(DependencyStatus {
                key: "tmux".into(),
                label: "tmux runtime".into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: "tmux is ready.".into(),
                action: None,
            }),
            Err(detail) => Ok(DependencyStatus {
                key: "tmux".into(),
                label: "tmux runtime".into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("tmux could not be checked: {detail}"),
                action: Some("Repair or reinstall tmux, then check again.".into()),
            }),
        }
    }

    fn check_github_dependency(&mut self) -> Result<DependencyStatus, String> {
        let path = self.resolve_and_store_executable("gh", "gh_executable_path")?;
        let Some(path) = path else {
            return Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Missing,
                executable_path: None,
                message: "GitHub CLI (`gh`) was not found.".into(),
                action: Some(
                    "Install GitHub CLI, then authenticate it with `gh auth login`.".into(),
                ),
            });
        };
        if let Err(detail) = check_command(&path, &["--version"]) {
            return Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("GitHub CLI could not run: {detail}"),
                action: Some("Repair or reinstall GitHub CLI, then check again.".into()),
            });
        }
        match check_command(&path, &["auth", "status", "--hostname", "github.com"]) {
            Ok(()) => Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: "GitHub CLI is installed and authenticated.".into(),
                action: None,
            }),
            Err(detail) if looks_like_authentication_failure(&detail) => Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Unauthenticated,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("GitHub CLI is not authenticated: {detail}"),
                action: Some("Run `gh auth login` in your terminal; Mission Manager will not log in for you.".into()),
            }),
            Err(detail) => Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("GitHub authentication status could not be checked: {detail}"),
                action: Some("Check network access to github.com, then check again.".into()),
            }),
        }
    }

    fn check_local_dependency(
        &mut self,
        key: &str,
        label: &str,
        setting_key: &str,
        args: &[&str],
        missing_action: &str,
    ) -> Result<DependencyStatus, String> {
        let path = self.resolve_and_store_executable(key, setting_key)?;
        let Some(path) = path else {
            return Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Missing,
                executable_path: None,
                message: format!("{label} was not found."),
                action: Some(missing_action.into()),
            });
        };
        if args.is_empty() {
            return Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("{label} is installed."),
                action: None,
            });
        }
        match check_command(&path, args) {
            Ok(()) => Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("{label} is ready."),
                action: None,
            }),
            Err(detail) => Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("{label} could not be checked: {detail}"),
                action: Some(missing_action.into()),
            }),
        }
    }

    fn resolve_and_store_executable(
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

    fn agent_executable(&mut self, machine: &Machine, agent: AgentKind) -> Result<PathBuf, String> {
        let name = agent_executable_name(agent);
        let setting_key = if matches!(&machine.transport, MachineTransport::Local) {
            format!("{name}_executable_path")
        } else {
            format!("machine_{}_{}_executable_path", machine.id, name)
        };
        if matches!(&machine.transport, MachineTransport::Local) {
            return self
                .resolve_and_store_executable(name, &setting_key)?
                .ok_or_else(|| format!("{name} is not installed on Machine {}", machine.name));
        }

        let executable = find_agent_executable(machine, name)?;
        if !executable.is_absolute() {
            return Err(format!(
                "Machine {} returned a non-absolute {name} executable path",
                machine.name
            ));
        }
        self.store
            .set_executable_path(&setting_key, &executable)
            .map_err(|error| error.to_string())?;
        Ok(executable)
    }

    fn create_project(
        &mut self,
        name: String,
        context_id: i64,
        defaults: ProjectDefaults,
    ) -> Result<Project, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateProject {
                context_id,
                name,
                defaults,
            },
        )
        .map_err(|error| error.to_string())?;
        let project = decision
            .state
            .projects
            .last()
            .cloned()
            .ok_or_else(|| "Project creation produced no Project".to_owned())?;
        self.commit(decision)?;
        Ok(project)
    }

    fn register_repository(
        &mut self,
        project_id: i64,
        name: String,
        remote_url: String,
    ) -> Result<Repository, String> {
        let decision = decide(
            self.state.clone(),
            Event::RegisterRepository {
                project_id,
                name,
                remote_url,
            },
        )
        .map_err(|error| error.to_string())?;
        let repository = decision
            .state
            .repositories
            .last()
            .cloned()
            .ok_or_else(|| "Repository registration produced no Repository".to_owned())?;
        self.commit(decision)?;
        Ok(repository)
    }

    fn register_repository_at_location(
        &mut self,
        project_id: i64,
        name: String,
        remote_url: Option<String>,
        base_branch: String,
        machine_id: i64,
        checkout_path: String,
        worktree_root: Option<String>,
        clone_into_destination: bool,
    ) -> Result<Repository, String> {
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| format!("Project {project_id} does not exist"))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {machine_id} does not exist"))?;
        if machine.context_id != project.context_id {
            return Err(format!(
                "Machine {} is not available in Project {}'s Context",
                machine.name, project.name
            ));
        }

        let machine_home = machine_home_directory(&machine);
        let normalized_checkout_path = normalize_machine_path(&checkout_path, &machine_home)
            .map_err(|error| error.to_string())?;
        let resolved_checkout_path = resolve_machine_path(&normalized_checkout_path, &machine_home);
        let worktree_root = worktree_root
            .filter(|root| !root.trim().is_empty())
            .unwrap_or_else(|| "~/worktrees".into());
        let normalized_worktree_root = normalize_machine_path(&worktree_root, &machine_home)
            .map_err(|error| error.to_string())?;
        let git = GitCli::system();
        let effective_remote = if clone_into_destination {
            let remote_url = remote_url
                .as_deref()
                .map(str::trim)
                .filter(|remote| !remote.is_empty())
                .ok_or_else(|| "A remote URL is required when cloning a Repository".to_owned())?;
            git.clone_repository(remote_url, &resolved_checkout_path)
                .map_err(|error| error.to_string())?;
            remote_url.to_owned()
        } else {
            let inspection = git
                .inspect_checkout(&resolved_checkout_path)
                .map_err(|error| error.to_string())?;
            let detected_remote = inspection
                .remote_url
                .ok_or_else(|| "The existing checkout has no Git remote".to_owned())?;
            if let Some(configured_remote) = remote_url
                .as_deref()
                .map(str::trim)
                .filter(|remote| !remote.is_empty())
            {
                if configured_remote != detected_remote {
                    return Err(format!(
                        "The existing checkout remote does not match the configured Repository: {configured_remote} != {detected_remote}"
                    ));
                }
            }
            detected_remote
        };

        let existing_repository_id = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.project_id == project_id && repository.name == name)
            .map(|repository| repository.id);
        let decision = decide(
            self.state.clone(),
            Event::RegisterRepositoryAtLocation {
                project_id,
                name: name.clone(),
                remote_url: effective_remote,
                base_branch,
                machine_id,
                checkout_path: normalized_checkout_path,
                worktree_root: normalized_worktree_root,
            },
        )
        .map_err(|error| error.to_string())?;
        let repository = decision
            .state
            .repositories
            .iter()
            .find(|repository| Some(repository.id) == existing_repository_id)
            .or_else(|| decision.state.repositories.last())
            .cloned()
            .ok_or_else(|| "Repository registration produced no Repository".to_owned())?;
        self.commit(decision)?;
        Ok(repository)
    }

    fn create_workspace(
        &mut self,
        item_id: i64,
        repositories: Vec<WorkspaceRepositoryInput>,
    ) -> Result<Workspace, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorkspace {
                item_id,
                repositories,
            },
        )
        .map_err(|error| error.to_string())?;
        let workspace = decision
            .state
            .workspaces
            .last()
            .cloned()
            .ok_or_else(|| "Workspace creation produced no Workspace".to_owned())?;
        self.commit(decision)?;
        Ok(workspace)
    }

    fn create_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        branch: String,
        base_branch: String,
    ) -> Result<Worktree, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorktree {
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty: false,
            },
        )
        .map_err(|error| error.to_string())?;
        let worktree = decision
            .state
            .worktrees
            .last()
            .cloned()
            .ok_or_else(|| "Worktree creation produced no Worktree".to_owned())?;
        self.commit(decision)?;
        Ok(worktree)
    }

    fn prepare_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        reuse_existing_branch: bool,
        confirm_dirty_attachment: bool,
    ) -> Result<Worktree, String> {
        let (workspace, selected, repository, machine, location) =
            self.worktree_inputs(workspace_id, repository_id, machine_id)?;
        self.ensure_worktree_not_recorded(workspace_id, repository_id)?;

        let machine_home = machine_home_directory(&machine);
        let canonical_checkout = resolve_machine_path(&location.checkout_path, &machine_home);
        let worktree_root = resolve_machine_path(&location.worktree_root, &machine_home);
        let destination = worktree_path(
            &worktree_root,
            workspace.id,
            &selected.branch,
            &repository.name,
        );
        let inspection = match GitCli::system().prepare_worktree_on_machine(
            &machine,
            &repository,
            &canonical_checkout,
            &destination,
            &selected.branch,
            &selected.base_branch,
            reuse_existing_branch,
            confirm_dirty_attachment,
        ) {
            Ok(inspection) => inspection,
            Err(error) => {
                let mark_error = self.mark_workspace_resumable(workspace_id).err();
                return Err(format_commit_error(error.to_string(), mark_error));
            }
        };
        let path = normalize_machine_path(&destination.to_string_lossy(), &machine_home)
            .map_err(|error| error.to_string())?;
        self.persist_prepared_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            selected.branch,
            selected.base_branch,
            inspection.is_dirty,
        )
    }

    fn mark_workspace_resumable(&mut self, workspace_id: i64) -> Result<(), String> {
        let decision = decide(
            self.state.clone(),
            Event::MarkWorkspaceResumable { workspace_id },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)
    }

    fn attach_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        confirm_dirty_attachment: bool,
    ) -> Result<Worktree, String> {
        let (_workspace, selected, repository, machine, location) =
            self.worktree_inputs(workspace_id, repository_id, machine_id)?;
        self.ensure_worktree_not_recorded(workspace_id, repository_id)?;

        let machine_home = machine_home_directory(&machine);
        let canonical_checkout = resolve_machine_path(&location.checkout_path, &machine_home);
        let path =
            normalize_machine_path(&path, &machine_home).map_err(|error| error.to_string())?;
        let worktree_path = resolve_machine_path(&path, &machine_home);
        let inspection = GitCli::system()
            .validate_worktree_attachment_on_machine(
                &machine,
                &repository,
                &canonical_checkout,
                &worktree_path,
                &selected.branch,
                confirm_dirty_attachment,
            )
            .map_err(|error| error.to_string())?;
        self.persist_prepared_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            selected.branch,
            selected.base_branch,
            inspection.is_dirty,
        )
    }

    fn worktree_inputs(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
    ) -> Result<
        (
            Workspace,
            crate::domain::WorkspaceRepository,
            Repository,
            Machine,
            crate::domain::RepositoryLocation,
        ),
        String,
    > {
        let workspace = self
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .cloned()
            .ok_or_else(|| format!("Workspace {workspace_id} does not exist"))?;
        let selected = workspace
            .repositories
            .iter()
            .find(|repository| repository.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                format!("Repository {repository_id} is not selected in Workspace {workspace_id}")
            })?;
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))?;
        let machine = self.machine_for_item(workspace.item_id, Some(machine_id))?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == repository_id && location.machine_id == machine_id
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Repository {} has no checkout registered on Machine {}",
                    repository.name, machine.name
                )
            })?;
        Ok((workspace, selected, repository, machine, location))
    }

    fn ensure_worktree_not_recorded(
        &self,
        workspace_id: i64,
        repository_id: i64,
    ) -> Result<(), String> {
        if self.state.worktrees.iter().any(|worktree| {
            worktree.workspace_id == workspace_id && worktree.repository_id == repository_id
        }) {
            return Err(format!(
                "Workspace {workspace_id} already has a recorded Worktree for Repository {repository_id}"
            ));
        }
        Ok(())
    }

    fn persist_prepared_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        branch: String,
        base_branch: String,
        is_dirty: bool,
    ) -> Result<Worktree, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorktree {
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty,
            },
        )
        .map_err(|error| error.to_string())?;
        let worktree = decision
            .state
            .worktrees
            .last()
            .cloned()
            .ok_or_else(|| "Worktree creation produced no Worktree".to_owned())?;
        self.commit(decision)?;
        Ok(worktree)
    }

    fn build_worktree_removal_report(
        &self,
        worktree_id: i64,
    ) -> Result<WorktreeRemovalReport, String> {
        let worktree = self
            .state
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .cloned()
            .ok_or_else(|| format!("Worktree {worktree_id} does not exist"))?;
        let repository = self.repository(worktree.repository_id)?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == worktree.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", worktree.machine_id))?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == worktree.repository_id
                    && location.machine_id == worktree.machine_id
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Repository {} has no checkout registered on Machine {}",
                    repository.name, machine.name
                )
            })?;
        let machine_home = machine_home_directory(&machine);
        let canonical_checkout = resolve_machine_path(&location.checkout_path, &machine_home);
        let path = resolve_machine_path(&worktree.path, &machine_home);
        let inspection = GitCli::system()
            .validate_worktree_attachment_on_machine(
                &machine,
                &repository,
                &canonical_checkout,
                &path,
                &worktree.branch,
                true,
            )
            .map_err(|error| error.to_string())?;
        Ok(WorktreeRemovalReport {
            worktree_id,
            workspace_id: worktree.workspace_id,
            repository_id: worktree.repository_id,
            repository_name: repository.name,
            machine_id: worktree.machine_id,
            path: worktree.path,
            branch: worktree.branch,
            is_dirty: inspection.is_dirty,
            requires_destructive_confirmation: inspection.is_dirty,
        })
    }

    fn prepare_worktree_removal(
        &mut self,
        worktree_id: i64,
    ) -> Result<WorktreeRemovalReport, String> {
        let report = self.build_worktree_removal_report(worktree_id)?;
        self.pending_worktree_removals
            .insert(worktree_id, report.clone());
        Ok(report)
    }

    fn remove_worktree(
        &mut self,
        worktree_id: i64,
        confirmed: bool,
        destructive_confirmed: bool,
    ) -> Result<WorktreeRemovalResult, String> {
        if !confirmed {
            return Err(
                "Worktree removal requires explicit confirmation after reviewing its safety report"
                    .into(),
            );
        }
        let pending = self
            .pending_worktree_removals
            .get(&worktree_id)
            .cloned()
            .ok_or_else(|| {
                "Review the Worktree removal safety report before removing it".to_owned()
            })?;
        let current = self.build_worktree_removal_report(worktree_id)?;
        if current != pending {
            return Err("The Worktree changed after the safety report; review the updated report before removing it".into());
        }
        if current.requires_destructive_confirmation && !destructive_confirmed {
            return Err("Removing a dirty Worktree requires destructive confirmation".into());
        }
        self.remove_worktree_physical(&current)?;
        let decision = decide(self.state.clone(), Event::RemoveWorktree { worktree_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_worktree_removals.remove(&worktree_id);
        Ok(WorktreeRemovalResult {
            worktree_id,
            branch_preserved: true,
        })
    }

    fn remove_worktree_physical(&self, report: &WorktreeRemovalReport) -> Result<(), String> {
        let worktree = self
            .state
            .worktrees
            .iter()
            .find(|worktree| worktree.id == report.worktree_id)
            .ok_or_else(|| format!("Worktree {} does not exist", report.worktree_id))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == worktree.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", worktree.machine_id))?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == worktree.repository_id
                    && location.machine_id == worktree.machine_id
            })
            .cloned()
            .ok_or_else(|| "The Repository checkout location no longer exists".to_owned())?;
        let machine_home = machine_home_directory(&machine);
        GitCli::system()
            .remove_worktree_on_machine(
                &machine,
                &resolve_machine_path(&location.checkout_path, &machine_home),
                &resolve_machine_path(&worktree.path, &machine_home),
                report.requires_destructive_confirmation,
            )
            .map_err(|error| error.to_string())
    }

    fn build_workspace_removal_report(
        &self,
        workspace_id: i64,
    ) -> Result<WorkspaceRemovalReport, String> {
        if !self
            .state
            .workspaces
            .iter()
            .any(|workspace| workspace.id == workspace_id)
        {
            return Err(format!("Workspace {workspace_id} does not exist"));
        }
        let worktrees = self
            .state
            .worktrees
            .iter()
            .filter(|worktree| worktree.workspace_id == workspace_id)
            .map(|worktree| self.build_worktree_removal_report(worktree.id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut blockers = Vec::new();
        if self
            .state
            .runs
            .iter()
            .any(|run| run.workspace_id == Some(workspace_id))
        {
            blockers.push("This Workspace has Run history and cannot be removed.".into());
        }
        Ok(WorkspaceRemovalReport {
            workspace_id,
            worktrees,
            safe: blockers.is_empty(),
            blockers,
        })
    }

    fn prepare_workspace_removal(
        &mut self,
        workspace_id: i64,
    ) -> Result<WorkspaceRemovalReport, String> {
        let report = self.build_workspace_removal_report(workspace_id)?;
        self.pending_workspace_removal = Some(report.clone());
        Ok(report)
    }

    fn remove_workspace(
        &mut self,
        workspace_id: i64,
        confirmed_worktree_ids: Vec<i64>,
        destructive_worktree_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<WorkspaceRemovalResult, String> {
        if !confirmed {
            return Err(
                "Workspace removal requires explicit confirmation for every Worktree".into(),
            );
        }
        let pending = self
            .pending_workspace_removal
            .as_ref()
            .filter(|report| report.workspace_id == workspace_id)
            .cloned()
            .ok_or_else(|| "Review the Workspace removal report before removing it".to_owned())?;
        let current = self.build_workspace_removal_report(workspace_id)?;
        if current != pending {
            return Err("The Workspace changed after the safety report; review the updated report before removing it".into());
        }
        if !current.safe {
            return Err(format!(
                "Workspace removal is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }
        let mut expected_ids = current
            .worktrees
            .iter()
            .map(|worktree| worktree.worktree_id)
            .collect::<Vec<_>>();
        let mut confirmed_ids = confirmed_worktree_ids;
        expected_ids.sort_unstable();
        confirmed_ids.sort_unstable();
        if expected_ids != confirmed_ids {
            return Err("Confirm each listed Worktree before removing the Workspace".into());
        }
        if current.worktrees.iter().any(|worktree| {
            worktree.requires_destructive_confirmation
                && !destructive_worktree_ids.contains(&worktree.worktree_id)
        }) {
            return Err("Each dirty Worktree requires destructive confirmation".into());
        }
        for worktree in &current.worktrees {
            self.remove_worktree_physical(worktree)?;
        }
        let worktree_count = current.worktrees.len();
        let decision = decide(self.state.clone(), Event::RemoveWorkspace { workspace_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_workspace_removal = None;
        Ok(WorkspaceRemovalResult {
            workspace_id,
            worktree_count,
            branches_preserved: true,
        })
    }

    fn local_machine_for_item(&mut self, item_id: i64) -> Result<Machine, String> {
        let context_id = self.item_context_id(item_id)?;
        if let Some(machine) = self
            .state
            .machines
            .iter()
            .find(|machine| machine.context_id == context_id && machine.name == "Local Mac")
            .cloned()
        {
            return Ok(machine);
        }

        let decision = decide(
            self.state.clone(),
            Event::RegisterMachine {
                context_id,
                name: "Local Mac".into(),
                socket_name: "ai-mission-manager".into(),
                transport: MachineTransport::Local,
            },
        )
        .map_err(|error| error.to_string())?;
        let machine = decision
            .state
            .machines
            .last()
            .cloned()
            .ok_or_else(|| "Machine registration produced no Machine".to_owned())?;
        self.commit(decision)?;
        Ok(machine)
    }

    fn machine_for_item(
        &mut self,
        item_id: i64,
        machine_id: Option<i64>,
    ) -> Result<Machine, String> {
        let Some(machine_id) = machine_id else {
            return self.local_machine_for_item(item_id);
        };
        let context_id = self.item_context_id(item_id)?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {machine_id} does not exist"))?;
        if machine.context_id != context_id {
            return Err(format!(
                "Machine {} is not available in this Item's Context",
                machine.name
            ));
        }
        Ok(machine)
    }

    fn register_machine(
        &mut self,
        context_id: i64,
        name: String,
        socket_name: String,
        transport: MachineTransport,
    ) -> Result<Machine, String> {
        let decision = decide(
            self.state.clone(),
            Event::RegisterMachine {
                context_id,
                name,
                socket_name,
                transport,
            },
        )
        .map_err(|error| error.to_string())?;
        let machine = decision
            .state
            .machines
            .last()
            .cloned()
            .ok_or_else(|| "Machine registration produced no Machine".to_owned())?;
        self.commit(decision)?;
        Ok(machine)
    }

    fn observe_machine(
        &mut self,
        machine_id: i64,
        observation: MachineObservation,
    ) -> Result<Machine, String> {
        let decision = decide(
            self.state.clone(),
            Event::ObserveMachine {
                machine_id,
                observation,
                observed_at: current_unix_seconds(),
            },
        )
        .map_err(|error| error.to_string())?;
        let machine = decision
            .state
            .machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {machine_id} does not exist"))?;
        self.commit(decision)?;
        Ok(machine)
    }

    fn check_machine(&mut self, machine_id: i64) -> Result<Machine, String> {
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {machine_id} does not exist"))?;
        probe_machine(&machine)
            .map_err(|error| format!("Could not check Machine {}: {error}", machine.name))?;
        self.observe_machine(machine_id, MachineObservation::Available)
    }

    fn compose_run_prompt(
        &self,
        item_id: i64,
        execution_profile: ExecutionProfile,
        selection: RunPromptSelection,
        custom_prompt: Option<String>,
    ) -> Result<String, String> {
        build_run_prompt(
            &self.state,
            item_id,
            execution_profile,
            &selection,
            custom_prompt.as_deref(),
        )
        .map_err(|error| error.to_string())
    }

    fn inspect_direct_checkouts(
        &mut self,
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
    ) -> Result<(Machine, Vec<DirectRunCheckoutPreview>), String> {
        let workspace = self
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id && workspace.item_id == item_id)
            .cloned()
            .ok_or_else(|| format!("Workspace {workspace_id} does not belong to Item {item_id}"))?;
        let machine = self.machine_for_item(item_id, machine_id)?;
        let machine_home = machine_home_directory(&machine);
        let git = GitCli::system();
        let mut previews = Vec::with_capacity(workspace.repositories.len());

        for selected in workspace.repositories {
            let repository = self
                .state
                .repositories
                .iter()
                .find(|repository| repository.id == selected.repository_id)
                .ok_or_else(|| format!("Repository {} does not exist", selected.repository_id))?;
            let location = self
                .state
                .repository_locations
                .iter()
                .find(|location| {
                    location.repository_id == selected.repository_id
                        && location.machine_id == machine.id
                })
                .ok_or_else(|| {
                    format!(
                        "Repository {} has no checkout registered on Machine {}",
                        repository.name, machine.name
                    )
                })?;
            let path = match machine.transport {
                MachineTransport::Local => {
                    resolve_machine_path(&location.checkout_path, &machine_home)
                }
                MachineTransport::Ssh { .. } => PathBuf::from(&location.checkout_path),
            };
            let inspection = git
                .inspect_checkout_on_machine(&machine, &path)
                .map_err(|error| {
                    format!(
                        "Could not inspect Repository {} on Machine {}: {error}",
                        repository.name, machine.name
                    )
                })?;
            if let Some(remote_url) = inspection.remote_url.as_deref() {
                if remote_url != repository.remote_url {
                    return Err(format!(
                        "Repository {} checkout remote does not match its registered Repository",
                        repository.name
                    ));
                }
            }
            previews.push(DirectRunCheckoutPreview {
                repository_id: repository.id,
                repository_name: repository.name.clone(),
                path: path.to_string_lossy().into_owned(),
                branch: inspection.current_branch,
                is_dirty: inspection.is_dirty,
            });
        }

        Ok((machine, previews))
    }

    fn prepare_direct_run(
        &mut self,
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
    ) -> Result<DirectRunPreview, String> {
        let (machine, checkout_details) =
            self.inspect_direct_checkouts(item_id, workspace_id, machine_id)?;
        let checkouts = checkout_details
            .iter()
            .map(|checkout| RunCheckout {
                repository_id: checkout.repository_id,
                path: checkout.path.clone(),
                branch: checkout.branch.clone(),
                is_dirty: checkout.is_dirty,
            })
            .collect::<Vec<_>>();
        let dirty_repository_ids = checkouts
            .iter()
            .filter(|checkout| checkout.is_dirty)
            .map(|checkout| checkout.repository_id)
            .collect::<Vec<_>>();
        let mut shared_runs = Vec::new();
        let mut shared_paths = Vec::new();
        for run in self.state.runs.iter().filter(|run| {
            run.machine_id == machine.id
                && run.state != RunState::Finished
                && run.pane_status != RunPaneStatus::Missing
        }) {
            for checkout in &checkouts {
                if run
                    .direct_checkouts
                    .iter()
                    .any(|active| active.path == checkout.path)
                {
                    shared_runs.push(DirectRunSharedRun {
                        run_id: run.id,
                        item_id: run.item_id,
                        path: checkout.path.clone(),
                    });
                    shared_paths.push(checkout.path.clone());
                }
            }
        }
        shared_runs.sort_by_key(|run| (run.run_id, run.path.clone()));
        shared_paths.sort();
        shared_paths.dedup();
        Ok(DirectRunPreview {
            workspace_id,
            machine_id: machine.id,
            machine_name: machine.name,
            working_directory: checkouts
                .first()
                .map(|checkout| checkout.path.clone())
                .ok_or_else(|| "Workspace has no selected Repositories".to_owned())?,
            current_branches: checkouts
                .iter()
                .map(|checkout| checkout.branch.clone())
                .collect(),
            checkouts,
            checkout_details,
            dirty_repository_ids,
            shared_runs,
            shared_paths,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn start_direct_run(
        &mut self,
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
    ) -> Result<Run, String> {
        let (machine, checkout_details) =
            self.inspect_direct_checkouts(item_id, workspace_id, machine_id)?;
        let current_checkouts = checkout_details
            .iter()
            .map(|checkout| RunCheckout {
                repository_id: checkout.repository_id,
                path: checkout.path.clone(),
                branch: checkout.branch.clone(),
                is_dirty: checkout.is_dirty,
            })
            .collect::<Vec<_>>();
        if current_checkouts != expected_checkouts {
            return Err(
                "A Direct checkout changed after the preview (branch or dirty state); review the Direct Run preview again before starting"
                    .into(),
            );
        }
        let working_directory = current_checkouts
            .iter()
            .find(|checkout| checkout.repository_id == primary_repository_id)
            .map(|checkout| checkout.path.clone())
            .ok_or_else(|| "Workspace has no selected Repositories".to_owned())?;
        let run_id = self.state.next_run_id;
        let session_name = format!("mission-item-{item_id}-run-{run_id}");
        let preflight = decide(
            self.state.clone(),
            Event::StartDirectRun {
                item_id,
                workspace_id,
                machine_id: machine.id,
                agent,
                execution_profile,
                prompt: prompt.clone(),
                working_directory: working_directory.clone(),
                session_name: format!("{session_name}-preflight"),
                pane_id: "%preflight".into(),
                started_at: current_unix_seconds(),
                prompt_selection: prompt_selection.clone(),
                checkouts: current_checkouts.clone(),
                repository_id: primary_repository_id,
                allow_dirty,
                allow_shared_checkouts,
            },
        )
        .map_err(|error| error.to_string())?;

        if let Err(error) = probe_machine(&machine) {
            return Err(format!(
                "Could not reach Machine {}. The Run was not started locally: {error}",
                machine.name
            ));
        }
        self.observe_machine(machine.id, MachineObservation::Available)?;
        if matches!(machine.transport, MachineTransport::Local) {
            self.provision_agent_hooks()?;
        }
        let state_file = if matches!(machine.transport, MachineTransport::Local) {
            state_file_path(&self.agent_state_directory, run_id)
        } else {
            PathBuf::from(format!("/tmp/ai-mission-manager-run-{run_id}.json"))
        };
        let executable = self
            .agent_executable(&machine, agent)
            .map_err(|error| format!("{}: {error}", agent_display_name(agent)))?;
        let (_, before_launch) =
            self.inspect_direct_checkouts(item_id, workspace_id, Some(machine.id))?;
        let before_launch = before_launch
            .iter()
            .map(|checkout| RunCheckout {
                repository_id: checkout.repository_id,
                path: checkout.path.clone(),
                branch: checkout.branch.clone(),
                is_dirty: checkout.is_dirty,
            })
            .collect::<Vec<_>>();
        if before_launch != current_checkouts {
            return Err(
                "A Direct checkout changed while preparing the Run; review the Direct Run preview again"
                    .into(),
            );
        }
        let terminal = TmuxRuntime;
        let pane_id = terminal.launch_agent(
            &machine,
            &session_name,
            Path::new(&working_directory),
            &executable,
            &prompt,
            AgentLaunchContext {
                run_id,
                state_file: &state_file,
            },
        )?;
        let (_, after_launch) =
            match self.inspect_direct_checkouts(item_id, workspace_id, Some(machine.id)) {
                Ok(value) => value,
                Err(error) => {
                    let cleanup = terminal.kill_session(&machine, &session_name);
                    return Err(format_commit_error(error, cleanup.err()));
                }
            };
        let after_launch = after_launch
            .iter()
            .map(|checkout| RunCheckout {
                repository_id: checkout.repository_id,
                path: checkout.path.clone(),
                branch: checkout.branch.clone(),
                is_dirty: checkout.is_dirty,
            })
            .collect::<Vec<_>>();
        if after_launch != current_checkouts {
            let cleanup = terminal.kill_session(&machine, &session_name);
            return Err(format_commit_error(
                "A Direct checkout changed before the Run was recorded; review the Direct Run preview again".into(),
                cleanup.err(),
            ));
        }
        let decision = match decide(
            self.state.clone(),
            Event::StartDirectRun {
                item_id,
                workspace_id,
                machine_id: machine.id,
                agent,
                execution_profile,
                prompt,
                working_directory,
                session_name: session_name.clone(),
                pane_id,
                started_at: current_unix_seconds(),
                prompt_selection,
                checkouts: after_launch,
                repository_id: primary_repository_id,
                allow_dirty,
                allow_shared_checkouts,
            },
        ) {
            Ok(decision) => decision,
            Err(error) => {
                let cleanup = terminal.kill_session(&machine, &session_name);
                return Err(format_commit_error(error.to_string(), cleanup.err()));
            }
        };
        let run = decision
            .state
            .runs
            .last()
            .cloned()
            .ok_or_else(|| "Direct Run creation produced no Run".to_owned())?;
        if let Err(error) = self.commit(decision) {
            let cleanup = terminal.kill_session(&machine, &run.session_name);
            return Err(format_commit_error(error, cleanup.err()));
        }
        let _ = preflight;
        Ok(run)
    }

    fn list_run_suggestions(&mut self) -> Result<Vec<RunSuggestion>, String> {
        self.recover_run_states()?;
        let local_machine_item_ids = self
            .state
            .workspaces
            .iter()
            .map(|workspace| workspace.item_id)
            .collect::<Vec<_>>();
        for item_id in local_machine_item_ids {
            self.local_machine_for_item(item_id)?;
        }
        let observations = self
            .state
            .machines
            .iter()
            .flat_map(|machine| match list_agent_panes(machine) {
                Ok(panes) => panes
                    .into_iter()
                    .map(|pane| AgentPaneObservation {
                        machine_id: machine.id,
                        agent: pane.agent,
                        session_name: pane.session_name,
                        pane_id: pane.pane_id,
                        current_path: pane.current_path,
                    })
                    .collect::<Vec<_>>(),
                Err(error) => {
                    eprintln!(
                        "Could not inspect Machine {} for agent Panes: {error}",
                        machine.name
                    );
                    Vec::new()
                }
            })
            .collect::<Vec<_>>();
        Ok(suggest_untracked_runs(&self.state, &observations))
    }

    fn attach_run(&mut self, suggestion: RunSuggestion) -> Result<Run, String> {
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == suggestion.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", suggestion.machine_id))?;
        let observation = list_agent_panes(&machine)?
            .into_iter()
            .find(|pane| {
                pane.agent == suggestion.agent
                    && pane.session_name == suggestion.session_name
                    && pane.pane_id == suggestion.pane_id
            })
            .ok_or_else(|| "The suggested agent is no longer available".to_owned())?;
        let canonical = suggest_untracked_runs(
            &self.state,
            &[AgentPaneObservation {
                machine_id: suggestion.machine_id,
                agent: observation.agent,
                session_name: observation.session_name.clone(),
                pane_id: observation.pane_id.clone(),
                current_path: observation.current_path,
            }],
        )
        .into_iter()
        .find(|candidate| {
            candidate.item_id == suggestion.item_id
                && candidate.machine_id == suggestion.machine_id
                && candidate.session_name == suggestion.session_name
                && candidate.pane_id == suggestion.pane_id
                && candidate.workspace_id == suggestion.workspace_id
                && candidate.repository_id == suggestion.repository_id
                && candidate.worktree_id == suggestion.worktree_id
                && candidate.location_path == suggestion.location_path
        })
        .ok_or_else(|| {
            "The suggested agent no longer matches its registered working location".to_owned()
        })?;
        let decision = if let Some(workspace_id) = canonical.workspace_id {
            let repository_id = canonical
                .repository_id
                .ok_or_else(|| "The suggested Workspace location has no Repository".to_owned())?;
            decide(
                self.state.clone(),
                Event::AttachRun {
                    item_id: canonical.item_id,
                    workspace_id,
                    worktree_id: canonical.worktree_id,
                    repository_id,
                    machine_id: canonical.machine_id,
                    agent: canonical.agent,
                    working_directory: canonical
                        .location_path
                        .clone()
                        .unwrap_or(canonical.current_path.clone()),
                    session_name: canonical.session_name,
                    pane_id: canonical.pane_id,
                    attached_at: current_unix_seconds(),
                },
            )
        } else {
            return Err("The suggested agent is not in a registered Workspace location".into());
        }
        .map_err(|error| error.to_string())?;
        let run = decision
            .state
            .runs
            .last()
            .cloned()
            .ok_or_else(|| "Run attachment produced no Run".to_owned())?;
        self.commit(decision)?;
        Ok(run)
    }

    fn open_terminal(
        &mut self,
        app: &AppHandle,
        run_id: i64,
        terminal_id: String,
        session_name: String,
        pane_id: String,
    ) -> Result<TerminalAttachment, String> {
        if terminal_id.trim().is_empty() {
            return Err("A terminal identity is required".to_owned());
        }
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id && run.session_name == session_name)
            .cloned()
            .ok_or_else(|| "The Pane does not belong to that Run".to_owned())?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        let pane_exists = list_panes(&machine, &session_name)?
            .iter()
            .any(|pane| pane.pane_id == pane_id);
        if !pane_exists {
            return Err(format!(
                "Pane {pane_id} is not available in session {session_name}"
            ));
        }

        if let Some(previous) = self.terminal_connections.remove(&terminal_id) {
            previous.close()?;
        }
        let output_app = app.clone();
        let output_terminal_id = terminal_id.clone();
        let output_pane_id = pane_id.clone();
        let exit_app = app.clone();
        let exit_terminal_id = terminal_id.clone();
        let exit_pane_id = pane_id.clone();
        let state_app = app.clone();
        let connection = TmuxControlPane::attach(
            &machine,
            &session_name,
            &pane_id,
            move |data| {
                let _ = output_app.emit(
                    "terminal-output",
                    TerminalOutputEvent {
                        terminal_id: output_terminal_id.clone(),
                        pane_id: output_pane_id.clone(),
                        data,
                    },
                );
            },
            move |record| {
                let run_id = record.run_id.parse::<i64>().ok();
                let Some(app_state) = state_app.try_state::<Mutex<Runtime>>() else {
                    return;
                };
                let Ok(mut runtime) = app_state.lock() else {
                    return;
                };
                if let Some(run_id) = run_id {
                    match runtime.apply_agent_state_record(run_id, record.clone()) {
                        Ok(true) => {
                            let _ = state_app.emit(
                                "run-state-changed",
                                RunStateChangedEvent {
                                    run_id,
                                    state: record.state,
                                },
                            );
                        }
                        Ok(false) => {}
                        Err(error) => {
                            eprintln!("Could not persist state for Run {run_id}: {error}");
                        }
                    }
                }
            },
            move |code| {
                let _ = exit_app.emit(
                    "terminal-exit",
                    TerminalExitEvent {
                        terminal_id: exit_terminal_id.clone(),
                        pane_id: exit_pane_id.clone(),
                        code,
                    },
                );
            },
        )?;
        let snapshot = match capture_pane(&machine, &pane_id) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.close();
                return Err(error);
            }
        };
        let panes = match self.list_run_panes(run_id) {
            Ok(panes) => panes,
            Err(error) => {
                let _ = connection.close();
                return Err(error);
            }
        };
        self.terminal_connections
            .insert(terminal_id.clone(), connection);
        Ok(TerminalAttachment {
            terminal_id,
            session_name,
            pane_id,
            snapshot,
            panes,
        })
    }

    fn list_run_panes(&mut self, run_id: i64) -> Result<Vec<PaneTab>, String> {
        self.recover_run_states()?;
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        let panes = list_panes(&machine, &run.session_name)?;
        Ok(panes
            .into_iter()
            .map(|pane| PaneTab::from_summary(&run, &run.session_name, pane, true))
            .collect())
    }

    fn terminal_input(&self, terminal_id: &str, input: Vec<u8>) -> Result<(), String> {
        self.terminal_connections
            .get(terminal_id)
            .ok_or_else(|| "The embedded terminal is not attached".to_owned())?
            .send_input(&input)
    }

    fn terminal_resize(&self, terminal_id: &str, columns: u16, rows: u16) -> Result<(), String> {
        self.terminal_connections
            .get(terminal_id)
            .ok_or_else(|| "The embedded terminal is not attached".to_owned())?
            .resize(columns, rows)
    }

    fn close_terminal(&mut self, terminal_id: &str) -> Result<(), String> {
        if let Some(connection) = self.terminal_connections.remove(terminal_id) {
            connection.close()?;
        }
        Ok(())
    }

    fn stop_run(&mut self, run_id: i64) -> Result<Run, String> {
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;

        TmuxRuntime
            .kill_pane(&machine, &run.session_name, &run.pane_id)
            .map_err(|error| format!("Could not stop Run {run_id}: {error}"))?;
        let decision = decide(
            self.state.clone(),
            Event::SetRunPaneStatus {
                run_id,
                status: RunPaneStatus::Missing,
            },
        )
        .map_err(|error| error.to_string())?;
        let stopped = decision
            .state
            .runs
            .iter()
            .find(|candidate| candidate.id == run_id)
            .cloned()
            .ok_or_else(|| "Run stop produced no Run".to_owned())?;
        self.commit_with_audit(decision, &[AuditAction::RunStopped { run_id }])?;
        Ok(stopped)
    }

    fn open_external_terminal(&self, run_id: i64) -> Result<(), String> {
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        let panes = list_panes(&machine, &run.session_name).map_err(|error| {
            format!(
                "Pane {} is not available in session {}: {error}",
                run.pane_id, run.session_name
            )
        })?;
        if !panes.iter().any(|pane| pane.pane_id == run.pane_id) {
            return Err(format!(
                "Pane {} is not available in session {}",
                run.pane_id, run.session_name
            ));
        }

        let transport = terminal_transport(&machine);
        let identity = ExternalPaneIdentity {
            tmux_path: "tmux".into(),
            socket_name: machine.socket_name,
            session_name: run.session_name,
            pane_id: run.pane_id,
            transport,
        };
        open_pane_in_terminal(&identity)
    }

    fn provision_agent_hooks(&self) -> Result<(), String> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set; agent hooks cannot be provisioned".to_owned())?;
        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the Mission Manager executable: {error}"))?;
        provision_hooks(&home, &executable)
    }

    fn recover_run_states(&mut self) -> Result<(), String> {
        for run in self.state.runs.clone() {
            let path = state_file_path(&self.agent_state_directory, run.id);
            let record = match read_state_file(&path) {
                Ok(record) => record,
                Err(error) => {
                    if path.exists() {
                        eprintln!(
                            "Could not recover state for Run {} from {}: {error}",
                            run.id,
                            path.display()
                        );
                    }
                    continue;
                }
            };
            let Ok(record_run_id) = record.run_id.parse::<i64>() else {
                continue;
            };
            if record_run_id != run.id || record.agent != run.agent {
                continue;
            }
            self.apply_agent_state_record(record_run_id, record)?;
        }
        Ok(())
    }

    fn reconcile_runs(&mut self) -> Result<(), String> {
        self.recover_run_states()?;
        for run in self.state.runs.clone() {
            let Some(machine) = self
                .state
                .machines
                .iter()
                .find(|machine| machine.id == run.machine_id)
            else {
                continue;
            };
            let pane_status = if probe_machine(machine).is_err() {
                RunPaneStatus::Unknown
            } else {
                match list_panes(machine, &run.session_name) {
                    Ok(panes) if panes.iter().any(|pane| pane.pane_id == run.pane_id) => {
                        RunPaneStatus::Available
                    }
                    Ok(_) | Err(_) => RunPaneStatus::Missing,
                }
            };
            if pane_status == run.pane_status {
                continue;
            }
            let decision = decide(
                self.state.clone(),
                Event::SetRunPaneStatus {
                    run_id: run.id,
                    status: pane_status,
                },
            )
            .map_err(|error| error.to_string())?;
            self.commit(decision)?;
        }
        Ok(())
    }

    fn apply_agent_state_record(
        &mut self,
        run_id: i64,
        record: AgentStateRecord,
    ) -> Result<bool, String> {
        let Some(run) = self.state.runs.iter().find(|run| run.id == run_id) else {
            return Ok(false);
        };
        if run.agent != record.agent || run.state == record.state {
            return Ok(false);
        }
        let decision = decide(
            self.state.clone(),
            Event::UpdateRunState {
                run_id,
                state: record.state,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        Ok(true)
    }

    fn item_context_id(&self, item_id: i64) -> Result<i64, String> {
        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| format!("Item {item_id} does not exist"))?;
        self.state
            .projects
            .iter()
            .find(|project| project.id == item.project_id)
            .map(|project| project.context_id)
            .ok_or_else(|| format!("Project {} does not exist", item.project_id))
    }

    fn build_item_deletion_preview(&self, item_id: i64) -> Result<ItemDeletionPreview, String> {
        let plan = plan_item_deletion(&self.state, item_id).map_err(|error| error.to_string())?;
        let blockers = plan
            .active_run_ids
            .iter()
            .map(|run_id| format!("Run #{run_id} is active; stop it before deleting this Item."))
            .collect::<Vec<_>>();
        Ok(ItemDeletionPreview { plan, blockers })
    }

    fn build_parent_deletion_preview(
        &self,
        target: ParentDeletionTarget,
    ) -> Result<ParentDeletionPreview, String> {
        let plan = match target {
            ParentDeletionTarget::Project(project_id) => {
                plan_project_deletion(&self.state, project_id)
            }
            ParentDeletionTarget::Context(context_id) => {
                plan_context_deletion(&self.state, context_id)
            }
        }
        .map_err(|error| error.to_string())?;
        let mut blockers = plan
            .active_run_ids
            .iter()
            .map(|run_id| {
                format!(
                    "Run #{run_id} is active; stop it before deleting this {}.",
                    target.label()
                )
            })
            .collect::<Vec<_>>();
        if matches!(target, ParentDeletionTarget::Context(_)) && self.state.contexts.len() == 1 {
            blockers.push(
                "This is the last Context; create another Context before deleting it.".into(),
            );
        }
        Ok(ParentDeletionPreview { plan, blockers })
    }

    fn prepare_project_deletion(
        &mut self,
        project_id: i64,
    ) -> Result<ParentDeletionPreview, String> {
        let preview =
            self.build_parent_deletion_preview(ParentDeletionTarget::Project(project_id))?;
        self.pending_parent_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn prepare_context_deletion(
        &mut self,
        context_id: i64,
    ) -> Result<ParentDeletionPreview, String> {
        let preview =
            self.build_parent_deletion_preview(ParentDeletionTarget::Context(context_id))?;
        self.pending_parent_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn build_reset_local_data_preview(&self) -> Result<ResetLocalDataPreview, String> {
        let plan = plan_reset_local_data(&self.state);
        let mut blockers = self
            .state
            .runs
            .iter()
            .filter(|run| run.state != RunState::Finished)
            .map(|run| {
                format!(
                    "Run #{} is active; stop it before resetting local data.",
                    run.id
                )
            })
            .collect::<Vec<_>>();
        Ok(ResetLocalDataPreview {
            plan,
            audit_entry_count: self
                .store
                .audit_entry_count()
                .map_err(|error| error.to_string())?,
            blockers,
            confirmation_phrase: RESET_CONFIRMATION_PHRASE.into(),
        })
    }

    fn prepare_reset_local_data(&mut self) -> Result<ResetLocalDataPreview, String> {
        let preview = self.build_reset_local_data_preview()?;
        self.pending_reset_local_data = Some(preview.clone());
        Ok(preview)
    }

    fn reset_all_local_data(
        &mut self,
        confirmation: String,
    ) -> Result<ResetLocalDataResult, String> {
        if confirmation != RESET_CONFIRMATION_PHRASE {
            return Err(format!(
                "Reset requires the exact confirmation phrase: {RESET_CONFIRMATION_PHRASE}"
            ));
        }
        let pending = self
            .pending_reset_local_data
            .as_ref()
            .cloned()
            .ok_or_else(|| "Review the reset preview before resetting local data".to_owned())?;
        let current = self.build_reset_local_data_preview()?;
        if current != pending {
            return Err(
                "The local model changed after the reset preview; review the updated preview before resetting local data"
                    .into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Reset is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let decision =
            decide(self.state.clone(), Event::ResetLocalData).map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_reset_local_data = None;
        self.pending_item_deletion = None;
        self.pending_external_object_deletion = None;
        self.pending_repository_deletion = None;
        self.pending_machine_deletion = None;
        self.pending_parent_deletion = None;
        self.terminal_connections.clear();

        if self.agent_state_directory.exists() {
            let _ = fs::remove_dir_all(&self.agent_state_directory);
        }

        Ok(ResetLocalDataResult {
            summary: current.plan.summary,
            audit_entry_count: current.audit_entry_count,
        })
    }

    fn delete_parent(
        &mut self,
        target: ParentDeletionTarget,
        event: Event,
        confirmed: bool,
    ) -> Result<ParentDeletionResult, String> {
        if !confirmed {
            return Err(format!(
                "{} deletion requires explicit confirmation after reviewing its deletion preview",
                target.label()
            ));
        }
        let pending = self
            .pending_parent_deletion
            .as_ref()
            .filter(|preview| target.matches(&preview.plan))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Review the {} deletion preview before deleting it",
                    target.label()
                )
            })?;
        let current = self.build_parent_deletion_preview(target)?;
        if current != pending {
            return Err(format!(
                "The {} changed after the preview; review the updated deletion preview",
                target.label()
            ));
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "{} deletion is blocked:\n{}",
                target.label(),
                current.blockers.join("\n")
            ));
        }
        let summary = current.plan.summary();
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_parent_deletion = None;
        Ok(ParentDeletionResult { summary })
    }

    fn delete_project(
        &mut self,
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<ParentDeletionResult, String> {
        self.delete_parent(
            ParentDeletionTarget::Project(project_id),
            Event::DeleteProject {
                project_id,
                item_ids,
                repository_ids,
                workspace_ids,
            },
            confirmed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn delete_context(
        &mut self,
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        machine_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<ParentDeletionResult, String> {
        self.delete_parent(
            ParentDeletionTarget::Context(context_id),
            Event::DeleteContext {
                context_id,
                project_ids,
                item_ids,
                repository_ids,
                workspace_ids,
                machine_ids,
            },
            confirmed,
        )
    }

    fn prepare_repository_deletion(
        &mut self,
        repository_id: i64,
    ) -> Result<RepositoryDeletionPreview, String> {
        let preview = self.build_repository_deletion_preview(repository_id)?;
        self.pending_repository_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn build_machine_deletion_preview(
        &self,
        machine_id: i64,
    ) -> Result<MachineDeletionPreview, String> {
        let plan =
            plan_machine_deletion(&self.state, machine_id).map_err(|error| error.to_string())?;
        let blockers = plan
            .active_run_ids
            .iter()
            .map(|run_id| {
                format!(
                    "Run #{run_id} is active on Machine {}; stop it before deleting the Machine.",
                    plan.name
                )
            })
            .collect();
        Ok(MachineDeletionPreview { plan, blockers })
    }

    fn prepare_machine_deletion(
        &mut self,
        machine_id: i64,
    ) -> Result<MachineDeletionPreview, String> {
        let preview = self.build_machine_deletion_preview(machine_id)?;
        self.pending_machine_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn prepare_item_deletion(&mut self, item_id: i64) -> Result<ItemDeletionPreview, String> {
        let preview = self.build_item_deletion_preview(item_id)?;
        self.pending_item_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn build_external_object_deletion_preview(
        &self,
        external_object_id: i64,
    ) -> Result<ExternalObjectDeletionPreview, String> {
        let plan = plan_external_object_deletion(&self.state, external_object_id)
            .map_err(|error| error.to_string())?;
        let links = plan
            .link_ids
            .iter()
            .map(|link_id| {
                let link = self
                    .state
                    .links
                    .iter()
                    .find(|link| link.id == *link_id)
                    .ok_or_else(|| format!("Link {link_id} disappeared while building preview"))?;
                let item = self
                    .state
                    .items
                    .iter()
                    .find(|item| item.id == link.item_id)
                    .ok_or_else(|| {
                        format!("Item {} disappeared while building preview", link.item_id)
                    })?;
                Ok(ExternalObjectLinkDeletionPreview {
                    link_id: link.id,
                    item_id: item.id,
                    item_identifier: item.human_identifier.clone(),
                    item_title: item.title.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(ExternalObjectDeletionPreview {
            plan,
            links,
            provider_warning: "This only removes local Mission Manager data. GitHub Issues, pull requests, and other provider-owned objects are never deleted.".into(),
        })
    }

    fn prepare_external_object_deletion(
        &mut self,
        external_object_id: i64,
    ) -> Result<ExternalObjectDeletionPreview, String> {
        let preview = self.build_external_object_deletion_preview(external_object_id)?;
        self.pending_external_object_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn delete_external_object(
        &mut self,
        external_object_id: i64,
        confirmed: bool,
    ) -> Result<ExternalObjectDeletionResult, String> {
        if !confirmed {
            return Err(
                "External Object deletion requires explicit confirmation after reviewing its local deletion preview".into(),
            );
        }
        let pending = self
            .pending_external_object_deletion
            .as_ref()
            .filter(|preview| preview.plan.external_object_id == external_object_id)
            .cloned()
            .ok_or_else(|| {
                "Review the External Object deletion preview before deleting it".to_owned()
            })?;
        let current = self.build_external_object_deletion_preview(external_object_id)?;
        if current != pending {
            return Err(
                "The External Object or one of its Links changed after the preview; review the updated local deletion preview before deleting it".into(),
            );
        }

        let summary = current.plan.summary();
        let decision = decide(
            self.state.clone(),
            Event::DeleteExternalObject { external_object_id },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_external_object_deletion = None;
        Ok(ExternalObjectDeletionResult { summary })
    }

    fn unlink_external_link(
        &mut self,
        link_id: i64,
        confirmed: bool,
    ) -> Result<ExternalLinkDeletionResult, String> {
        if !confirmed {
            return Err("Unlinking an Item requires explicit confirmation".into());
        }
        let external_object_id = self
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .map(|link| link.external_object_id)
            .ok_or_else(|| format!("Link {link_id} does not exist"))?;
        let decision = decide(self.state.clone(), Event::DeleteLink { link_id })
            .map_err(|error| error.to_string())?;
        let external_object_deleted = !decision
            .state
            .external_objects
            .iter()
            .any(|object| object.id == external_object_id);
        self.commit(decision)?;
        Ok(ExternalLinkDeletionResult {
            link_id,
            external_object_id,
            external_object_deleted,
        })
    }

    fn build_repository_deletion_preview(
        &self,
        repository_id: i64,
    ) -> Result<RepositoryDeletionPreview, String> {
        let plan = plan_repository_deletion(&self.state, repository_id)
            .map_err(|error| error.to_string())?;
        let blockers = plan
            .workspaces
            .iter()
            .filter_map(|workspace| {
                self.state
                    .runs
                    .iter()
                    .find(|run| run.workspace_id == Some(workspace.id))
                    .map(|run| {
                        format!(
                            "Workspace #{} has Run #{} history and cannot be removed with the Repository.",
                            workspace.id, run.id
                        )
                    })
            })
            .collect();
        Ok(RepositoryDeletionPreview { plan, blockers })
    }

    fn delete_item(&mut self, item_id: i64, confirmed: bool) -> Result<ItemDeletionResult, String> {
        if !confirmed {
            return Err(
                "Item deletion requires explicit confirmation after reviewing its deletion preview"
                    .into(),
            );
        }
        let pending = self
            .pending_item_deletion
            .as_ref()
            .filter(|preview| preview.plan.item_id == item_id)
            .cloned()
            .ok_or_else(|| "Review the Item deletion preview before deleting it".to_owned())?;
        let current = self.build_item_deletion_preview(item_id)?;
        if current != pending {
            return Err(
                "The Item changed after the preview; review the updated deletion preview".into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Item deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }
        let summary = current.plan.summary();
        let decision = decide(self.state.clone(), Event::DeleteItem { item_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_item_deletion = None;
        Ok(ItemDeletionResult { summary })
    }

    fn delete_repository(
        &mut self,
        repository_id: i64,
        _workspace_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<RepositoryDeletionResult, String> {
        if !confirmed {
            return Err("Repository deletion requires explicit confirmation after reviewing its deletion preview".into());
        }
        let pending = self
            .pending_repository_deletion
            .as_ref()
            .filter(|preview| preview.plan.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                "Review the Repository deletion preview before deleting it".to_owned()
            })?;
        let current = self.build_repository_deletion_preview(repository_id)?;
        if current != pending {
            return Err("The Repository or its Workspace relationships changed after the preview; review the updated deletion preview".into());
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Repository deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }
        let workspace_count = current.plan.workspaces.len();
        let decision = decide(
            self.state.clone(),
            Event::DeleteRepository {
                repository_id,
                workspace_ids: current
                    .plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect(),
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_repository_deletion = None;
        Ok(RepositoryDeletionResult {
            repository_id,
            workspace_count,
        })
    }

    fn delete_machine(
        &mut self,
        machine_id: i64,
        run_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<MachineDeletionResult, String> {
        if !confirmed {
            return Err(
                "Machine deletion requires explicit confirmation after reviewing its deletion preview"
                    .into(),
            );
        }
        let pending = self
            .pending_machine_deletion
            .as_ref()
            .filter(|preview| preview.plan.machine_id == machine_id)
            .cloned()
            .ok_or_else(|| "Review the Machine deletion preview before deleting it".to_owned())?;
        let current = self.build_machine_deletion_preview(machine_id)?;
        if current != pending {
            return Err(
                "The Machine or one of its Run states changed after the preview; review the updated deletion preview before deleting it"
                    .into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Machine deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let run_count = current.plan.runs.len();
        let decision = decide(
            self.state.clone(),
            Event::DeleteMachine {
                machine_id,
                run_ids,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_machine_deletion = None;

        Ok(MachineDeletionResult {
            machine_id,
            run_count,
        })
    }

    fn delete_run(&mut self, run_id: i64, confirmed: bool) -> Result<RunDeletionResult, String> {
        if !confirmed {
            return Err("Run deletion requires explicit confirmation".into());
        }
        let decision = decide(self.state.clone(), Event::DeleteRun { run_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        Ok(RunDeletionResult { run_id })
    }

    fn repository(&self, repository_id: i64) -> Result<Repository, String> {
        self.state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))
    }

    fn create_item(
        &mut self,
        title: String,
        context_id: i64,
        project_id: i64,
    ) -> Result<Item, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateItem {
                title,
                context_id,
                project_id,
            },
        )
        .map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .last()
            .cloned()
            .ok_or_else(|| "Item creation produced no Item".to_owned())?;
        self.commit(decision)?;
        Ok(item)
    }

    fn update_item(&mut self, event: Event, item_id: i64) -> Result<Item, String> {
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| "Item update produced no Item".to_owned())?;
        self.commit(decision)?;
        Ok(item)
    }

    fn set_item_relation(
        &mut self,
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    ) -> Result<ItemRelation, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetItemRelation {
                from_item_id,
                to_item_id,
                kind,
            },
        )
        .map_err(|error| error.to_string())?;
        let relation = decision
            .state
            .relationships
            .last()
            .cloned()
            .ok_or_else(|| "Item relationship produced no relationship".to_owned())?;
        self.commit(decision)?;
        Ok(relation)
    }

    fn link_external_object(
        &mut self,
        item_id: i64,
        url: String,
    ) -> Result<ExternalLinkAction, String> {
        let object_input = classify_url(&url).map_err(|error| error.to_string())?;
        let known_object = self.state.external_objects.iter().find(|object| {
            object.provider == object_input.provider
                && object.external_key == object_input.external_key
        });
        let mut warning = None;
        let snapshot =
            if object_input.provider == ExternalProvider::GitHub && known_object.is_none() {
                match self.fetch_github_object(&object_input) {
                    Ok(snapshot) => Some(snapshot),
                    Err(error) => {
                        warning = Some(error);
                        None
                    }
                }
            } else {
                None
            };
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id,
                object: object_input,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| "Link creation produced no Link".to_owned())?;
        self.commit(decision)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| "Link creation produced no External Object".to_owned())?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    fn create_github_issue(
        &mut self,
        item_id: i64,
        repository: String,
        title: String,
        body: String,
    ) -> Result<ExternalLinkAction, String> {
        let item_exists = self.state.items.iter().any(|item| item.id == item_id);
        if !item_exists {
            return Err(format!("Item {item_id} does not exist"));
        }
        if repository.trim().is_empty() {
            return Err("A GitHub repository is required".into());
        }
        if title.trim().is_empty() {
            return Err("A GitHub Issue title is required".into());
        }

        let executable = self.gh_executable_path()?;
        let created_url = GithubCli::new(executable.clone())
            .create_issue(repository.trim(), title.trim(), &body)
            .map_err(|error| error.to_string())?;
        let object = classify_url(&created_url).map_err(|error| error.to_string())?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue {
            return Err("GitHub CLI returned a URL that is not a GitHub Issue".into());
        }

        let mut warning = None;
        let snapshot = match GithubCli::new(executable).fetch(&object, current_unix_seconds()) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                warning = Some(error.to_string());
                None
            }
        };
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id,
                object,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| "Issue creation produced no Link".to_owned())?;
        self.commit(decision)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| "Issue creation produced no External Object".to_owned())?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    fn add_external_comment(
        &mut self,
        link_id: i64,
        body: String,
    ) -> Result<ExternalLinkView, String> {
        let body = body.trim().to_owned();
        if body.is_empty() {
            return Err("A GitHub comment cannot be blank".into());
        }
        let link = self
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| format!("Link {link_id} does not exist"))?;
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
            .cloned()
            .ok_or_else(|| "The linked External Object does not exist".to_owned())?;
        if object.provider != ExternalProvider::GitHub || object.kind == ExternalObjectKind::Generic
        {
            return Err("Comments are only supported for GitHub Issues and pull requests".into());
        }

        let executable = self.gh_executable_path()?;
        GithubCli::new(executable)
            .add_comment(&object.canonical_url, &body)
            .map_err(|error| error.to_string())?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| "Comment target produced no External Object".to_owned())
    }

    fn refresh_external_object(
        &mut self,
        external_object_id: i64,
    ) -> Result<ExternalSnapshot, String> {
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == external_object_id)
            .cloned()
            .ok_or_else(|| format!("External Object {external_object_id} does not exist"))?;
        if object.provider != ExternalProvider::GitHub {
            return Err("Only GitHub External Objects can be refreshed".into());
        }
        let input = ExternalObjectInput {
            provider: object.provider,
            kind: object.kind,
            external_key: object.external_key.clone(),
            canonical_url: object.canonical_url.clone(),
        };
        let snapshot = self.fetch_github_object(&input)?;
        let decision = decide(
            self.state.clone(),
            Event::RefreshExternalObject {
                external_object_id,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let snapshot = decision
            .state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == external_object_id)
            .cloned()
            .ok_or_else(|| "Refresh produced no External snapshot".to_owned())?;
        self.commit(decision)?;
        Ok(snapshot)
    }

    fn poll_external_objects(&mut self) -> PollResult {
        let mut object_ids = self
            .state
            .links
            .iter()
            .filter_map(|link| {
                self.state
                    .external_objects
                    .iter()
                    .find(|object| object.id == link.external_object_id)
                    .filter(|object| object.provider == ExternalProvider::GitHub)
                    .map(|object| object.id)
            })
            .collect::<Vec<_>>();
        object_ids.sort_unstable();
        object_ids.dedup();

        let mut result = PollResult {
            refreshed: 0,
            failures: Vec::new(),
        };
        for external_object_id in object_ids {
            match self.refresh_external_object(external_object_id) {
                Ok(_) => result.refreshed += 1,
                Err(error) => result.failures.push(PollFailure {
                    external_object_id,
                    error,
                }),
            }
        }
        result
    }

    fn set_link_attention_policy(
        &mut self,
        link_id: i64,
        policy: Option<ExternalChangePolicy>,
    ) -> Result<ExternalLinkView, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetLinkAttentionPolicy { link_id, policy },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| "Link policy update produced no Link".to_owned())?;
        self.commit(decision)?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| "Link policy update produced no External Object".to_owned())
    }

    fn set_link_schedule(
        &mut self,
        event: Event,
        link_id: i64,
        error_prefix: &str,
    ) -> Result<ExternalLinkView, String> {
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| format!("{error_prefix} produced no Link"))?;
        self.commit(decision)?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| format!("{error_prefix} produced no External Object"))
    }

    fn set_context_attention_default(
        &mut self,
        context_id: i64,
        object_kind: ExternalObjectKind,
        policy: ExternalChangePolicy,
    ) -> Result<ContextAttentionDefault, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetContextAttentionDefault {
                context_id,
                object_kind,
                policy,
            },
        )
        .map_err(|error| error.to_string())?;
        let attention_default = decision
            .state
            .attention_defaults
            .iter()
            .find(|attention_default| {
                attention_default.context_id == context_id
                    && attention_default.object_kind == object_kind
            })
            .cloned()
            .ok_or_else(|| "Context default update produced no default".to_owned())?;
        self.commit(decision)?;
        Ok(attention_default)
    }

    fn mark_link_reviewed(&mut self, link_id: i64) -> Result<ExternalLinkView, String> {
        let decision = decide(self.state.clone(), Event::MarkLinkReviewed { link_id })
            .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| "Mark reviewed produced no Link".to_owned())?;
        self.commit(decision)?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| "Mark reviewed produced no External Object".to_owned())
    }

    fn fetch_github_object(
        &mut self,
        object: &ExternalObjectInput,
    ) -> Result<crate::domain::ExternalSnapshotData, String> {
        let executable = self.gh_executable_path()?;
        GithubCli::new(executable)
            .fetch(object, current_unix_seconds())
            .map_err(|error| error.to_string())
    }

    fn gh_executable_path(&mut self) -> Result<PathBuf, String> {
        let stored_path = self.gh_executable_path.as_deref();
        let executable = resolve_gh_executable(stored_path).map_err(|error| error.to_string())?;
        if self.gh_executable_path.as_deref() != Some(executable.as_path()) {
            self.store
                .set_gh_executable_path(&executable)
                .map_err(|error| error.to_string())?;
            self.gh_executable_path = Some(executable.clone());
        }
        Ok(executable)
    }

    fn list_audit_history(&self) -> Result<Vec<AuditEntry>, String> {
        self.store
            .list_audit_history()
            .map_err(|error| error.to_string())
    }

    fn activity_tab(&self) -> Result<ActivityTabView, String> {
        let audit_entries = self
            .store
            .list_audit_history()
            .map_err(|error| error.to_string())?;
        Ok(activity_tab_view(&self.state, audit_entries))
    }

    fn commit(&mut self, decision: crate::domain::Decision) -> Result<(), String> {
        self.commit_with_audit(decision, &[])
    }

    fn commit_with_audit(
        &mut self,
        decision: crate::domain::Decision,
        additional_actions: &[AuditAction],
    ) -> Result<(), String> {
        let mut audit_actions = audit_actions(&self.state, &decision.effects);
        audit_actions.extend_from_slice(additional_actions);
        self.store
            .apply_with_audit(&decision.effects, &audit_actions)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        Ok(())
    }
}

fn looks_like_authentication_failure(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    [
        "not logged in",
        "not authenticated",
        "no accounts",
        "authentication token",
        "token is invalid",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
}

fn audit_actions(before: &DomainState, effects: &[Effect]) -> Vec<AuditAction> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::PersistContext { context, .. } => Some(AuditAction::ContextCreated {
                context_id: context.id,
            }),
            Effect::PersistProject { project, .. } => Some(AuditAction::ProjectCreated {
                project_id: project.id,
            }),
            Effect::PersistRepository { repository, .. } => {
                Some(AuditAction::RepositoryRegistered {
                    repository_id: repository.id,
                })
            }
            Effect::ResetLocalData {
                context, project, ..
            } => Some(AuditAction::ResetBoundary {
                context_id: context.id,
                project_id: project.id,
            }),
            Effect::PersistItem { item, .. } => Some(AuditAction::ItemCreated { item_id: item.id }),
            Effect::PersistItemUpdate { item } => {
                let previous = before
                    .items
                    .iter()
                    .find(|candidate| candidate.id == item.id);
                match previous {
                    Some(previous) if previous.title != item.title => {
                        Some(AuditAction::ItemTitleChanged { item_id: item.id })
                    }
                    Some(previous) if previous.status != item.status => {
                        Some(AuditAction::ItemStatusChanged {
                            item_id: item.id,
                            from: previous.status,
                            to: item.status,
                        })
                    }
                    Some(previous) if previous.notes != item.notes => {
                        Some(AuditAction::ItemNotesChanged { item_id: item.id })
                    }
                    Some(_) => None,
                    None => Some(AuditAction::ItemNotesChanged { item_id: item.id }),
                }
            }
            Effect::PersistItemReminders { item, .. } => {
                Some(AuditAction::ItemRemindersChanged { item_id: item.id })
            }
            Effect::PersistWorkspace { workspace, .. } => Some(AuditAction::WorkspaceCreated {
                workspace_id: workspace.id,
            }),
            Effect::PersistWorkspaceUpdate { workspace } => {
                let previous = before
                    .workspaces
                    .iter()
                    .find(|candidate| candidate.id == workspace.id);
                match previous {
                    Some(previous)
                        if previous.item_id == workspace.item_id
                            && previous.repositories == workspace.repositories
                            && previous.preparation_state == workspace.preparation_state =>
                    {
                        None
                    }
                    _ => Some(AuditAction::WorkspaceUpdated {
                        workspace_id: workspace.id,
                    }),
                }
            }
            Effect::RemoveWorkspace { workspace_id } => Some(AuditAction::WorkspaceRemoved {
                workspace_id: *workspace_id,
                repository_count: before
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == *workspace_id)
                    .map(|workspace| workspace.repositories.len())
                    .unwrap_or_default()
                    .into(),
            }),
            Effect::RemoveRepository { repository_id } => Some(AuditAction::RepositoryDeleted {
                repository_id: *repository_id,
                workspace_count: before
                    .workspaces
                    .iter()
                    .filter(|workspace| {
                        workspace
                            .repositories
                            .iter()
                            .any(|repository| repository.repository_id == *repository_id)
                    })
                    .count()
                    .into(),
            }),
            Effect::RemoveMachine { machine_id } => Some(AuditAction::MachineDeleted {
                machine_id: *machine_id,
                run_count: before
                    .runs
                    .iter()
                    .filter(|run| run.machine_id == *machine_id)
                    .count()
                    .into(),
            }),
            Effect::RemoveRun { run_id } => Some(AuditAction::RunDeleted { run_id: *run_id }),
            Effect::RemoveLink {
                link_id,
                external_object_id,
            } => Some(AuditAction::LinkDeleted {
                link_id: *link_id,
                external_object_id: Some(*external_object_id),
                external_object_deleted: Some(
                    before
                        .links
                        .iter()
                        .filter(|link| link.external_object_id == *external_object_id)
                        .count()
                        == 1,
                ),
            }),
            Effect::RemoveExternalObject { external_object_id } => {
                Some(AuditAction::ExternalObjectDeleted {
                    external_object_id: *external_object_id,
                    link_count: Some(
                        before
                            .links
                            .iter()
                            .filter(|link| link.external_object_id == *external_object_id)
                            .count(),
                    ),
                    snapshot_count: Some(
                        before
                            .snapshots
                            .iter()
                            .filter(|snapshot| snapshot.external_object_id == *external_object_id)
                            .count(),
                    ),
                    activity_count: Some(
                        before
                            .activities
                            .iter()
                            .filter(|activity| activity.external_object_id == *external_object_id)
                            .count(),
                    ),
                })
            }
            Effect::RemoveItemCascade { summary, .. } => Some(AuditAction::ItemDeleted {
                summary: summary.clone(),
            }),
            Effect::RemoveProjectCascade { summary, .. } => Some(AuditAction::ProjectDeleted {
                summary: summary.clone(),
            }),
            Effect::RemoveContextCascade { summary, .. } => Some(AuditAction::ContextDeleted {
                summary: summary.clone(),
            }),
            Effect::PersistMachine { machine, .. } => Some(AuditAction::MachineRegistered {
                machine_id: machine.id,
            }),
            Effect::PersistMachineObservation { machine } => {
                let previous = before
                    .machines
                    .iter()
                    .find(|candidate| candidate.id == machine.id);
                (previous.is_none()
                    || previous.map(|candidate| candidate.last_observed)
                        != Some(machine.last_observed)
                    || previous.and_then(|candidate| candidate.last_observed_at)
                        != machine.last_observed_at)
                    .then_some(AuditAction::MachineObserved {
                        machine_id: machine.id,
                        observation: machine.last_observed,
                    })
            }
            Effect::PersistRun { run, .. } => Some(AuditAction::RunCreated { run_id: run.id }),
            Effect::PersistRunState { run } => {
                let previous = before.runs.iter().find(|candidate| candidate.id == run.id);
                previous
                    .filter(|previous| previous.state != run.state)
                    .map(|previous| AuditAction::RunStateChanged {
                        run_id: run.id,
                        from: previous.state,
                        to: run.state,
                    })
            }
            Effect::PersistRunPaneStatus { run } => {
                let previous = before.runs.iter().find(|candidate| candidate.id == run.id);
                previous
                    .filter(|previous| previous.pane_status != run.pane_status)
                    .map(|previous| AuditAction::RunPaneStatusChanged {
                        run_id: run.id,
                        from: previous.pane_status,
                        to: run.pane_status,
                    })
            }
            Effect::PersistWorktree { .. }
            | Effect::RemoveWorktree { .. }
            | Effect::UpdateRepository { .. }
            | Effect::PersistRepositoryLocation { .. } => None,
            Effect::PersistItemRelation { relation } => Some(AuditAction::ItemRelationChanged {
                from_item_id: relation.from_item_id,
                to_item_id: relation.to_item_id,
                kind: relation.kind,
            }),
            Effect::PersistExternalObject { object, .. } => {
                Some(AuditAction::ExternalObjectCreated {
                    external_object_id: object.id,
                })
            }
            Effect::PersistLink { link, .. } => Some(AuditAction::LinkCreated { link_id: link.id }),
            Effect::PersistLinkState { link } => {
                let previous = before
                    .links
                    .iter()
                    .find(|candidate| candidate.id == link.id);
                previous
                    .filter(|previous| previous != &link)
                    .map(|_| AuditAction::LinkUpdated { link_id: link.id })
            }
            Effect::PersistActivity { activity, .. } => {
                Some(AuditAction::ExternalObjectRefreshed {
                    external_object_id: activity.external_object_id,
                })
            }
            Effect::PersistExternalSnapshot { .. } => None,
            Effect::PersistContextAttentionDefault { attention_default } => {
                Some(AuditAction::ContextAttentionDefaultChanged {
                    context_id: attention_default.context_id,
                    object_kind: attention_default.object_kind,
                })
            }
        })
        .collect()
}

struct CheckoutReceipt {
    root: PathBuf,
    root_was_created: bool,
    destinations: Vec<PathBuf>,
}

impl CheckoutReceipt {
    fn cleanup(self) -> Result<(), String> {
        let mut errors = Vec::new();
        for destination in self.destinations.iter().rev() {
            if destination.exists() {
                if let Err(error) = fs::remove_dir_all(destination) {
                    errors.push(format!("{}: {error}", destination.display()));
                }
            }
        }
        if self.root_was_created && self.root.exists() {
            let is_empty = fs::read_dir(&self.root)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if is_empty {
                if let Err(error) = fs::remove_dir(&self.root) {
                    errors.push(format!("{}: {error}", self.root.display()));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("could not remove checkout: {}", errors.join(", ")))
        }
    }
}

fn format_commit_error(error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) => format!("{error}; {cleanup_error}"),
        None => error,
    }
}

fn restore_staged_directories(staged: &[(PathBuf, PathBuf)]) -> Option<String> {
    let mut errors = Vec::new();
    for (root, staging) in staged.iter().rev() {
        if let Err(error) = fs::rename(staging, root) {
            errors.push(format!(
                "could not restore {} to {}: {error}",
                staging.display(),
                root.display()
            ));
        }
    }
    (!errors.is_empty()).then(|| errors.join(", "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentDeletionTarget {
    Project(i64),
    Context(i64),
}

impl ParentDeletionTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Project(_) => "Project",
            Self::Context(_) => "Context",
        }
    }

    fn matches(self, plan: &ParentDeletionPlan) -> bool {
        match self {
            Self::Project(project_id) => plan.project_id == Some(project_id),
            Self::Context(context_id) => plan.context_id == Some(context_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RepositoryRemovalReport {
    pub repository_id: i64,
    pub name: String,
    pub path: String,
    pub current_branch: String,
    pub unpushed_commits: Vec<String>,
    pub unpushed_commits_unknown: bool,
    pub uncommitted_changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovalReport {
    pub worktree_id: i64,
    pub workspace_id: i64,
    pub repository_id: i64,
    pub repository_name: String,
    pub machine_id: i64,
    pub path: String,
    pub branch: String,
    pub is_dirty: bool,
    pub requires_destructive_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRemovalReport {
    pub workspace_id: i64,
    pub worktrees: Vec<WorktreeRemovalReport>,
    pub safe: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionPreview {
    pub plan: ItemDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionResult {
    pub summary: ItemDeletionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectLinkDeletionPreview {
    pub link_id: i64,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionPreview {
    pub plan: ExternalObjectDeletionPlan,
    pub links: Vec<ExternalObjectLinkDeletionPreview>,
    pub provider_warning: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionResult {
    pub summary: ExternalObjectDeletionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLinkDeletionResult {
    pub link_id: i64,
    pub external_object_id: i64,
    pub external_object_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionPreview {
    pub plan: RepositoryDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionPreview {
    pub plan: MachineDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionPreview {
    pub plan: ParentDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataPreview {
    pub plan: ResetLocalDataPlan,
    pub audit_entry_count: usize,
    pub blockers: Vec<String>,
    pub confirmation_phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataResult {
    pub summary: ResetLocalDataSummary,
    pub audit_entry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionResult {
    pub summary: crate::domain::ParentDeletionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionResult {
    pub machine_id: i64,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDeletionResult {
    pub run_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionResult {
    pub repository_id: i64,
    pub workspace_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovalResult {
    pub worktree_id: i64,
    pub branch_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRemovalResult {
    pub workspace_id: i64,
    pub worktree_count: usize,
    pub branches_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExternalLinkAction {
    pub link: ExternalLinkView,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PollFailure {
    pub external_object_id: i64,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PollResult {
    pub refreshed: usize,
    pub failures: Vec<PollFailure>,
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
    fn from_summary(run: &Run, session_name: &str, pane: PaneSummary, available: bool) -> Self {
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

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

fn agent_executable_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "claude",
        AgentKind::Codex => "codex",
    }
}

fn agent_display_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "Claude Code",
        AgentKind::Codex => "Codex",
    }
}

#[tauri::command]
pub fn list_contexts(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Context>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.contexts.clone())
}

#[tauri::command]
pub fn list_projects(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Project>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.projects.clone())
}

#[tauri::command]
pub fn list_repositories(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Repository>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.repositories.clone())
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_repository_locations(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<crate::domain::RepositoryLocation>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.repository_locations.clone())
}

#[tauri::command]
pub fn list_machines(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Machine>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.machines.clone())
}

#[tauri::command]
pub fn get_setup_state(state: State<'_, Mutex<Runtime>>) -> Result<SetupState, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .setup_state()
}

#[tauri::command(rename_all = "camelCase")]
pub fn complete_setup(
    context_name: String,
    provider: ProviderChoice,
    state: State<'_, Mutex<Runtime>>,
) -> Result<SetupState, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .complete_setup(context_name, provider)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_health_status(
    provider: Option<ProviderChoice>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HealthStatus, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .health_status(provider)
}

#[tauri::command(rename_all = "camelCase")]
pub fn compose_run_prompt(
    item_id: i64,
    execution_profile: ExecutionProfile,
    prompt_selection: RunPromptSelection,
    custom_prompt: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<String, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .compose_run_prompt(item_id, execution_profile, prompt_selection, custom_prompt)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_direct_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_direct_run(item_id, workspace_id, machine_id)
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .start_direct_run(
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
        )
}

#[tauri::command]
pub fn list_run_suggestions(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<RunSuggestion>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .list_run_suggestions()
}

#[tauri::command(rename_all = "camelCase")]
pub fn attach_run(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .attach_run(suggestion)
}

#[tauri::command(rename_all = "camelCase")]
pub fn register_machine(
    context_id: i64,
    name: String,
    socket_name: String,
    transport: MachineTransport,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Machine, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .register_machine(context_id, name, socket_name, transport)
}

#[tauri::command(rename_all = "camelCase")]
pub fn check_machine(machine_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Machine, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .check_machine(machine_id)
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .open_terminal(&app, run_id, terminal_id, session_name, pane_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn terminal_input(
    terminal_id: String,
    input: Vec<u8>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .terminal_input(&terminal_id, input)
}

#[tauri::command(rename_all = "camelCase")]
pub fn terminal_resize(
    terminal_id: String,
    columns: u16,
    rows: u16,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .terminal_resize(&terminal_id, columns, rows)
}

#[tauri::command(rename_all = "camelCase")]
pub fn close_terminal(terminal_id: String, state: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .close_terminal(&terminal_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn open_external_terminal(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .open_external_terminal(run_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn stop_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .stop_run(run_id)
}

#[tauri::command]
pub fn list_audit_history(state: State<'_, Mutex<Runtime>>) -> Result<Vec<AuditEntry>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .list_audit_history()
}

#[tauri::command]
pub fn get_activity_tab(state: State<'_, Mutex<Runtime>>) -> Result<ActivityTabView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .activity_tab()
}

#[tauri::command]
pub fn list_context_attention_defaults(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ContextAttentionDefault>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.attention_defaults.clone())
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_home(
    context_id: Option<i64>,
    now: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HomeView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .and_then(|mut runtime| {
            runtime.recover_run_states()?;
            Ok(home_view(&runtime.state, context_id, &now))
        })
}

#[tauri::command]
pub fn reconcile_runs(state: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .reconcile_runs()
}

#[tauri::command(rename_all = "camelCase")]
pub fn search_items_command(
    query: String,
    context_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ItemView>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| search_items(&runtime.state, &query, context_id))
}

#[tauri::command]
pub fn create_context(name: String, state: State<'_, Mutex<Runtime>>) -> Result<Context, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_context(name)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_project(
    name: String,
    context_id: i64,
    default_item_status: ItemStatus,
    execution_mode: Option<ExecutionMode>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_project(
            name,
            context_id,
            ProjectDefaults {
                item_status: default_item_status,
                execution_mode: execution_mode.unwrap_or(ExecutionMode::Worktree),
            },
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn register_repository(
    project_id: i64,
    name: String,
    remote_url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .register_repository(project_id, name, remote_url)
}

#[tauri::command(rename_all = "camelCase")]
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .register_repository_at_location(
            project_id,
            name,
            remote_url,
            base_branch,
            machine_id,
            checkout_path,
            worktree_root,
            clone_into_destination,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_project_deletion(
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_project_deletion(project_id)
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_project(
            project_id,
            item_ids,
            repository_ids,
            workspace_ids,
            confirmed,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_context_deletion(
    context_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_context_deletion(context_id)
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_context(
            context_id,
            project_ids,
            item_ids,
            repository_ids,
            workspace_ids,
            machine_ids,
            confirmed,
        )
}

#[tauri::command]
pub fn prepare_reset_local_data(
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_reset_local_data()
}

#[tauri::command(rename_all = "camelCase")]
pub fn reset_all_local_data(
    confirmation: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .reset_all_local_data(confirmation)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_workspace(
    item_id: i64,
    repositories: Vec<WorkspaceRepositoryInput>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workspace, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_workspace(item_id, repositories)
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            branch,
            base_branch,
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_worktree(
            workspace_id,
            repository_id,
            machine_id,
            reuse_existing_branch,
            confirm_dirty_attachment,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_worktree_removal(
    worktree_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalReport, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_worktree_removal(worktree_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_worktree(
    worktree_id: i64,
    confirmed: bool,
    destructive_confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .remove_worktree(worktree_id, confirmed, destructive_confirmed)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_workspace_removal(
    workspace_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorkspaceRemovalReport, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_workspace_removal(workspace_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_workspace(
    workspace_id: i64,
    confirmed_worktree_ids: Vec<i64>,
    destructive_worktree_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorkspaceRemovalResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .remove_workspace(
            workspace_id,
            confirmed_worktree_ids,
            destructive_worktree_ids,
            confirmed,
        )
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
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .attach_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            confirm_dirty_attachment,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_repository_deletion(
    repository_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_repository_deletion(repository_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_repository(
    repository_id: i64,
    workspace_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_repository(repository_id, workspace_ids, confirmed)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_machine_deletion(
    machine_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineDeletionPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_machine_deletion(machine_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_machine(
    machine_id: i64,
    run_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_machine(machine_id, run_ids, confirmed)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_item_deletion(
    item_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_item_deletion(item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_item(
    item_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_item(item_id, confirmed)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_external_object_deletion(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalObjectDeletionPreview, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_external_object_deletion(external_object_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_external_object(
    external_object_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalObjectDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_external_object(external_object_id, confirmed)
}

#[tauri::command(rename_all = "camelCase")]
pub fn unlink_external_link(
    link_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .unlink_external_link(link_id, confirmed)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_run(
    run_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RunDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_run(run_id, confirmed)
}

#[tauri::command]
pub fn list_inbox_items(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Item>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| {
            runtime
                .state
                .items
                .iter()
                .filter(|item| item.status == ItemStatus::Inbox)
                .cloned()
                .collect()
        })
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_item(
    title: String,
    context_id: i64,
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_item(title, context_id, project_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_status(
    item_id: i64,
    status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemStatus { item_id, status }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_title(
    item_id: i64,
    title: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemTitle { item_id, title }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_notes(
    item_id: i64,
    notes: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemNotes { item_id, notes }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_item_reminder(
    item_id: i64,
    remind_at: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::AddItemReminder { item_id, remind_at }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_item_reminder(
    item_id: i64,
    reminder_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(
            Event::RemoveItemReminder {
                item_id,
                reminder_id,
            },
            item_id,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_relation(
    from_item_id: i64,
    to_item_id: i64,
    kind: ItemRelationKind,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemRelation, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_item_relation(from_item_id, to_item_id, kind)
}

#[tauri::command(rename_all = "camelCase")]
pub fn link_external_object(
    item_id: i64,
    url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .link_external_object(item_id, url)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_github_issue(
    item_id: i64,
    repository: String,
    title: String,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_github_issue(item_id, repository, title, body)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_external_comment(
    link_id: i64,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .add_external_comment(link_id, body)
}

#[tauri::command(rename_all = "camelCase")]
pub fn refresh_external_object(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalSnapshot, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .refresh_external_object(external_object_id)
}

#[tauri::command]
pub fn poll_external_objects(state: State<'_, Mutex<Runtime>>) -> Result<PollResult, String> {
    Ok(state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .poll_external_objects())
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_attention_policy(
    link_id: i64,
    policy: Option<ExternalChangePolicy>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_attention_policy(link_id, policy)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_watch_until(
    link_id: i64,
    watch_until: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_schedule(
            Event::SetLinkWatchUntil {
                link_id,
                watch_until,
            },
            link_id,
            "Setting watch period",
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_review_at(
    link_id: i64,
    review_at: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_schedule(
            Event::SetLinkReviewAt { link_id, review_at },
            link_id,
            "Setting review date",
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn clear_link_review_at(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_schedule(
            Event::ClearLinkReviewAt { link_id },
            link_id,
            "Clearing review date",
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_context_attention_default(
    context_id: i64,
    object_kind: ExternalObjectKind,
    policy: ExternalChangePolicy,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ContextAttentionDefault, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_context_attention_default(context_id, object_kind, policy)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mark_link_reviewed(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .mark_link_reviewed(link_id)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use tempfile::tempdir;

    use super::*;
    use crate::persistence::SqliteStore;

    #[test]
    fn setup_completion_reuses_the_default_context_and_survives_reopening() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");

        assert!(
            !runtime
                .setup_state()
                .expect("setup state should be readable")
                .completed
        );
        let setup = runtime
            .complete_setup("Personal".into(), ProviderChoice::GitHub)
            .expect("setup should complete");

        assert!(setup.completed);
        assert_eq!(setup.provider, ProviderChoice::GitHub);
        assert_eq!(runtime.state.contexts.len(), 1);

        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(
            reopened
                .setup_state()
                .expect("setup state should survive reopening"),
            setup
        );
        assert_eq!(reopened.state.contexts.len(), 1);
    }

    #[test]
    fn creating_multiple_workspaces_through_the_application_persists_them_for_the_item() {
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

        let first = runtime
            .create_workspace(
                1,
                vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "feature/api".into(),
                    base_branch: "main".into(),
                }],
            )
            .expect("first Workspace should be created");
        let second = runtime
            .create_workspace(
                1,
                vec![WorkspaceRepositoryInput {
                    repository_id: 2,
                    branch: "feature/web".into(),
                    base_branch: "main".into(),
                }],
            )
            .expect("second Workspace should be created");

        assert_eq!((first.id, second.id), (1, 2));
        assert_eq!(runtime.state.workspaces.len(), 2);
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

    #[cfg(unix)]
    #[test]
    fn health_reports_an_unauthenticated_provider_without_hiding_a_healthy_runtime() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let tmux = directory.path().join("tmux");
        let gh = directory.path().join("gh");
        fs::write(&tmux, "#!/bin/sh\nprintf 'tmux 3.4\\n'\n").expect("fake tmux should be written");
        fs::write(
            &gh,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 0; fi\nprintf 'not logged in\\n' >&2\nexit 1\n",
        )
        .expect("fake gh should be written");
        for path in [&tmux, &gh] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("fake dependency should be executable");
        }

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .complete_setup("Personal".into(), ProviderChoice::GitHub)
            .expect("setup should complete");
        runtime
            .store
            .set_executable_path("tmux_executable_path", &tmux)
            .expect("tmux path should persist");
        runtime
            .store
            .set_executable_path("gh_executable_path", &gh)
            .expect("gh path should persist");

        let health = runtime
            .health_status(None)
            .expect("health checks should return independent statuses");

        assert_eq!(health.runtime.state, DependencyState::Available);
        assert_eq!(health.provider.state, DependencyState::Unauthenticated);
        assert!(Path::new(health.runtime.executable_path.as_deref().unwrap()).is_absolute());
        assert_eq!(
            runtime
                .store
                .executable_path("tmux_executable_path")
                .expect("tmux path should remain readable")
                .unwrap()
                .to_string_lossy(),
            health.runtime.executable_path.as_deref().unwrap()
        );
        assert!(health
            .provider
            .action
            .as_deref()
            .unwrap()
            .contains("gh auth login"));
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
            prompt: "private prompt that must not enter the audit history".into(),
            working_directory: "/private/working-directory".into(),
            session_name: "private-session".into(),
            pane_id: "%1".into(),
            started_at: 1,
            state: RunState::Working,
            pane_status: RunPaneStatus::Available,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
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
            prompt: "private prompt".into(),
            working_directory: "/tmp".into(),
            session_name: session,
            pane_id,
            started_at: 1,
            state: RunState::Working,
            pane_status: RunPaneStatus::Available,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
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
            prompt: "Open the missing Pane".into(),
            working_directory: directory.path().to_string_lossy().into_owned(),
            session_name: "missing-session".into(),
            pane_id: "%99".into(),
            started_at: 1,
            state: RunState::Unknown,
            pane_status: RunPaneStatus::Unknown,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
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
                prompt: "Keep the known Run".into(),
                working_directory: directory.path().to_string_lossy().into_owned(),
                session_name: session.clone(),
                pane_id: pane_id.clone(),
                started_at: 1,
                state: RunState::Working,
                pane_status: RunPaneStatus::Unknown,
                workspace_id: None,
                repository_id: None,
                worktree_id: None,
                direct_checkouts: Vec::new(),
            },
            Run {
                id: 2,
                item_id: 1,
                machine_id: 1,
                agent: AgentKind::Codex,
                execution_profile: ExecutionProfile::Review,
                prompt: "Keep the missing Run".into(),
                working_directory: directory.path().to_string_lossy().into_owned(),
                session_name: session.clone(),
                pane_id: "%999".into(),
                started_at: 2,
                state: RunState::Blocked,
                pane_status: RunPaneStatus::Unknown,
                workspace_id: None,
                repository_id: None,
                worktree_id: None,
                direct_checkouts: Vec::new(),
            },
            Run {
                id: 3,
                item_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Investigate,
                prompt: "Keep the missing session Run".into(),
                working_directory: directory.path().to_string_lossy().into_owned(),
                session_name: "gone-session".into(),
                pane_id: "%1".into(),
                started_at: 3,
                state: RunState::Finished,
                pane_status: RunPaneStatus::Unknown,
                workspace_id: None,
                repository_id: None,
                worktree_id: None,
                direct_checkouts: Vec::new(),
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

        let result = runtime
            .create_github_issue(
                1,
                "acme/app".into(),
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

    fn run_git_output(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .expect("Git should start");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
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
