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
    domain::{
        compose_run_prompt as build_run_prompt, decide, external_link_view, home_view,
        search_items, AgentKind, AttachedRepositoryInput, Context, ContextAttentionDefault,
        DomainState, Event, ExecutionProfile, ExternalChangePolicy, ExternalLinkView,
        ExternalObjectInput, ExternalObjectKind, ExternalProvider, ExternalSnapshot, HomeView,
        Item, ItemRelation, ItemRelationKind, ItemStatus, ItemView, Machine, Project,
        ProjectDefaults, Repository, Run, RunPromptSelection, RunState, Workset,
        WorksetRepositoryInput,
    },
    git::GitCli,
    persistence::SqliteStore,
    provider::{classify_url, resolve_gh_executable, GithubCli},
    terminal::{
        capture_pane, list_panes, AgentLaunchContext, PaneSummary, TerminalRuntime,
        TmuxControlPane, TmuxRuntime,
    },
};

pub struct Runtime {
    store: SqliteStore,
    state: DomainState,
    gh_executable_path: Option<PathBuf>,
    pending_workset_removal: Option<WorksetRemovalReport>,
    terminal_connections: HashMap<String, TmuxControlPane>,
    agent_state_directory: PathBuf,
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

    fn start_run(
        &mut self,
        item_id: i64,
        workset_id: i64,
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
        let root = Path::new(&workset.root_directory);
        if !root.is_dir() {
            return Err(format!(
                "Workset root is not a directory: {}",
                root.display()
            ));
        }

        let machine = self.local_machine_for_item(item_id)?;
        self.provision_agent_hooks()?;
        let run_id = self.state.next_run_id;
        let session_name = format!("mission-item-{item_id}-run-{run_id}");
        let state_file = state_file_path(&self.agent_state_directory, run_id);
        let executable = find_executable(agent_executable_name(agent)).ok_or_else(|| {
            format!(
                "{} is not installed on Local Mac",
                agent_display_name(agent)
            )
        })?;
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

        Ok(WorksetRemovalReport {
            workset_id,
            root_directory: workset.root_directory.clone(),
            repositories,
        })
    }

    fn prepare_workset_removal(&mut self, workset_id: i64) -> Result<WorksetRemovalReport, String> {
        let report = self.build_workset_removal_report(workset_id)?;
        self.pending_workset_removal = Some(report.clone());
        Ok(report)
    }

    fn remove_workset(&mut self, workset_id: i64, confirmed: bool) -> Result<(), String> {
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
        let current = self.build_workset_removal_report(workset_id)?;
        if current != pending {
            return Err(
                "The Workset changed after the safety report; review the updated report before removing it"
                    .into(),
            );
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
        fs::remove_dir_all(&staging).map_err(|error| {
            format!(
                "Workset was removed from Mission Manager, but its staged directory could not be deleted at {}: {error}",
                staging.display()
            )
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

    fn commit(&mut self, decision: crate::domain::Decision) -> Result<(), String> {
        self.store
            .apply(&decision.effects)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        Ok(())
    }
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

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
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
pub fn start_run(
    item_id: i64,
    workset_id: i64,
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
            agent,
            execution_profile,
            prompt,
            prompt_selection,
        )
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
) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .remove_workset(workset_id, confirmed)
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
        assert!(runtime.remove_workset(1, false).is_err());
        assert!(
            root.is_dir(),
            "a rejected confirmation must keep the Workset"
        );

        runtime
            .remove_workset(1, true)
            .expect("confirmed removal should delete the Workset");
        assert!(!root.exists());
        assert!(runtime.state.worksets.is_empty());
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
}
