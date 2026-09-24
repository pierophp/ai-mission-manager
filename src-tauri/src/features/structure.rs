//! Structure feature implementation.
//!
//! This module owns the organizational model and its adapters: Contexts,
//! Projects, Repositories, repository locations, Machines, defaults, and
//! structure-level cleanup. The domain planners and reducer stay pure in
//! `domain`; SQLite, Git, filesystem, and Tauri state access stay here.

use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tauri::State;

use crate::{
    app::{current_unix_seconds, MachineSettingsView, Runtime},
    domain::{
        decide, normalize_machine_path, Context, ContextAttentionDefault, Event, ExecutionMode,
        ExternalChangePolicy, ExternalObjectKind, Machine, MachineObservation, MachineTransport,
        Project, ProjectDefaults, Repository, RepositoryLocation,
    },
    git::GitCli,
    terminal::{MachineReadiness, TerminalRuntime},
};

use crate::features::deletion::{
    MachineDeletionPreview, MachineDeletionResult, ParentDeletionPreview, ParentDeletionResult,
    RepositoryDeletionPreview, RepositoryDeletionResult, ResetLocalDataPreview,
    ResetLocalDataResult,
};

pub(crate) fn machine_home_directory(machine: &Machine) -> String {
    match &machine.transport {
        MachineTransport::Local => env::var("HOME").unwrap_or_else(|_| "/".into()),
        MachineTransport::Ssh { .. } => "~".into(),
    }
}

