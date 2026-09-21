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
        home_view, plan_context_deletion, plan_external_object_deletion, plan_item_deletion,
        plan_machine_deletion, plan_project_deletion, plan_repository_deletion,
        plan_reset_local_data, search_items, suggest_untracked_runs, ActivityTabView, AgentKind,
        AgentPaneObservation, AttachedRepositoryInput, AuditAction, AuditEntry, Context,
        ContextAttentionDefault, DomainState, Effect, Event, ExecutionProfile,
        ExternalChangePolicy, ExternalLinkView, ExternalObjectDeletionPlan,
        ExternalObjectDeletionSummary, ExternalObjectInput, ExternalObjectKind, ExternalProvider,
        ExternalSnapshot, HomeView, Item, ItemDeletionPlan, ItemDeletionSummary, ItemRelation,
        ItemRelationKind, ItemStatus, ItemView, Machine, MachineDeletionPlan, MachineObservation,
        MachineTransport, ParentDeletionPlan, Project, ProjectDefaults, Repository,
        RepositoryDeletionPlan, ResetLocalDataPlan, ResetLocalDataSummary, Run, RunPaneStatus,
        RunPromptSelection, RunState, RunSuggestion, Workset, WorksetRepositoryInput,
    },
    git::GitCli,
    persistence::SqliteStore,
    provider::{classify_url, resolve_gh_executable, GithubCli},
    terminal::{
        capture_pane, find_agent_executable, list_agent_panes, list_panes, open_pane_in_terminal,
        probe_local_runtime, probe_machine, terminal_transport, workset_root_exists,
        AgentLaunchContext, ExternalPaneIdentity, PaneSummary, TerminalRuntime, TmuxControlPane,
        TmuxRuntime,
    },
};

pub const RESET_CONFIRMATION_PHRASE: &str = "RESET ALL LOCAL DATA";

pub struct Runtime {
    store: SqliteStore,
    state: DomainState,
    gh_executable_path: Option<PathBuf>,
    pending_workset_removal: Option<WorksetRemovalReport>,
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
            pending_workset_removal: None,
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

    fn create_workset(
        &mut self,
        item_id: i64,
        root_directory: String,
        branch: String,
        repositories: Vec<WorksetRepositoryInput>,
    ) -> Result<Workset, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorkset {
                item_id,
                root_directory,
                branch,
                repositories,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .last()
            .cloned()
            .ok_or_else(|| "Workset creation produced no Workset".to_owned())?;
        let checkout = self.checkout_new_workset(&workset)?;
        if let Err(error) = self.commit(decision) {
            let cleanup_error = checkout.cleanup().err();
            return Err(format_commit_error(error, cleanup_error));
        }
        Ok(workset)
    }

    fn attach_workset(&mut self, item_id: i64, root_directory: String) -> Result<Workset, String> {
        let root = Path::new(&root_directory);
        let repositories = GitCli::system()
            .inspect_workset(root)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|repository| AttachedRepositoryInput {
                name: repository.name,
                remote_url: repository.remote_url,
                current_branch: repository.current_branch,
                is_dirty: repository.is_dirty,
            })
            .collect();
        let decision = decide(
            self.state.clone(),
            Event::AttachWorkset {
                item_id,
                root_directory,
                repositories,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .last()
            .cloned()
            .ok_or_else(|| "Workset attachment produced no Workset".to_owned())?;
        self.commit(decision)?;
        Ok(workset)
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

    #[allow(clippy::too_many_arguments)]
    fn start_run(
        &mut self,
        item_id: i64,
        workset_id: i64,
        machine_id: Option<i64>,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
        prompt: String,
        prompt_selection: RunPromptSelection,
    ) -> Result<Run, String> {
        let workset = self
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id && workset.item_id == item_id)
            .cloned()
            .ok_or_else(|| format!("Workset {workset_id} does not belong to Item {item_id}"))?;
        let machine = self.machine_for_item(item_id, machine_id)?;
        if let Err(error) = probe_machine(&machine) {
            return Err(format!(
                "Could not reach Machine {}. The Run was not started locally: {error}",
                machine.name
            ));
        }
        self.observe_machine(machine.id, MachineObservation::Available)?;
        let root = Path::new(&workset.root_directory);
        workset_root_exists(&machine, root)?;
        if matches!(machine.transport, MachineTransport::Local) {
            self.provision_agent_hooks()?;
        }
        let run_id = self.state.next_run_id;
        let session_name = format!("mission-item-{item_id}-run-{run_id}");
        let state_file = if matches!(machine.transport, MachineTransport::Local) {
            state_file_path(&self.agent_state_directory, run_id)
        } else {
            PathBuf::from(format!("/tmp/ai-mission-manager-run-{run_id}.json"))
        };
        let executable = self
            .agent_executable(&machine, agent)
            .map_err(|error| format!("{}: {error}", agent_display_name(agent)))?;
        let terminal = TmuxRuntime;
        let pane_id = terminal.launch_agent(
            &machine,
            &session_name,
            root,
            &executable,
            &prompt,
            AgentLaunchContext {
                run_id,
                state_file: &state_file,
            },
        )?;
        let decision = decide(
            self.state.clone(),
            Event::StartRun {
                item_id,
                workset_id,
                machine_id: machine.id,
                agent,
                execution_profile,
                prompt,
                working_directory: workset.root_directory,
                session_name: session_name.clone(),
                pane_id,
                started_at: current_unix_seconds(),
                prompt_selection,
            },
        )
        .map_err(|error| {
            let cleanup =
                terminal.kill_session(&machine, &format!("mission-item-{item_id}-run-{run_id}"));
            format_commit_error(error.to_string(), cleanup.err())
        })?;
        let run = decision
            .state
            .runs
            .last()
            .cloned()
            .ok_or_else(|| "Run creation produced no Run".to_owned())?;
        if let Err(error) = self.commit(decision) {
            let cleanup = terminal.kill_session(&machine, &run.session_name);
            return Err(format_commit_error(error, cleanup.err()));
        }
        Ok(run)
    }

    fn list_workset_panes(&mut self, workset_id: i64) -> Result<Vec<PaneTab>, String> {
        self.recover_run_states()?;
        if !self
            .state
            .worksets
            .iter()
            .any(|workset| workset.id == workset_id)
        {
            return Err(format!("Workset {workset_id} does not exist"));
        }

        let mut tabs = Vec::new();
        let mut seen = HashSet::new();
        for run in self
            .state
            .runs
            .iter()
            .filter(|run| run.workset_id == workset_id)
        {
            let machine = self
                .state
                .machines
                .iter()
                .find(|machine| machine.id == run.machine_id)
                .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
            match list_panes(machine, &run.session_name) {
                Ok(panes) => {
                    for pane in panes {
                        if seen.insert((run.session_name.clone(), pane.pane_id.clone())) {
                            tabs.push(PaneTab::from_summary(run, &run.session_name, pane, true));
                        }
                    }
                }
                Err(_) if seen.insert((run.session_name.clone(), run.pane_id.clone())) => {
                    tabs.push(PaneTab::from_summary(
                        run,
                        &run.session_name,
                        PaneSummary {
                            pane_id: run.pane_id.clone(),
                            pane_index: 0,
                            pid: 0,
                            columns: 0,
                            rows: 0,
                            title: String::new(),
                            current_command: String::new(),
                            current_path: run.working_directory.clone(),
                        },
                        false,
                    ));
                }
                _ => {}
            }
        }
        Ok(tabs)
    }

    fn list_run_suggestions(&mut self) -> Result<Vec<RunSuggestion>, String> {
        self.recover_run_states()?;
        let local_machine_item_ids = self
            .state
            .worksets
            .iter()
            .filter(|workset| !workset.archived)
            .map(|workset| workset.item_id)
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
            candidate.workset_id == suggestion.workset_id
                && candidate.item_id == suggestion.item_id
                && candidate.machine_id == suggestion.machine_id
                && candidate.session_name == suggestion.session_name
                && candidate.pane_id == suggestion.pane_id
        })
        .ok_or_else(|| "The suggested agent no longer matches that Workset".to_owned())?;
        let decision = decide(
            self.state.clone(),
            Event::AttachRun {
                item_id: canonical.item_id,
                workset_id: canonical.workset_id,
                machine_id: canonical.machine_id,
                agent: canonical.agent,
                working_directory: canonical.workset_root_directory,
                session_name: canonical.session_name,
                pane_id: canonical.pane_id,
                attached_at: current_unix_seconds(),
            },
        )
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
        workset_id: i64,
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
            .find(|run| run.workset_id == workset_id && run.session_name == session_name)
            .cloned()
            .ok_or_else(|| "The Pane does not belong to a Run in this Workset".to_owned())?;
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
        let panes = match self.list_workset_panes(workset_id) {
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

    fn add_repository_to_workset(
        &mut self,
        workset_id: i64,
        repository_id: i64,
        branch_override: Option<String>,
        base_branch_override: Option<String>,
    ) -> Result<Workset, String> {
        let decision = decide(
            self.state.clone(),
            Event::AddRepositoryToWorkset {
                workset_id,
                repository_id,
                branch_override,
                base_branch_override,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id)
            .cloned()
            .ok_or_else(|| "Workset update produced no Workset".to_owned())?;
        let selected = workset
            .repositories
            .iter()
            .find(|selected| selected.repository_id == repository_id)
            .ok_or_else(|| "Workset update produced no Repository selection".to_owned())?;
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| "Workset update produced no Repository".to_owned())?;
        let destination = match self.checkout_repository(
            &workset,
            &repository,
            selected.branch_override.as_deref(),
            selected.base_branch_override.as_deref(),
        ) {
            Ok(destination) => destination,
            Err(error) => {
                let cleanup_error = CheckoutReceipt {
                    root: Path::new(&workset.root_directory).to_owned(),
                    root_was_created: false,
                    destinations: vec![Path::new(&workset.root_directory).join(&repository.name)],
                }
                .cleanup()
                .err();
                return Err(format_commit_error(error, cleanup_error));
            }
        };
        if let Err(error) = self.commit(decision) {
            let cleanup_error = CheckoutReceipt {
                root: Path::new(&workset.root_directory).to_owned(),
                root_was_created: false,
                destinations: vec![destination],
            }
            .cleanup()
            .err();
            return Err(format_commit_error(error, cleanup_error));
        }
        Ok(workset)
    }

    fn set_workset_archived(&mut self, workset_id: i64, archived: bool) -> Result<Workset, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetWorksetArchived {
                workset_id,
                archived,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id)
            .cloned()
            .ok_or_else(|| "Workset archive update produced no Workset".to_owned())?;
        self.commit(decision)?;
        Ok(workset)
    }