pub(crate) fn resolve_machine_path(path: &str, machine_home: &str) -> PathBuf {
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

fn locked<T>(
    state: State<'_, Mutex<Runtime>>,
    operation: impl FnOnce(&mut Runtime) -> Result<T, String>,
) -> Result<T, String> {
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    operation(&mut runtime)
}

pub(crate) fn list_contexts(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Context>, String> {
    locked(state, |runtime| Ok(runtime.state.contexts.clone()))
}

pub(crate) fn list_projects(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Project>, String> {
    locked(state, |runtime| Ok(runtime.state.projects.clone()))
}

pub(crate) fn list_repositories(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<Repository>, String> {
    locked(state, |runtime| Ok(runtime.state.repositories.clone()))
}

pub(crate) fn list_repository_locations(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<crate::domain::RepositoryLocation>, String> {
    locked(state, |runtime| {
        Ok(runtime.state.repository_locations.clone())
    })
}

pub(crate) fn list_machines(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<MachineSettingsView>, String> {
    locked(state, |runtime| {
        Ok(runtime
            .state
            .machines
            .iter()
            .map(|machine| runtime.machine_settings_view(machine))
            .collect())
    })
}

pub(crate) fn list_context_attention_defaults(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ContextAttentionDefault>, String> {
    locked(state, |runtime| {
        Ok(runtime.state.attention_defaults.clone())
    })
}

pub(crate) fn create_context(
    name: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    locked(state, |runtime| runtime.create_context(name))
}

pub(crate) fn update_context(
    context_id: i64,
    name: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    locked(state, |runtime| runtime.update_context(context_id, name))
}

pub(crate) fn set_context_execution_machine(
    context_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    locked(state, |runtime| {
        runtime.set_context_execution_machine(context_id, machine_id)
    })
}

pub(crate) fn set_context_grill_defaults(
    context_id: i64,
    defaults: crate::domain::GrillConfiguration,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Context, String> {
    locked(state, |runtime| {
        runtime.set_context_grill_defaults(context_id, defaults)
    })
}

pub(crate) fn create_project(
    name: String,
    context_id: i64,
    default_item_status: crate::domain::ItemStatus,
    execution_mode: Option<ExecutionMode>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    locked(state, |runtime| {
        runtime.create_project(
            name,
            context_id,
            ProjectDefaults {
                item_status: default_item_status,
                execution_mode: execution_mode.unwrap_or(ExecutionMode::Worktree),
            },
        )
    })
}

pub(crate) fn update_project(
    project_id: i64,
    name: String,
    default_item_status: crate::domain::ItemStatus,
    execution_mode: Option<ExecutionMode>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    locked(state, |runtime| {
        runtime.update_project(
            project_id,
            name,
            ProjectDefaults {
                item_status: default_item_status,
                execution_mode: execution_mode.unwrap_or(ExecutionMode::Worktree),
            },
        )
    })
}

pub(crate) fn register_repository(
    project_id: i64,
    name: String,
    remote_url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    locked(state, |runtime| {
        runtime.register_repository(project_id, name, remote_url)
    })
}

pub(crate) fn update_repository(
    repository_id: i64,
    name: String,
    remote_url: String,
    base_branch: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    locked(state, |runtime| {
        runtime.update_repository(repository_id, name, remote_url, base_branch)
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn register_repository_at_location(
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
    register_repository_at_location_with_state(
        project_id,
        name,
        remote_url,
        base_branch,
        machine_id,
        checkout_path,
        worktree_root,
        clone_into_destination,
        state.inner(),
    )
    .await
}

pub(crate) fn update_repository_location(
    repository_id: i64,
    previous_machine_id: Option<i64>,
    machine_id: i64,
    checkout_path: String,
    worktree_root: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<crate::domain::RepositoryLocation, String> {
    locked(state, |runtime| {
        runtime.update_repository_location(
            repository_id,
            previous_machine_id,
            machine_id,
            checkout_path,
            worktree_root,
        )
    })
}

pub(crate) fn register_machine(
    context_id: i64,
    name: String,
    socket_name: String,
    transport: MachineTransport,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Machine, String> {
    locked(state, |runtime| {
        runtime.register_machine(context_id, name, socket_name, transport)
    })
}

pub(crate) fn update_machine(
    machine_id: i64,
    name: String,
    socket_name: String,
    transport: MachineTransport,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Machine, String> {
    locked(state, |runtime| {
        runtime.update_machine(machine_id, name, socket_name, transport)
    })
}

pub(crate) async fn check_machine(
    machine_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineSettingsView, String> {
    // The Machine check may provision hooks and run remote tmux commands. Keep
    // that work off the shared Runtime lock and off Tauri's command thread.
    check_machine_with_state(machine_id, state.inner()).await
}

pub(crate) async fn check_machine_with_state(
    machine_id: i64,
    state: &Mutex<Runtime>,
) -> Result<MachineSettingsView, String> {
    let snapshot = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.machine_check_snapshot(machine_id)?
    };
    let observation = tauri::async_runtime::spawn_blocking(move || snapshot.observe())
        .await
        .map_err(|error| format!("Machine check worker failed: {error}"))?;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    runtime.apply_machine_check(observation)
}

struct MachineCheckSnapshot {
    machine: Machine,
    generation: u64,
    terminal_runtime: Arc<dyn TerminalRuntime>,
}

struct MachineCheckObservation {
    machine: Machine,
    generation: u64,
    readiness: MachineReadiness,
}

impl MachineCheckSnapshot {
    fn observe(self) -> MachineCheckObservation {
        let readiness = self.terminal_runtime.check_machine(&self.machine);
        MachineCheckObservation {
            machine: self.machine,
            generation: self.generation,
            readiness,
        }
    }
}

impl Runtime {
    fn machine_check_snapshot(&mut self, machine_id: i64) -> Result<MachineCheckSnapshot, String> {
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {machine_id} does not exist"))?;
        let generation = self
            .machine_check_request_generations
            .entry(machine_id)
            .or_default();
        *generation = generation.wrapping_add(1).max(1);
        Ok(MachineCheckSnapshot {
            machine,
            generation: *generation,
            terminal_runtime: Arc::clone(&self.terminal_runtime),
        })
    }

    fn apply_machine_check(
        &mut self,
        observation: MachineCheckObservation,
    ) -> Result<MachineSettingsView, String> {
        if self
            .machine_check_request_generations
            .get(&observation.machine.id)
            != Some(&observation.generation)
        {
            return Err(format!(
                "Machine {} check result was superseded by a newer check",
                observation.machine.id
            ));
        }
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == observation.machine.id)
            .cloned()
            .ok_or_else(|| format!("Machine {} no longer exists", observation.machine.id))?;
        if machine.context_id != observation.machine.context_id
            || machine.name != observation.machine.name
            || machine.socket_name != observation.machine.socket_name
            || machine.transport != observation.machine.transport
        {
            return Err(format!(
                "Machine {} changed while it was being checked; check it again",
                observation.machine.id
            ));
        }
        let availability = if observation.readiness.reachable == Some(true)
            && observation.readiness.tmux_available == Some(true)
        {
            MachineObservation::Available
        } else {
            MachineObservation::Offline
        };
        self.machine_readiness
            .insert(machine.id, observation.readiness);
        let machine = self.observe_machine(machine.id, availability)?;
        Ok(self.machine_settings_view(&machine))
    }
}

pub(crate) fn prepare_project_deletion(
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionPreview, String> {
    locked(state, |runtime| {
        runtime.prepare_project_deletion(project_id)
    })
}

pub(crate) fn delete_project(
    project_id: i64,
    item_ids: Vec<i64>,
    repository_ids: Vec<i64>,
    workspace_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionResult, String> {
    locked(state, |runtime| {
        runtime.delete_project(
            project_id,
            item_ids,
            repository_ids,
            workspace_ids,
            confirmed,
        )
    })
}

pub(crate) fn prepare_context_deletion(
    context_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionPreview, String> {
    locked(state, |runtime| {
        runtime.prepare_context_deletion(context_id)
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn delete_context(
    context_id: i64,
    project_ids: Vec<i64>,
    item_ids: Vec<i64>,
    repository_ids: Vec<i64>,
    workspace_ids: Vec<i64>,
    machine_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ParentDeletionResult, String> {
    locked(state, |runtime| {
        runtime.delete_context(
            context_id,
            project_ids,
            item_ids,
            repository_ids,
            workspace_ids,
            machine_ids,
            confirmed,
        )
    })
}

pub(crate) fn prepare_reset_local_data(
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataPreview, String> {
    locked(state, |runtime| runtime.prepare_reset_local_data())
}

pub(crate) fn reset_all_local_data(
    confirmation: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ResetLocalDataResult, String> {
    locked(state, |runtime| runtime.reset_all_local_data(confirmation))
}

pub(crate) fn prepare_repository_deletion(
    repository_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionPreview, String> {
    locked(state, |runtime| {
        runtime.prepare_repository_deletion(repository_id)
    })
}

pub(crate) fn delete_repository(
    repository_id: i64,
    workspace_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RepositoryDeletionResult, String> {
    locked(state, |runtime| {
        runtime.delete_repository(repository_id, workspace_ids, confirmed)
    })
}

pub(crate) fn prepare_machine_deletion(
    machine_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineDeletionPreview, String> {
    locked(state, |runtime| {
        runtime.prepare_machine_deletion(machine_id)
    })
}

pub(crate) async fn delete_machine(
    machine_id: i64,
    run_ids: Vec<i64>,
    worktree_ids: Vec<i64>,
    repository_location_repository_ids: Vec<i64>,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<MachineDeletionResult, String> {
    crate::features::deletion::delete_machine_with_state(
        machine_id,
        run_ids,
        worktree_ids,
        repository_location_repository_ids,
        confirmed,
        state.inner(),
    )
    .await
}

pub(crate) fn set_context_attention_default(
    context_id: i64,
    object_kind: ExternalObjectKind,
    policy: ExternalChangePolicy,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ContextAttentionDefault, String> {
    locked(state, |runtime| {
        runtime.set_context_attention_default(context_id, object_kind, policy)
    })
}

#[derive(Clone)]
struct RepositoryRegistrationSnapshot {
    project: Project,
    machine: Machine,
    project_repositories: Vec<Repository>,
    project_locations: Vec<RepositoryLocation>,
    name: String,
    remote_url: Option<String>,
    base_branch: String,
    normalized_checkout_path: String,
    resolved_checkout_path: PathBuf,
    normalized_worktree_root: String,
    clone_into_destination: bool,
}

struct RepositoryRegistrationObservation {
    snapshot: RepositoryRegistrationSnapshot,
    effective_remote: String,
    cloned_remote: Option<String>,
}

impl RepositoryRegistrationSnapshot {
    fn observe(self) -> Result<RepositoryRegistrationObservation, String> {
        let git = GitCli::system();
        let cloned_remote = if self.clone_into_destination {
            let remote_url = self
                .remote_url
                .as_deref()
                .map(str::trim)
                .filter(|remote| !remote.is_empty())
                .ok_or_else(|| "A remote URL is required when cloning a Repository".to_owned())?;
            match git.clone_repository_on_machine(
                &self.machine,
                remote_url,
                &self.resolved_checkout_path,
            ) {
                Ok(_) => Some(remote_url.to_owned()),
                Err(error) => {
                    return Err(format!(
                        "{error}; a partial checkout may remain at {}",
                        self.normalized_checkout_path
                    ))
                }
            }
        } else {
            None
        };
        let effective_remote = if let Some(remote) = &cloned_remote {
            remote.clone()
        } else {
            git.adopt_repository_on_machine(
                &self.machine,
                &self.resolved_checkout_path,
                self.remote_url.as_deref(),
            )
            .map_err(|error| error.to_string())?
            .remote_url
            .ok_or_else(|| "The existing checkout has no Git remote".to_owned())?
        };
        Ok(RepositoryRegistrationObservation {
            snapshot: self,
            effective_remote,
            cloned_remote,
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn register_repository_at_location_with_state(
    project_id: i64,
    name: String,
    remote_url: Option<String>,
    base_branch: String,
    machine_id: i64,
    checkout_path: String,
    worktree_root: Option<String>,
    clone_into_destination: bool,
    state: &Mutex<Runtime>,
) -> Result<Repository, String> {
    let snapshot = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.repository_registration_snapshot(
            project_id,
            name,
            remote_url,
            base_branch,
            machine_id,
            checkout_path,
            worktree_root,
            clone_into_destination,
        )?
    };
    let worker_snapshot = snapshot.clone();
    let observation = tauri::async_runtime::spawn_blocking(move || worker_snapshot.observe())
        .await
        .map_err(|error| format!("Repository registration worker failed: {error}"))?;
    let partial_clone_path = snapshot
        .clone_into_destination
        .then(|| snapshot.normalized_checkout_path.clone());
    let result = {
        let mut runtime = state
            .lock()
            .map_err(|_| match partial_clone_path.as_deref() {
                Some(path) => format!(
                    "Mission Manager state is unavailable after clone; the checkout may remain at {path} without Repository metadata"
                ),
                None => "Mission Manager state is unavailable after Git inspected the checkout".to_owned(),
            })?;
        match observation {
            Err(error) => Err(error),
            Ok(observation) if !runtime.repository_registration_snapshot_is_current(&snapshot) => {
                let error = "The Project, Machine, or Repository registration changed while Git was inspecting the checkout; review it again";
                let result = if observation.cloned_remote.is_some() {
                    Err(format!(
                        "{error}; the cloned checkout remains at {}",
                        observation.snapshot.normalized_checkout_path
                    ))
                } else {
                    Err(error.into())
                };
                result
            }
            Ok(observation) => {
                let existing_repository_id = observation
                    .snapshot
                    .project_repositories
                    .iter()
                    .find(|repository| repository.name == observation.snapshot.name)
                    .map(|repository| repository.id);
                let decision = decide(
                    runtime.state.clone(),
                    Event::RegisterRepositoryAtLocation {
                        project_id,
                        name: observation.snapshot.name.clone(),
                        remote_url: observation.effective_remote,
                        base_branch: observation.snapshot.base_branch.clone(),
                        machine_id,
                        checkout_path: observation.snapshot.normalized_checkout_path.clone(),
                        worktree_root: observation.snapshot.normalized_worktree_root.clone(),
                    },
                );
                let result = decision
                    .map_err(|error| error.to_string())
                    .and_then(|decision| {
                        let repository = decision
                            .state
                            .repositories
                            .iter()
                            .find(|repository| Some(repository.id) == existing_repository_id)
                            .or_else(|| decision.state.repositories.last())
                            .cloned()
                            .ok_or_else(|| {
                                "Repository registration produced no Repository".to_owned()
                            })?;
                        runtime.commit(decision)?;
                        Ok(repository)
                    });
                let result = match (result, observation.cloned_remote) {
                    (Err(error), Some(_)) => Err(format!(
                        "{error}; the cloned checkout remains at {}",
                        observation.snapshot.normalized_checkout_path
                    )),
                    (result, _) => result,
                };
                result
            }
        }
    };
    result
}

impl Runtime {
    pub(crate) fn create_context(&mut self, name: String) -> Result<Context, String> {
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

    pub(crate) fn update_context(
        &mut self,
        context_id: i64,
        name: String,
    ) -> Result<Context, String> {
        let decision = decide(
            self.state.clone(),
            Event::UpdateContext { context_id, name },
        )
        .map_err(|error| error.to_string())?;
        let context = decision
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .cloned()
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        self.commit(decision)?;
        Ok(context)
    }

    pub(crate) fn set_context_execution_machine(
        &mut self,
        context_id: i64,
        machine_id: Option<i64>,
    ) -> Result<Context, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetContextExecutionMachine {
                context_id,
                machine_id,
            },
        )
        .map_err(|error| error.to_string())?;
        let context = decision
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .cloned()
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        self.commit(decision)?;
        Ok(context)
    }

    pub(crate) fn set_context_grill_defaults(
        &mut self,
        context_id: i64,
        defaults: crate::domain::GrillConfiguration,
    ) -> Result<Context, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetContextGrillDefaults {
                context_id,
                defaults,
            },
        )
        .map_err(|error| error.to_string())?;
        let context = decision
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .cloned()
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        self.commit(decision)?;
        Ok(context)
    }

    pub(crate) fn create_project(
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

    pub(crate) fn update_project(
        &mut self,
        project_id: i64,
        name: String,
        defaults: ProjectDefaults,
    ) -> Result<Project, String> {
        let decision = decide(
            self.state.clone(),
            Event::UpdateProject {
                project_id,
                name,
                defaults,
            },
        )
        .map_err(|error| error.to_string())?;
        let project = decision
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
            .ok_or_else(|| format!("Project {project_id} does not exist"))?;
        self.commit(decision)?;
        Ok(project)
    }

    pub(crate) fn register_repository(
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

    pub(crate) fn update_repository(
        &mut self,
        repository_id: i64,
        name: String,
        remote_url: String,
        base_branch: String,
    ) -> Result<Repository, String> {
        let decision = decide(
            self.state.clone(),
            Event::UpdateRepository {
                repository_id,
                name,
                remote_url,
                base_branch,
            },
        )
        .map_err(|error| error.to_string())?;
        let repository = decision
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))?;
        self.commit(decision)?;
        Ok(repository)
    }

    #[allow(clippy::too_many_arguments)]
    fn repository_registration_snapshot(
        &self,
        project_id: i64,
        name: String,
        remote_url: Option<String>,
        base_branch: String,
        machine_id: i64,
        checkout_path: String,
        worktree_root: Option<String>,
        clone_into_destination: bool,
    ) -> Result<RepositoryRegistrationSnapshot, String> {
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
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
        if clone_into_destination
            && remote_url
                .as_deref()
                .map(str::trim)
                .filter(|remote| !remote.is_empty())
                .is_none()
        {
            return Err("A remote URL is required when cloning a Repository".to_owned());
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
        let project_repositories = self
            .state
            .repositories
            .iter()
            .filter(|repository| repository.project_id == project_id)
            .cloned()
            .collect::<Vec<_>>();
        let repository_ids = project_repositories
            .iter()
            .map(|repository| repository.id)
            .collect::<std::collections::HashSet<_>>();
        let project_locations = self
            .state
            .repository_locations
            .iter()
            .filter(|location| repository_ids.contains(&location.repository_id))
            .cloned()
            .collect();
        Ok(RepositoryRegistrationSnapshot {
            project,
            machine,
            project_repositories,
            project_locations,
            name,
            remote_url,
            base_branch,
            normalized_checkout_path,
            resolved_checkout_path,
            normalized_worktree_root,
            clone_into_destination,
        })
    }

    fn repository_registration_snapshot_is_current(
        &self,
        snapshot: &RepositoryRegistrationSnapshot,
    ) -> bool {
        self.state
            .projects
            .iter()
            .any(|project| project == &snapshot.project)
            && self
                .state
                .machines
                .iter()
                .any(|machine| machine == &snapshot.machine)
            && self
                .state
                .repositories
                .iter()
                .filter(|repository| repository.project_id == snapshot.project.id)
                .cloned()
                .collect::<Vec<_>>()
                == snapshot.project_repositories
            && self
                .state
                .repository_locations
                .iter()
                .filter(|location| {
                    snapshot
                        .project_repositories
                        .iter()
                        .any(|repository| repository.id == location.repository_id)
                })
                .cloned()
                .collect::<Vec<_>>()
                == snapshot.project_locations
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub(crate) fn register_repository_at_location(
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
            git.clone_repository_on_machine(&machine, remote_url, &resolved_checkout_path)
                .map_err(|error| error.to_string())?;
            remote_url.to_owned()
        } else {
            let inspection = git
                .adopt_repository_on_machine(
                    &machine,
                    &resolved_checkout_path,
                    remote_url.as_deref(),
                )
                .map_err(|error| error.to_string())?;
            inspection
                .remote_url
                .ok_or_else(|| "The existing checkout has no Git remote".to_owned())?
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

    pub(crate) fn update_repository_location(
        &mut self,
        repository_id: i64,
        previous_machine_id: Option<i64>,
        machine_id: i64,
        checkout_path: String,
        worktree_root: String,
    ) -> Result<crate::domain::RepositoryLocation, String> {
        let decision = decide(
            self.state.clone(),
            Event::UpdateRepositoryLocation {
                repository_id,
                previous_machine_id,
                machine_id,
                checkout_path,
                worktree_root,
            },
        )
        .map_err(|error| error.to_string())?;
        let location = decision
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == repository_id && location.machine_id == machine_id
            })
            .cloned()
            .ok_or_else(|| "Repository location update produced no location".to_owned())?;
        self.commit(decision)?;
        Ok(location)
    }

    pub(crate) fn register_machine(
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

    pub(crate) fn update_machine(
        &mut self,
        machine_id: i64,
        name: String,
        socket_name: String,
        transport: MachineTransport,
    ) -> Result<Machine, String> {
        let decision = decide(
            self.state.clone(),
            Event::UpdateMachine {
                machine_id,
                name,
                socket_name,
                transport,
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
        self.machine_readiness.remove(&machine_id);
        Ok(machine)
    }

    pub(crate) fn observe_machine(
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

    pub(crate) fn set_context_attention_default(
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
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn repository_cleanup_keeps_preview_and_confirmation_as_separate_guards() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");

        runtime
            .register_repository(
                1,
                "service".into(),
                "https://example.com/service.git".into(),
            )
            .expect("Repository should register");

        let missing_preview = runtime
            .delete_repository(1, Vec::new(), true)
            .expect_err("deletion should require a preview");
        assert!(missing_preview.contains("Review the Repository deletion preview"));

        runtime
            .prepare_repository_deletion(1)
            .expect("Repository deletion preview should be available");
        let missing_confirmation = runtime
            .delete_repository(1, Vec::new(), false)
            .expect_err("deletion should require explicit confirmation");
        assert!(missing_confirmation.contains("explicit confirmation"));
        assert!(runtime
            .state
            .repositories
            .iter()
            .any(|repository| repository.id == 1));
    }

    #[test]
    fn updating_machine_invalidates_cached_readiness() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        let context = runtime
            .create_context("Remote operations".into())
            .expect("Context should be created");
        let machine = runtime
            .register_machine(
                context.id,
                "Local machine".into(),
                "default".into(),
                MachineTransport::Local,
            )
            .expect("Machine should register");
        runtime.machine_readiness.insert(
            machine.id,
            crate::terminal::MachineReadiness {
                reachable: Some(true),
                ..Default::default()
            },
        );

        runtime
            .update_machine(
                machine.id,
                "Remote machine".into(),
                "default".into(),
                MachineTransport::Ssh {
                    host: "example.invalid".into(),
                    user: None,
                    port: None,
                    identity_file: None,
                    known_hosts_file: None,
                    strict_host_key_checking: None,
                },
            )
            .expect("Machine should update");

        assert!(!runtime.machine_readiness.contains_key(&machine.id));
    }

    #[test]
    fn machine_check_apply_rejects_an_older_observation() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        let machine = runtime
            .register_machine(
                1,
                "Local machine".into(),
                "default".into(),
                MachineTransport::Local,
            )
            .expect("Machine should register");

        let older_snapshot = runtime
            .machine_check_snapshot(machine.id)
            .expect("older Machine check should snapshot");
        let newer_snapshot = runtime
            .machine_check_snapshot(machine.id)
            .expect("newer Machine check should snapshot");
        assert!(newer_snapshot.generation > older_snapshot.generation);

        runtime
            .apply_machine_check(MachineCheckObservation {
                machine: newer_snapshot.machine.clone(),
                generation: newer_snapshot.generation,
                readiness: MachineReadiness {
                    reachable: Some(true),
                    tmux_available: Some(true),
                    ..Default::default()
                },
            })
            .expect("newer check result should apply");
        let error = runtime
            .apply_machine_check(MachineCheckObservation {
                machine: older_snapshot.machine,
                generation: older_snapshot.generation,
                readiness: MachineReadiness {
                    reachable: Some(false),
                    tmux_available: Some(false),
                    ..Default::default()
                },
            })
            .expect_err("older check result should be rejected");

        assert!(error.contains("superseded by a newer check"));
        let readiness = runtime
            .machine_readiness
            .get(&machine.id)
            .expect("newer readiness should remain cached");
        assert_eq!(readiness.reachable, Some(true));
        assert_eq!(readiness.tmux_available, Some(true));
    }
}