    fn build_workset_removal_report(
        &self,
        workset_id: i64,
    ) -> Result<WorksetRemovalReport, String> {
        let workset = self
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id)
            .ok_or_else(|| format!("Workset {workset_id} does not exist"))?;
        let root = Path::new(&workset.root_directory);
        let inspected = GitCli::system()
            .inspect_workset(root)
            .map_err(|error| error.to_string())?;
        let repositories = workset
            .repositories
            .iter()
            .map(|selected| {
                let repository = self.repository(selected.repository_id)?;
                let repository_state = inspected
                    .iter()
                    .find(|candidate| candidate.name == repository.name)
                    .ok_or_else(|| {
                        format!(
                            "Repository directory is missing from Workset: {}",
                            repository.name
                        )
                    })?;
                Ok(RepositoryRemovalReport {
                    repository_id: repository.id,
                    name: repository.name.clone(),
                    path: root.join(&repository.name).to_string_lossy().into_owned(),
                    current_branch: repository_state.current_branch.clone(),
                    unpushed_commits: repository_state.unpushed_commits.clone(),
                    unpushed_commits_unknown: repository_state.unpushed_commits_unknown,
                    uncommitted_changes: repository_state.uncommitted_changes.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut report = WorksetRemovalReport {
            workset_id,
            root_directory: workset.root_directory.clone(),
            repositories,
            safe: true,
            blockers: Vec::new(),
        };
        report.blockers = workset_safety_blockers(&report);
        if self.state.worksets.iter().any(|candidate| {
            candidate.id != workset_id && candidate.root_directory == workset.root_directory
        }) {
            report.blockers.push(
                "Another Workset references this physical root; separate the roots before deleting."
                    .into(),
            );
        }
        report.safe = report.blockers.is_empty();
        Ok(report)
    }

    fn prepare_workset_removal(&mut self, workset_id: i64) -> Result<WorksetRemovalReport, String> {
        let report = self.build_standalone_workset_removal_report(workset_id)?;
        self.pending_workset_removal = Some(report.clone());
        Ok(report)
    }

    fn build_standalone_workset_removal_report(
        &self,
        workset_id: i64,
    ) -> Result<WorksetRemovalReport, String> {
        let mut report = self.build_workset_removal_report(workset_id)?;
        if self
            .state
            .runs
            .iter()
            .any(|run| run.workset_id == workset_id)
        {
            report
                .blockers
                .push("This Workset has Run history and cannot be removed directly.".into());
            report.safe = false;
        }
        Ok(report)
    }

    fn build_item_deletion_preview(&self, item_id: i64) -> Result<ItemDeletionPreview, String> {
        let plan = plan_item_deletion(&self.state, item_id).map_err(|error| error.to_string())?;
        let mut blockers = plan
            .active_run_ids
            .iter()
            .map(|run_id| format!("Run #{run_id} is active; stop it before deleting this Item."))
            .collect::<Vec<_>>();
        let worksets = plan
            .worksets
            .iter()
            .map(|workset| {
                let preview = self.build_workset_deletion_preview(
                    workset.id,
                    &workset.root_directory,
                    &workset.branch,
                    workset.archived,
                );
                blockers.extend(
                    preview
                        .blockers
                        .iter()
                        .map(|blocker| format!("Workset #{}: {blocker}", workset.id)),
                );
                preview
            })
            .collect();

        Ok(ItemDeletionPreview {
            plan,
            worksets,
            blockers,
        })
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
        let worksets = plan
            .worksets
            .iter()
            .map(|workset| {
                let preview = self.build_workset_deletion_preview(
                    workset.id,
                    &workset.root_directory,
                    &workset.branch,
                    workset.archived,
                );
                preview
            })
            .collect();

        Ok(ParentDeletionPreview {
            plan,
            worksets,
            blockers,
        })
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
        let worksets = plan
            .worksets
            .iter()
            .map(|workset| {
                let preview = self.build_workset_deletion_preview(
                    workset.id,
                    &workset.root_directory,
                    &workset.branch,
                    workset.archived,
                );
                blockers.extend(
                    preview
                        .blockers
                        .iter()
                        .map(|blocker| format!("Workset #{}: {blocker}", workset.id)),
                );
                preview
            })
            .collect();

        Ok(ResetLocalDataPreview {
            plan,
            audit_entry_count: self
                .store
                .audit_entry_count()
                .map_err(|error| error.to_string())?,
            worksets,
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
        delete_workset_directories: bool,
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
                "The local model or a Workset safety report changed after the reset preview; review the updated preview before resetting local data"
                    .into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Reset is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let mut staged = Vec::new();
        if delete_workset_directories {
            for workset in &current.plan.worksets {
                let root = Path::new(&workset.root_directory);
                let staging = match workset_removal_staging_path(root, workset.id) {
                    Ok(staging) => staging,
                    Err(error) => {
                        let restore_error = restore_staged_directories(&staged);
                        return Err(format_commit_error(error, restore_error));
                    }
                };
                if let Err(error) = fs::rename(root, &staging) {
                    let restore_error = restore_staged_directories(&staged);
                    return Err(format_commit_error(
                        format!("Could not stage Workset directory for local-data reset: {error}"),
                        restore_error,
                    ));
                }
                staged.push((root.to_owned(), staging));
            }
        }

        let decision = match decide(self.state.clone(), Event::ResetLocalData) {
            Ok(decision) => decision,
            Err(error) => {
                let restore_error = restore_staged_directories(&staged);
                return Err(format_commit_error(error.to_string(), restore_error));
            }
        };
        if let Err(error) = self.commit(decision) {
            let restore_error = restore_staged_directories(&staged);
            return Err(format_commit_error(error, restore_error));
        }
        self.pending_reset_local_data = None;
        self.pending_workset_removal = None;
        self.pending_item_deletion = None;
        self.pending_external_object_deletion = None;
        self.pending_repository_deletion = None;
        self.pending_machine_deletion = None;
        self.pending_parent_deletion = None;
        self.terminal_connections.clear();

        let mut cleanup_errors = Vec::new();
        let workset_cleanup_failed = if delete_workset_directories {
            let mut failed = false;
            for (_, staging) in staged {
                if let Err(error) = fs::remove_dir_all(&staging) {
                    failed = true;
                    cleanup_errors.push(format!("{}: {error}", staging.display()));
                }
            }
            failed
        } else {
            false
        };
        if self.agent_state_directory.exists() {
            if let Err(error) = fs::remove_dir_all(&self.agent_state_directory) {
                cleanup_errors.push(format!("{}: {error}", self.agent_state_directory.display()));
            }
        }
        let physical_cleanup_warning = if !cleanup_errors.is_empty() {
            Some(format!(
                "Local data was reset, but some local physical data could not be removed: {}",
                cleanup_errors.join(", ")
            ))
        } else if !delete_workset_directories && !current.plan.worksets.is_empty() {
            Some("Local data was reset; Workset directories were left on disk by choice.".into())
        } else {
            None
        };

        Ok(ResetLocalDataResult {
            summary: current.plan.summary,
            audit_entry_count: current.audit_entry_count,
            workset_directories_deleted: delete_workset_directories && !workset_cleanup_failed,
            physical_cleanup_warning,
        })
    }

    fn delete_parent(
        &mut self,
        target: ParentDeletionTarget,
        event: Event,
        confirmed: bool,
        delete_workset_directories: bool,
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
                "The {} or one of its Workset safety reports changed after the preview; review the updated deletion preview before deleting it",
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
        if delete_workset_directories {
            let physical_blockers = current
                .worksets
                .iter()
                .flat_map(|workset| {
                    workset
                        .blockers
                        .iter()
                        .map(move |blocker| format!("Workset #{}: {blocker}", workset.workset_id))
                })
                .collect::<Vec<_>>();
            if !physical_blockers.is_empty() {
                return Err(format!(
                    "{} Workset directory cleanup is blocked:\n{}",
                    target.label(),
                    physical_blockers.join("\n")
                ));
            }
        }

        let mut staged = Vec::new();
        if delete_workset_directories {
            for workset in &current.plan.worksets {
                let root = Path::new(&workset.root_directory);
                let staging = match workset_removal_staging_path(root, workset.id) {
                    Ok(staging) => staging,
                    Err(error) => {
                        let restore_error = restore_staged_directories(&staged);
                        return Err(format_commit_error(error, restore_error));
                    }
                };
                if let Err(error) = fs::rename(root, &staging) {
                    let restore_error = restore_staged_directories(&staged);
                    return Err(format_commit_error(
                        format!(
                            "Could not stage Workset directory for {} deletion: {error}",
                            target.label()
                        ),
                        restore_error,
                    ));
                }
                staged.push((root.to_owned(), staging));
            }
        }

        let decision = match decide(self.state.clone(), event) {
            Ok(decision) => decision,
            Err(error) => {
                let restore_error = restore_staged_directories(&staged);
                return Err(format_commit_error(error.to_string(), restore_error));
            }
        };
        let summary = current.plan.summary();
        if let Err(error) = self.commit(decision) {
            let restore_error = restore_staged_directories(&staged);
            return Err(format_commit_error(error, restore_error));
        }
        self.pending_parent_deletion = None;

        let physical_cleanup_warning = if delete_workset_directories {
            let mut cleanup_errors = Vec::new();
            for (_, staging) in staged {
                if let Err(error) = fs::remove_dir_all(&staging) {
                    cleanup_errors.push(format!("{}: {error}", staging.display()));
                }
            }
            (!cleanup_errors.is_empty()).then(|| {
                format!(
                    "{} records were deleted, but some Workset directories could not be removed: {}",
                    target.label(),
                    cleanup_errors.join(", ")
                )
            })
        } else if current.plan.worksets.is_empty() {
            None
        } else if current.worksets.iter().any(|workset| !workset.safe) {
            Some(format!(
                "{} records were deleted; Workset directories were left on disk because physical cleanup was not safe.",
                target.label()
            ))
        } else {
            Some(format!(
                "{} records were deleted; Workset directories were left on disk by choice.",
                target.label()
            ))
        };

        Ok(ParentDeletionResult {
            summary,
            workset_directories_deleted: delete_workset_directories,
            physical_cleanup_warning,
        })
    }

    fn delete_project(
        &mut self,
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workset_ids: Vec<i64>,
        confirmed: bool,
        delete_workset_directories: bool,
    ) -> Result<ParentDeletionResult, String> {
        self.delete_parent(
            ParentDeletionTarget::Project(project_id),
            Event::DeleteProject {
                project_id,
                item_ids,
                repository_ids,
                workset_ids,
            },
            confirmed,
            delete_workset_directories,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn delete_context(
        &mut self,
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workset_ids: Vec<i64>,
        machine_ids: Vec<i64>,
        confirmed: bool,
        delete_workset_directories: bool,
    ) -> Result<ParentDeletionResult, String> {
        self.delete_parent(
            ParentDeletionTarget::Context(context_id),
            Event::DeleteContext {
                context_id,
                project_ids,
                item_ids,
                repository_ids,
                workset_ids,
                machine_ids,
            },
            confirmed,
            delete_workset_directories,
        )
    }

    fn build_workset_deletion_preview(
        &self,
        workset_id: i64,
        root_directory: &str,
        branch: &str,
        archived: bool,
    ) -> WorksetDeletionPreview {
        match self.build_workset_removal_report(workset_id) {
            Ok(report) => WorksetDeletionPreview {
                workset_id,
                root_directory: root_directory.into(),
                branch: branch.into(),
                archived,
                safe: report.safe,
                blockers: report.blockers.clone(),
                safety_report: Some(report),
            },
            Err(error) => WorksetDeletionPreview {
                workset_id,
                root_directory: root_directory.into(),
                branch: branch.into(),
                archived,
                safe: false,
                blockers: vec![error],
                safety_report: None,
            },
        }
    }

    fn build_repository_deletion_preview(
        &self,
        repository_id: i64,
    ) -> Result<RepositoryDeletionPreview, String> {
        let plan = plan_repository_deletion(&self.state, repository_id)
            .map_err(|error| error.to_string())?;
        let mut blockers = Vec::new();
        let worksets = plan
            .worksets
            .iter()
            .map(|workset| {
                let mut preview = self.build_workset_deletion_preview(
                    workset.id,
                    &workset.root_directory,
                    &workset.branch,
                    workset.archived,
                );
                if self
                    .state
                    .runs
                    .iter()
                    .any(|run| run.workset_id == workset.id)
                {
                    preview.blockers.push(
                        "This Workset has Run history and cannot be removed with the Repository."
                            .into(),
                    );
                    preview.safe = false;
                }
                blockers.extend(
                    preview
                        .blockers
                        .iter()
                        .map(|blocker| format!("Workset #{}: {blocker}", workset.id)),
                );
                preview
            })
            .collect();

        Ok(RepositoryDeletionPreview {
            plan,
            worksets,
            blockers,
        })
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

    fn delete_item(
        &mut self,
        item_id: i64,
        confirmed: bool,
        delete_workset_directories: bool,
    ) -> Result<ItemDeletionResult, String> {
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
                "The Item or a Workset safety report changed after the preview; review the updated deletion preview before deleting it"
                    .into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Item deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let mut staged = Vec::new();
        if delete_workset_directories {
            for workset in &current.plan.worksets {
                let root = Path::new(&workset.root_directory);
                let staging = match workset_removal_staging_path(root, workset.id) {
                    Ok(staging) => staging,
                    Err(error) => {
                        let restore_error = restore_staged_directories(&staged);
                        return Err(format_commit_error(error, restore_error));
                    }
                };
                if let Err(error) = fs::rename(root, &staging) {
                    let restore_error = restore_staged_directories(&staged);
                    return Err(format_commit_error(
                        format!("Could not stage Workset directory for Item deletion: {error}"),
                        restore_error,
                    ));
                }
                staged.push((root.to_owned(), staging));
            }
        }

        let decision = match decide(self.state.clone(), Event::DeleteItem { item_id }) {
            Ok(decision) => decision,
            Err(error) => {
                let restore_error = restore_staged_directories(&staged);
                return Err(format_commit_error(error.to_string(), restore_error));
            }
        };
        let summary = current.plan.summary();
        if let Err(error) = self.commit(decision) {
            let restore_error = restore_staged_directories(&staged);
            return Err(format_commit_error(error, restore_error));
        }
        self.pending_item_deletion = None;

        let physical_cleanup_warning = if delete_workset_directories {
            let mut cleanup_errors = Vec::new();
            for (_, staging) in staged {
                if let Err(error) = fs::remove_dir_all(&staging) {
                    cleanup_errors.push(format!("{}: {error}", staging.display()));
                }
            }
            (!cleanup_errors.is_empty()).then(|| {
                format!(
                    "Item records were deleted, but some Workset directories could not be removed: {}",
                    cleanup_errors.join(", ")
                )
            })
        } else if current.plan.worksets.is_empty() {
            None
        } else {
            Some(
                "Item records were deleted; Workset directories were left on disk by choice."
                    .into(),
            )
        };

        Ok(ItemDeletionResult {
            summary,
            workset_directories_deleted: delete_workset_directories,
            physical_cleanup_warning,
        })
    }

    fn delete_repository(
        &mut self,
        repository_id: i64,
        workset_ids: Vec<i64>,
        confirmed: bool,
        delete_workset_directories: bool,
    ) -> Result<RepositoryDeletionResult, String> {
        if !confirmed {
            return Err(
                "Repository deletion requires explicit confirmation after reviewing its deletion preview"
                    .into(),
            );
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
            return Err(
                "The Repository or a Workset safety report changed after the preview; review the updated deletion preview before deleting it"
                    .into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Repository deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let mut staged = Vec::new();
        if delete_workset_directories {
            for workset in &current.plan.worksets {
                let root = Path::new(&workset.root_directory);
                let staging = match workset_removal_staging_path(root, workset.id) {
                    Ok(staging) => staging,
                    Err(error) => {
                        let restore_error = restore_staged_directories(&staged);
                        return Err(format_commit_error(error, restore_error));
                    }
                };
                if let Err(error) = fs::rename(root, &staging) {
                    let restore_error = restore_staged_directories(&staged);
                    return Err(format_commit_error(
                        format!(
                            "Could not stage Workset directory for Repository deletion: {error}"
                        ),
                        restore_error,
                    ));
                }
                staged.push((root.to_owned(), staging));
            }
        }

        let decision = match decide(
            self.state.clone(),
            Event::DeleteRepository {
                repository_id,
                workset_ids,
            },
        ) {
            Ok(decision) => decision,
            Err(error) => {
                let restore_error = restore_staged_directories(&staged);
                return Err(format_commit_error(error.to_string(), restore_error));
            }
        };
        if let Err(error) = self.commit(decision) {
            let restore_error = restore_staged_directories(&staged);
            return Err(format_commit_error(error, restore_error));
        }
        self.pending_repository_deletion = None;

        let physical_cleanup_warning = if delete_workset_directories {
            let mut cleanup_errors = Vec::new();
            for (_, staging) in staged {
                if let Err(error) = fs::remove_dir_all(&staging) {
                    cleanup_errors.push(format!("{}: {error}", staging.display()));
                }
            }
            (!cleanup_errors.is_empty()).then(|| {
                format!(
                    "Repository and Workset records were deleted, but some Workset directories could not be removed: {}",
                    cleanup_errors.join(", ")
                )
            })
        } else if current.plan.worksets.is_empty() {
            None
        } else {
            Some(
                "Repository and Workset records were deleted; Workset directories were left on disk by choice."
                    .into(),
            )
        };

        Ok(RepositoryDeletionResult {
            repository_id,
            workset_count: current.plan.worksets.len(),
            workset_directories_deleted: delete_workset_directories,
            physical_cleanup_warning,
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

    fn remove_workset(
        &mut self,
        workset_id: i64,
        confirmed: bool,
    ) -> Result<WorksetRemovalResult, String> {
        if !confirmed {
            return Err(
                "Workset removal requires explicit confirmation after reviewing its safety report"
                    .into(),
            );
        }
        let pending = self
            .pending_workset_removal
            .as_ref()
            .filter(|report| report.workset_id == workset_id)
            .cloned()
            .ok_or_else(|| {
                "Review the Workset removal safety report before removing it".to_owned()
            })?;
        let current = self.build_standalone_workset_removal_report(workset_id)?;
        if current != pending {
            return Err(
                "The Workset changed after the safety report; review the updated report before removing it"
                    .into(),
            );
        }
        if !current.safe {
            return Err(format!(
                "Workset removal is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let decision = decide(self.state.clone(), Event::RemoveWorkset { workset_id })
            .map_err(|error| error.to_string())?;
        let root = Path::new(&current.root_directory);
        let staging = workset_removal_staging_path(root, workset_id)?;
        fs::rename(root, &staging)
            .map_err(|error| format!("Could not stage Workset directory for removal: {error}"))?;
        if let Err(error) = self.commit(decision) {
            let restore_error = fs::rename(&staging, root).err().map(|restore_error| {
                format!(
                    "could not restore Workset directory after persistence failed: {restore_error}"
                )
            });
            return Err(format_commit_error(error, restore_error));
        }
        self.pending_workset_removal = None;
        let physical_cleanup_warning = fs::remove_dir_all(&staging).err().map(|error| {
            format!(
                "Workset was removed from Mission Manager, but its staged directory could not be deleted at {}: {error}",
                staging.display()
            )
        });
        Ok(WorksetRemovalResult {
            workset_id,
            workset_directories_deleted: true,
            physical_cleanup_warning,
        })
    }

    fn checkout_new_workset(&self, workset: &Workset) -> Result<CheckoutReceipt, String> {
        let root = Path::new(&workset.root_directory);
        let root_was_created = if root.exists() {
            if !root.is_dir() {
                return Err(format!(
                    "Workset root is not a directory: {}",
                    root.display()
                ));
            }
            if fs::read_dir(root)
                .map_err(|error| format!("Could not inspect Workset root: {error}"))?
                .next()
                .is_some()
            {
                return Err(format!("Workset root is not empty: {}", root.display()));
            }
            false
        } else {
            fs::create_dir_all(root)
                .map_err(|error| format!("Could not create Workset root: {error}"))?;
            true
        };

        let mut destinations = Vec::new();
        for selected in &workset.repositories {
            let repository = self.repository(selected.repository_id)?;
            let destination = root.join(&repository.name);
            if destination.exists() {
                if root_was_created {
                    let _ = fs::remove_dir(root);
                }
                return Err(format!(
                    "Repository checkout destination already exists: {}",
                    destination.display()
                ));
            }
            destinations.push((repository, selected.clone(), destination));
        }

        let mut attempted = Vec::new();
        for (repository, selected, destination) in destinations {
            attempted.push(destination.clone());
            if let Err(error) = self.checkout_repository(
                workset,
                &repository,
                selected.branch_override.as_deref(),
                selected.base_branch_override.as_deref(),
            ) {
                for attempted_destination in attempted.iter().rev() {
                    if attempted_destination.exists() {
                        let _ = fs::remove_dir_all(attempted_destination);
                    }
                }
                if root_was_created && root.exists() {
                    let is_empty = fs::read_dir(root)
                        .map(|mut entries| entries.next().is_none())
                        .unwrap_or(false);
                    if is_empty {
                        let _ = fs::remove_dir(root);
                    }
                }
                return Err(error);
            }
        }
        Ok(CheckoutReceipt {
            root: root.to_owned(),
            root_was_created,
            destinations: attempted,
        })
    }

    fn checkout_repository(
        &self,
        workset: &Workset,
        repository: &Repository,
        branch_override: Option<&str>,
        base_branch_override: Option<&str>,
    ) -> Result<PathBuf, String> {
        let branch = branch_override.unwrap_or(&workset.branch);
        let destination = Path::new(&workset.root_directory).join(&repository.name);
        GitCli::system()
            .checkout_repository(repository, &destination, branch, base_branch_override)
            .map_err(|error| error.to_string())
            .map(|()| destination)
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
            Effect::PersistWorkset { workset, .. } => Some(AuditAction::WorksetCreated {
                workset_id: workset.id,
            }),
            Effect::PersistWorksetUpdate { workset } => {
                let previous = before
                    .worksets
                    .iter()
                    .find(|candidate| candidate.id == workset.id);
                match previous {
                    Some(previous) if previous.archived != workset.archived => {
                        Some(AuditAction::WorksetArchived {
                            workset_id: workset.id,
                            archived: workset.archived,
                        })
                    }
                    Some(previous)
                        if previous.root_directory == workset.root_directory
                            && previous.branch == workset.branch
                            && previous.repositories == workset.repositories =>
                    {
                        None
                    }
                    _ => Some(AuditAction::WorksetUpdated {
                        workset_id: workset.id,
                    }),
                }
            }
            Effect::RemoveWorkset { workset_id } => Some(AuditAction::WorksetRemoved {
                workset_id: *workset_id,
                repository_count: before
                    .worksets
                    .iter()
                    .find(|workset| workset.id == *workset_id)
                    .map(|workset| workset.repositories.len())
                    .unwrap_or_default()
                    .into(),
            }),
            Effect::RemoveRepository { repository_id } => Some(AuditAction::RepositoryDeleted {
                repository_id: *repository_id,
                workset_count: before
                    .worksets
                    .iter()
                    .filter(|workset| {
                        workset
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

fn workset_removal_staging_path(root: &Path, workset_id: i64) -> Result<PathBuf, String> {
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "Workset root has no usable directory name: {}",
                root.display()
            )
        })?;
    let staging = parent.join(format!(".{name}.mission-manager-removing-{workset_id}"));
    if staging.exists() {
        return Err(format!(
            "Workset removal staging path already exists: {}",
            staging.display()
        ));
    }
    Ok(staging)
}

fn workset_safety_blockers(report: &WorksetRemovalReport) -> Vec<String> {
    report
        .repositories
        .iter()
        .flat_map(|repository| {
            let mut blockers = Vec::new();
            if repository.unpushed_commits_unknown {
                blockers.push(format!(
                    "Repository {} has unverified unpushed commits; configure an upstream branch and review again.",
                    repository.name
                ));
            } else if !repository.unpushed_commits.is_empty() {
                blockers.push(format!(
                    "Repository {} has {} unpushed commit{}; push or preserve the work before deleting.",
                    repository.name,
                    repository.unpushed_commits.len(),
                    if repository.unpushed_commits.len() == 1 { "" } else { "s" }
                ));
            }
            if !repository.uncommitted_changes.is_empty() {
                blockers.push(format!(
                    "Repository {} has uncommitted changes; commit or preserve the work before deleting.",
                    repository.name
                ));
            }
            blockers
        })
        .collect()
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
pub struct WorksetRemovalReport {
    pub workset_id: i64,
    pub root_directory: String,
    pub repositories: Vec<RepositoryRemovalReport>,
    pub safe: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksetDeletionPreview {
    pub workset_id: i64,
    pub root_directory: String,
    pub branch: String,
    pub archived: bool,
    pub safe: bool,
    pub blockers: Vec<String>,
    pub safety_report: Option<WorksetRemovalReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionPreview {
    pub plan: ItemDeletionPlan,
    pub worksets: Vec<WorksetDeletionPreview>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionResult {
    pub summary: ItemDeletionSummary,
    pub workset_directories_deleted: bool,
    pub physical_cleanup_warning: Option<String>,
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
    pub worksets: Vec<WorksetDeletionPreview>,
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
    pub worksets: Vec<WorksetDeletionPreview>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataPreview {
    pub plan: ResetLocalDataPlan,
    pub audit_entry_count: usize,
    pub worksets: Vec<WorksetDeletionPreview>,
    pub blockers: Vec<String>,
    pub confirmation_phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataResult {
    pub summary: ResetLocalDataSummary,
    pub audit_entry_count: usize,
    pub workset_directories_deleted: bool,
    pub physical_cleanup_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionResult {
    pub summary: crate::domain::ParentDeletionSummary,
    pub workset_directories_deleted: bool,
    pub physical_cleanup_warning: Option<String>,
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
    pub workset_count: usize,
    pub workset_directories_deleted: bool,
    pub physical_cleanup_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksetRemovalResult {
    pub workset_id: i64,
    pub workset_directories_deleted: bool,
    pub physical_cleanup_warning: Option<String>,
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
#[allow(clippy::too_many_arguments)]
pub fn start_run(
    item_id: i64,
    workset_id: i64,
    machine_id: Option<i64>,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .start_run(
            item_id,
            workset_id,
            machine_id,
            agent,
            execution_profile,
            prompt,
            prompt_selection,
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
pub fn list_workset_panes(
    workset_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<PaneTab>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .list_workset_panes(workset_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn open_terminal(
    workset_id: i64,
    terminal_id: String,
    session_name: String,
    pane_id: String,
    app: AppHandle,
    state: State<'_, Mutex<Runtime>>,
) -> Result<TerminalAttachment, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .open_terminal(&app, workset_id, terminal_id, session_name, pane_id)
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
    workset_ids: Vec<i64>,
    confirmed: bool,
    delete_workset_directories: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_project(
            project_id,
            item_ids,
            repository_ids,
            workset_ids,
            confirmed,
            delete_workset_directories,
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
    workset_ids: Vec<i64>,
    machine_ids: Vec<i64>,
    confirmed: bool,
    delete_workset_directories: bool,
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
            workset_ids,
            machine_ids,
            confirmed,
            delete_workset_directories,
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
    delete_workset_directories: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .reset_all_local_data(confirmation, delete_workset_directories)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_workset(
    item_id: i64,
    root_directory: String,
    branch: String,
    repositories: Vec<WorksetRepositoryInput>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_workset(item_id, root_directory, branch, repositories)
}

#[tauri::command(rename_all = "camelCase")]
pub fn attach_workset(
    item_id: i64,
    root_directory: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .attach_workset(item_id, root_directory)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_repository_to_workset(
    workset_id: i64,
    repository_id: i64,
    branch_override: Option<String>,
    base_branch_override: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .add_repository_to_workset(
            workset_id,
            repository_id,
            branch_override,
            base_branch_override,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_workset_archived(
    workset_id: i64,
    archived: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_workset_archived(workset_id, archived)
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
    workset_ids: Vec<i64>,
    confirmed: bool,
    delete_workset_directories: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_repository(
            repository_id,
            workset_ids,
            confirmed,
            delete_workset_directories,
        )
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
pub fn prepare_workset_removal(
    workset_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorksetRemovalReport, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_workset_removal(workset_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_workset(
    workset_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorksetRemovalResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .remove_workset(workset_id, confirmed)
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
    delete_workset_directories: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionResult, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .delete_item(item_id, confirmed, delete_workset_directories)
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
            .reset_all_local_data("RESET".into(), true)
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
            .reset_all_local_data(RESET_CONFIRMATION_PHRASE.into(), true)
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
    fn reset_staging_failure_leaves_the_working_model_and_roots_intact() {
        let directory = tempdir().expect("temporary repository directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "safe\n").expect("seed file should be written");
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
            .register_repository(1, "service".into(), origin_url)
            .expect("Repository should register");
        runtime
            .create_item("Reset the safe Workset".into(), 1, 1)
            .expect("Item should be created");
        let root = directory.path().join("workset-root");
        runtime
            .create_workset(
                1,
                root.to_string_lossy().into_owned(),
                "feature/reset-safe".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("Workset should be created");
        run_git(
            &root.join("service"),
            &["branch", "--set-upstream-to=origin/main"],
        );

        let preview = runtime
            .prepare_reset_local_data()
            .expect("reset preview should be available");
        assert!(preview.blockers.is_empty());
        let blocking_staging_path = directory
            .path()
            .join(".workset-root.mission-manager-removing-1");
        fs::create_dir(&blocking_staging_path).expect("the staging blocker should be created");
        let staging_error = runtime
            .reset_all_local_data(RESET_CONFIRMATION_PHRASE.into(), true)
            .expect_err("a staging failure should abort before logical reset");
        assert!(staging_error.contains("staging path already exists"));
        assert!(root.is_dir());
        assert_eq!(runtime.state.items.len(), 1);
        assert_eq!(runtime.state.worksets.len(), 1);

        fs::remove_dir_all(&blocking_staging_path).expect("the staging blocker should be removed");
        runtime
            .prepare_reset_local_data()
            .expect("fresh reset preview should be available");
        let result = runtime
            .reset_all_local_data(RESET_CONFIRMATION_PHRASE.into(), true)
            .expect("reset should remove safe Workset roots");
        assert!(result.workset_directories_deleted);
        assert!(result.physical_cleanup_warning.is_none());
        assert!(!root.exists());
        assert!(runtime.state.items.is_empty());
        assert!(runtime.state.worksets.is_empty());
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
            workset_id: 1,
            machine_id: 1,
            agent: AgentKind::Claude,
            execution_profile: ExecutionProfile::Implement,
            prompt: "private prompt that must not enter the audit history".into(),
            working_directory: "/private/workset".into(),
            session_name: "private-session".into(),
            pane_id: "%1".into(),
            started_at: 1,
            state: RunState::Working,
            pane_status: RunPaneStatus::Available,
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
        assert!(!serialized.contains("/private/workset"));
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
            workset_id: 1,
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
            workset_id: 1,
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
                workset_id: 1,
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
            },
            Run {
                id: 2,
                item_id: 1,
                workset_id: 1,
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
            },
            Run {
                id: 3,
                item_id: 1,
                workset_id: 1,
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
    fn a_manual_agent_pane_is_suggested_and_attaches_only_after_approval() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let root = directory.path().join("manual-workset");
        fs::create_dir_all(&root).expect("the Workset root should exist");
        let agent = directory.path().join("claude");
        fs::write(&agent, "#!/bin/sh\nwhile true; do sleep 1; done\n")
            .expect("the fake agent should be written");
        fs::set_permissions(&agent, fs::Permissions::from_mode(0o755))
            .expect("the fake agent should be executable");

        let socket = format!("mission-manager-suggestion-{}", std::process::id());
        let session = format!("manual-agent-{}", std::process::id());
        let root_string = root.to_string_lossy().into_owned();
        let agent_string = agent.to_string_lossy().into_owned();
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
            &root_string,
            &agent_string,
        ]);
        run_tmux(&[
            "-f",
            "/dev/null",
            "-L",
            &socket,
            "select-pane",
            "-t",
            &session,
            "-T",
            "claude",
        ]);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        let item = decide(
            runtime.state.clone(),
            Event::CreateItem {
                title: "Attach the manual agent".into(),
                context_id: 1,
                project_id: 1,
            },
        )
        .expect("the Item should be created");
        runtime
            .commit(item.clone())
            .expect("the Item should persist");
        let workset = decide(
            item.state,
            Event::AttachWorkset {
                item_id: 1,
                root_directory: root_string.clone(),
                repositories: vec![AttachedRepositoryInput {
                    name: "service".into(),
                    remote_url: "https://example.com/service.git".into(),
                    current_branch: "main".into(),
                    is_dirty: false,
                }],
            },
        )
        .expect("the Workset should attach");
        runtime
            .commit(workset.clone())
            .expect("the Workset should persist");
        let machine = decide(
            workset.state,
            Event::RegisterMachine {
                context_id: 1,
                name: "Local Mac".into(),
                socket_name: socket.clone(),
                transport: MachineTransport::Local,
            },
        )
        .expect("the Machine should register");
        runtime.commit(machine).expect("the Machine should persist");

        let suggestions = runtime
            .list_run_suggestions()
            .expect("manual agent detection should succeed");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].workset_id, 1);
        assert_eq!(suggestions[0].item_identifier, "MC-1");
        assert_eq!(suggestions[0].agent, AgentKind::Claude);
        assert!(runtime.state.runs.is_empty());

        let run = runtime
            .attach_run(suggestions[0].clone())
            .expect("the approved suggestion should attach");
        assert_eq!(run.item_id, 1);
        assert_eq!(run.state, RunState::Unknown);
        assert_eq!(runtime.state.runs.len(), 1);
        assert!(runtime
            .list_run_suggestions()
            .expect("the attached Pane should no longer be suggested")
            .is_empty());

        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.runs.len(), 1);
        assert_eq!(reopened.state.runs[0].workset_id, 1);
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
    fn creating_a_workset_checks_out_a_repository_and_persists_the_execution_root() {
        let directory = tempdir().expect("temporary app directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "service-a\n").expect("seed file should be written");
        run_git(&seed, &["add", "README.md"]);
        run_git(&seed, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        let origin_url = origin.to_string_lossy().into_owned();
        run_git(&seed, &["remote", "add", "origin", &origin_url]);
        run_git(&seed, &["push", "origin", "main"]);

        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .register_repository(1, "service-a".into(), origin.to_string_lossy().into_owned())
            .expect("Repository should register");
        runtime
            .create_item("Implement the platform change".into(), 1, 1)
            .expect("Item should be created");

        let root = directory.path().join("workset-root");
        let workset = runtime
            .create_workset(
                1,
                root.to_string_lossy().into_owned(),
                "feature/platform-change".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("Workset should be created");

        assert_eq!(workset.branch, "feature/platform-change");
        assert!(root.join("service-a/README.md").is_file());
        assert_eq!(
            run_git_output(&root.join("service-a"), &["branch", "--show-current"]),
            "feature/platform-change"
        );
        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.worksets, vec![workset]);
    }

    #[test]
    fn attaching_a_workset_does_not_change_its_branch_configuration_or_files() {
        let directory = tempdir().expect("temporary app directory should exist");
        let root = directory.path().join("workset-root");
        fs::create_dir(&root).expect("Workset root should exist");
        let repository = root.join("service-a");
        run_git(&root, &["init", "--initial-branch=main", "service-a"]);
        run_git(&repository, &["config", "user.email", "test@example.com"]);
        run_git(&repository, &["config", "user.name", "Test User"]);
        fs::write(repository.join("README.md"), "service-a\n")
            .expect("repository file should be written");
        run_git(&repository, &["add", "README.md"]);
        run_git(&repository, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        let origin_url = origin.to_string_lossy().into_owned();
        run_git(&repository, &["remote", "add", "origin", &origin_url]);
        run_git(&repository, &["push", "--set-upstream", "origin", "main"]);
        fs::write(repository.join("notes.txt"), "keep this work\n")
            .expect("uncommitted file should be written");

        let branch_before = run_git_output(&repository, &["branch", "--show-current"]);
        let config_before = run_git_output(&repository, &["config", "--local", "--list"]);
        let status_before = run_git_output(&repository, &["status", "--porcelain"]);
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Adopt the platform change".into(), 1, 1)
            .expect("Item should be created");

        let workset = runtime
            .attach_workset(1, root.to_string_lossy().into_owned())
            .expect("Workset should attach");

        assert_eq!(workset.branch, "main");
        assert_eq!(workset.repositories[0].current_branch, "main");
        assert!(workset.repositories[0].is_dirty);
        assert_eq!(
            run_git_output(&repository, &["branch", "--show-current"]),
            branch_before
        );
        assert_eq!(
            run_git_output(&repository, &["config", "--local", "--list"]),
            config_before
        );
        assert_eq!(
            run_git_output(&repository, &["status", "--porcelain"]),
            status_before
        );
        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.worksets, vec![workset]);
        assert_eq!(reopened.state.repositories[0].remote_url, origin_url);

        runtime
            .set_workset_archived(1, true)
            .expect("Workset should be archivable");
        assert!(root.is_dir(), "archiving must leave the Workset on disk");
        assert!(runtime.state.worksets[0].archived);

        let report = runtime
            .prepare_workset_removal(1)
            .expect("removal safety report should be available");
        assert_eq!(report.repositories.len(), 1);
        assert!(!report.repositories[0].uncommitted_changes.is_empty());
        assert!(!report.safe);
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("uncommitted changes")));
        assert!(runtime.remove_workset(1, true).is_err());
        assert_eq!(runtime.state.worksets.len(), 1);
        assert!(runtime.remove_workset(1, false).is_err());
        assert!(
            root.is_dir(),
            "a rejected confirmation must keep the Workset"
        );

        fs::remove_file(repository.join("notes.txt")).expect("uncommitted file should be removed");
        let safe_report = runtime
            .prepare_workset_removal(1)
            .expect("a safe removal report should be available");
        assert!(safe_report.safe);
        let result = runtime
            .remove_workset(1, true)
            .expect("confirmed removal should delete the Workset");
        assert!(result.physical_cleanup_warning.is_none());
        assert!(!root.exists());
        assert!(runtime.state.worksets.is_empty());
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
        assert!(runtime.delete_item(1, true, false).is_err());
        assert_eq!(runtime.state.items.len(), 1);

        runtime
            .prepare_item_deletion(1)
            .expect("a fresh deletion preview should be available");
        let result = runtime
            .delete_item(1, true, false)
            .expect("the confirmed Item deletion should succeed");
        assert_eq!(result.summary.item_id, 1);
        assert!(!result.workset_directories_deleted);
        assert!(result.physical_cleanup_warning.is_none());
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
            .delete_project(3, vec![1], Vec::new(), Vec::new(), true, false)
            .is_err());
        assert!(runtime.state.projects.iter().any(|project| project.id == 3));

        runtime
            .prepare_project_deletion(3)
            .expect("fresh Project preview should be available");
        let result = runtime
            .delete_project(3, vec![1], Vec::new(), Vec::new(), true, false)
            .expect("Project deletion should succeed");
        assert_eq!(result.summary.item_count, 1);
        assert!(runtime.state.items.is_empty());
        assert!(runtime.state.projects.iter().all(|project| project.id != 3));
        assert!(runtime.state.contexts.iter().any(|context| context.id == 2));

        runtime
            .prepare_project_deletion(2)
            .expect("the remaining Project preview should be available");
        runtime
            .delete_project(2, Vec::new(), Vec::new(), Vec::new(), true, false)
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
                false,
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
                false,
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
    fn project_deletion_can_keep_an_unsafe_workset_directory() {
        let directory = tempdir().expect("temporary app directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "preserve this work\n")
            .expect("seed file should be written");
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
            .create_project(
                "Billing".into(),
                1,
                ProjectDefaults {
                    item_status: ItemStatus::Inbox,
                },
            )
            .expect("Project should be created");
        runtime
            .register_repository(2, "service".into(), origin_url)
            .expect("Repository should register");
        runtime
            .create_item("Delete the Project records".into(), 1, 2)
            .expect("Item should be created");
        let root = directory.path().join("workset-root");
        runtime
            .create_workset(
                1,
                root.to_string_lossy().into_owned(),
                "feature/delete-project".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("Workset should be created");

        let preview = runtime
            .prepare_project_deletion(2)
            .expect("Project deletion preview should be available");
        assert!(preview
            .worksets
            .iter()
            .flat_map(|workset| &workset.blockers)
            .any(|blocker| blocker.contains("unverified unpushed commits")));
        assert!(runtime
            .delete_project(2, vec![1], vec![1], vec![1], true, true)
            .is_err());

        let result = runtime
            .delete_project(2, vec![1], vec![1], vec![1], true, false)
            .expect("Project records should be deletable while keeping the directory");
        assert!(!result.workset_directories_deleted);
        assert!(result
            .physical_cleanup_warning
            .as_deref()
            .is_some_and(|warning| warning.contains("physical cleanup was not safe")));
        assert!(root.exists());
        assert!(runtime.state.projects.iter().all(|project| project.id != 2));
        assert!(runtime.state.items.is_empty());
    }

    #[test]
    fn machine_deletion_requires_finished_runs_and_a_fresh_preview() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Delete the old Machine".into(), 1, 1)
            .expect("Item should be created");
        let workset = decide(
            runtime.state.clone(),
            Event::AttachWorkset {
                item_id: 1,
                root_directory: "/tmp/machine-app-deletion".into(),
                repositories: vec![AttachedRepositoryInput {
                    name: "service".into(),
                    remote_url: "https://example.com/service.git".into(),
                    current_branch: "main".into(),
                    is_dirty: false,
                }],
            },
        )
        .expect("Workset should attach");
        runtime.commit(workset).expect("Workset should persist");
        let machine = decide(
            runtime.state.clone(),
            Event::RegisterMachine {
                context_id: 1,
                name: "Old Machine".into(),
                socket_name: "old-machine".into(),
                transport: MachineTransport::Local,
            },
        )
        .expect("Machine should register");
        runtime.commit(machine).expect("Machine should persist");
        let run = decide(
            runtime.state.clone(),
            Event::AttachRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Codex,
                working_directory: "/tmp/machine-app-deletion".into(),
                session_name: "old-machine-run".into(),
                pane_id: "%1".into(),
                attached_at: 1,
            },
        )
        .expect("Run should attach");
        runtime.commit(run).expect("Run should persist");

        let blocked_preview = runtime
            .prepare_machine_deletion(1)
            .expect("Machine deletion preview should be available");
        assert_eq!(blocked_preview.plan.runs.len(), 1);
        assert_eq!(blocked_preview.plan.active_run_ids, vec![1]);
        assert!(blocked_preview
            .blockers
            .iter()
            .any(|blocker| blocker.contains("stop it before deleting the Machine")));
        assert!(runtime.delete_machine(1, vec![1], true).is_err());

        let finished = decide(
            runtime.state.clone(),
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("Run should finish");
        runtime
            .commit(finished)
            .expect("finished state should persist");
        assert!(runtime.delete_machine(1, vec![1], true).is_err());

        let fresh_preview = runtime
            .prepare_machine_deletion(1)
            .expect("fresh Machine deletion preview should be available");
        assert!(fresh_preview.blockers.is_empty());
        let result = runtime
            .delete_machine(1, vec![1], true)
            .expect("Machine deletion should succeed after the Run finishes");
        assert_eq!(result.run_count, 1);
        assert!(runtime.state.machines.is_empty());
        assert!(runtime.state.runs.is_empty());
        let history = runtime
            .list_audit_history()
            .expect("audit history should load");
        assert!(history
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::RunDeleted { run_id: 1 })));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::MachineDeleted { machine_id: 1, .. }
        )));
    }

    #[test]
    fn confirmed_item_deletion_stages_and_removes_safe_workset_directories() {
        let directory = tempdir().expect("temporary app directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "safe\n").expect("seed file should be written");
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
            .register_repository(1, "service".into(), origin_url)
            .expect("Repository should register");
        runtime
            .create_item("Delete the safe Workset".into(), 1, 1)
            .expect("Item should be created");
        let root = directory.path().join("workset-root");
        runtime
            .create_workset(
                1,
                root.to_string_lossy().into_owned(),
                "feature/delete-safe".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("Workset should be created");
        run_git(
            &root.join("service"),
            &["branch", "--set-upstream-to=origin/main"],
        );

        let preview = runtime
            .prepare_item_deletion(1)
            .expect("deletion preview should be available");
        assert!(preview.blockers.is_empty());
        let result = runtime
            .delete_item(1, true, true)
            .expect("safe Item deletion should succeed");
        assert!(result.workset_directories_deleted);
        assert!(!root.exists());
        assert!(runtime.state.items.is_empty());
    }

    #[test]
    fn repository_deletion_stages_all_safe_workset_roots_and_audits_each_record() {
        let directory = tempdir().expect("temporary repository directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "safe\n").expect("seed file should be written");
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
            .register_repository(1, "service".into(), origin_url)
            .expect("Repository should register");
        runtime
            .create_item("Delete the Repository and its Worksets".into(), 1, 1)
            .expect("Item should be created");
        let first_root = directory.path().join("first-workset");
        let second_root = directory.path().join("second-workset");
        runtime
            .create_workset(
                1,
                first_root.to_string_lossy().into_owned(),
                "feature/first".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("first Workset should be created");
        runtime
            .create_workset(
                1,
                second_root.to_string_lossy().into_owned(),
                "feature/second".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("second Workset should be created");
        for root in [&first_root, &second_root] {
            run_git(
                &root.join("service"),
                &["branch", "--set-upstream-to=origin/main"],
            );
        }
        runtime
            .set_workset_archived(2, true)
            .expect("the second Workset should be archived");

        let preview = runtime
            .prepare_repository_deletion(1)
            .expect("Repository deletion preview should be available");
        assert_eq!(
            preview
                .plan
                .worksets
                .iter()
                .map(|workset| workset.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(preview.blockers.is_empty());
        assert!(preview.worksets.iter().all(|workset| workset.safe));

        let blocking_staging_path = directory
            .path()
            .join(".second-workset.mission-manager-removing-2");
        fs::create_dir(&blocking_staging_path).expect("the staging-path blocker should be created");
        let staging_error = runtime
            .delete_repository(1, vec![1, 2], true, true)
            .expect_err("a staging failure should abort before logical deletion");
        assert!(staging_error.contains("staging path already exists"));
        assert!(first_root.is_dir());
        assert!(second_root.is_dir());
        assert_eq!(runtime.state.repositories.len(), 1);
        assert_eq!(runtime.state.worksets.len(), 2);
        fs::remove_dir_all(&blocking_staging_path)
            .expect("the staging-path blocker should be removed");

        let result = runtime
            .delete_repository(1, vec![1, 2], true, true)
            .expect("Repository deletion should remove all explicitly included Worksets");
        assert_eq!(result.workset_count, 2);
        assert!(result.workset_directories_deleted);
        assert!(result.physical_cleanup_warning.is_none());
        assert!(!first_root.exists());
        assert!(!second_root.exists());
        assert!(runtime.state.repositories.is_empty());
        assert!(runtime.state.worksets.is_empty());
        assert_eq!(runtime.state.items.len(), 1);
        let history = runtime
            .list_audit_history()
            .expect("audit history should be available");
        assert_eq!(
            history
                .iter()
                .filter(|entry| matches!(entry.action, AuditAction::WorksetRemoved { .. }))
                .count(),
            2
        );
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::RepositoryDeleted {
                repository_id: 1,
                ..
            }
        )));
    }

    #[test]
    fn reopening_recovers_a_run_state_written_while_the_connection_was_down() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut store = SqliteStore::open(&database).expect("database should open");

        let item = decide(
            store.load_state().expect("state should load"),
            Event::CreateItem {
                title: "Recover the blocked Run".into(),
                context_id: 1,
                project_id: 1,
            },
        )
        .expect("Item should be created");
        store.apply(&item.effects).expect("Item should persist");
        let repository = decide(
            item.state,
            Event::RegisterRepository {
                project_id: 1,
                name: "service".into(),
                remote_url: "https://example.com/service.git".into(),
            },
        )
        .expect("Repository should register");
        store
            .apply(&repository.effects)
            .expect("Repository should persist");
        let workset = decide(
            repository.state,
            Event::CreateWorkset {
                item_id: 1,
                root_directory: "/tmp/recovery-workset".into(),
                branch: "main".into(),
                repositories: vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: None,
                }],
            },
        )
        .expect("Workset should be created");
        store
            .apply(&workset.effects)
            .expect("Workset should persist");
        let machine = decide(
            workset.state,
            Event::RegisterMachine {
                context_id: 1,
                name: "Local Mac".into(),
                socket_name: "mission-manager".into(),
                transport: MachineTransport::Local,
            },
        )
        .expect("Machine should register");
        store
            .apply(&machine.effects)
            .expect("Machine should persist");
        let run = decide(
            machine.state,
            Event::StartRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Implement,
                prompt: "Do the work".into(),
                working_directory: "/tmp/recovery-workset".into(),
                session_name: "mission-item-1-run-1".into(),
                pane_id: "%1".into(),
                started_at: 123,
                prompt_selection: RunPromptSelection {
                    include_objective: true,
                    include_notes: false,
                    external_object_ids: Vec::new(),
                },
            },
        )
        .expect("Run should start");
        store.apply(&run.effects).expect("Run should persist");
        drop(store);

        let state_file = state_file_path(&directory.path().join("agent-state"), 1);
        fs::create_dir_all(state_file.parent().expect("state directory should exist"))
            .expect("state directory should be created");
        fs::write(
            &state_file,
            serde_json::json!({
                "agent": "claude",
                "runId": "1",
                "state": "blocked",
                "updatedAt": "2026-09-19T12:34:56Z"
            })
            .to_string(),
        )
        .expect("hook state should be written");

        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.runs[0].state, RunState::Blocked);
        assert_eq!(
            home_view(&reopened.state, None, "2026-09-19T13:00").attention_entries[0].kind,
            crate::domain::AttentionEntryKind::BlockedRun
        );
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
