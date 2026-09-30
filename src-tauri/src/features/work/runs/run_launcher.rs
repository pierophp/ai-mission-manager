use super::*;

/// A complete launch operation. Checkout-specific approval data travels in
/// `strategy`, while the command itself has one stable request argument.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunLaunchRequest {
    pub item_id: i64,
    pub workspace_id: i64,
    pub strategy: RunLaunchStrategy,
    #[serde(default)]
    pub queue_attachment: Option<ImplementationQueueEntryAttachment>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum RunLaunchStrategy {
    Direct {
        machine_id: Option<i64>,
        primary_repository_id: i64,
        agent: AgentKind,
        configuration: Option<GrillConfiguration>,
        implementation_queue: Option<crate::domain::ImplementationQueueStart>,
        execution_profile: ExecutionProfile,
        workflow: crate::domain::Workflow,
        prompt: String,
        prompt_selection: RunPromptSelection,
        expected_checkouts: Vec<RunCheckout>,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    Grill {
        machine_id: Option<i64>,
        primary_repository_id: i64,
        configuration: GrillConfiguration,
        language: GrillLanguage,
        prompt: String,
        expected_checkouts: Vec<RunCheckout>,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    Worktree {
        worktree_id: i64,
        agent: AgentKind,
        configuration: Option<GrillConfiguration>,
        execution_profile: ExecutionProfile,
        workflow: crate::domain::Workflow,
        prompt: String,
        prompt_selection: RunPromptSelection,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImplementationQueueEntryAttachment {
    pub queue_id: i64,
    pub position: i64,
}

impl RunLaunchRequest {
    fn into_input(self) -> RunLaunchInput {
        match self.strategy {
            RunLaunchStrategy::Direct {
                machine_id,
                primary_repository_id,
                agent,
                configuration,
                implementation_queue,
                execution_profile,
                workflow,
                prompt,
                prompt_selection,
                expected_checkouts,
                allow_dirty,
                allow_shared_checkouts,
            } => RunLaunchInput::Direct {
                item_id: self.item_id,
                workspace_id: self.workspace_id,
                machine_id,
                primary_repository_id,
                agent,
                configuration,
                implementation_queue,
                execution_profile,
                workflow,
                prompt,
                prompt_selection,
                expected_checkouts,
                allow_dirty,
                allow_shared_checkouts,
            },
            RunLaunchStrategy::Grill {
                machine_id,
                primary_repository_id,
                configuration,
                language,
                prompt,
                expected_checkouts,
                allow_dirty,
                allow_shared_checkouts,
            } => RunLaunchInput::Grill {
                item_id: self.item_id,
                workspace_id: self.workspace_id,
                machine_id,
                primary_repository_id,
                configuration,
                prompt: language.enforce_prompt(&prompt),
                expected_checkouts,
                allow_dirty,
                allow_shared_checkouts,
            },
            RunLaunchStrategy::Worktree {
                worktree_id,
                agent,
                configuration,
                execution_profile,
                workflow,
                prompt,
                prompt_selection,
            } => RunLaunchInput::Worktree {
                item_id: self.item_id,
                workspace_id: self.workspace_id,
                worktree_id,
                agent,
                configuration,
                execution_profile,
                workflow,
                prompt,
                prompt_selection,
            },
        }
    }
}

#[derive(Clone)]
enum RunLaunchInput {
    Direct {
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
        primary_repository_id: i64,
        agent: AgentKind,
        configuration: Option<GrillConfiguration>,
        implementation_queue: Option<crate::domain::ImplementationQueueStart>,
        execution_profile: ExecutionProfile,
        workflow: crate::domain::Workflow,
        prompt: String,
        prompt_selection: RunPromptSelection,
        expected_checkouts: Vec<RunCheckout>,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    Grill {
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
        primary_repository_id: i64,
        configuration: GrillConfiguration,
        prompt: String,
        expected_checkouts: Vec<RunCheckout>,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    Worktree {
        item_id: i64,
        workspace_id: i64,
        worktree_id: i64,
        agent: AgentKind,
        configuration: Option<GrillConfiguration>,
        execution_profile: ExecutionProfile,
        workflow: crate::domain::Workflow,
        prompt: String,
        prompt_selection: RunPromptSelection,
    },
}

impl RunLaunchInput {
    fn item_id(&self) -> i64 {
        match self {
            Self::Direct { item_id, .. }
            | Self::Grill { item_id, .. }
            | Self::Worktree { item_id, .. } => *item_id,
        }
    }

    fn agent(&self) -> AgentKind {
        match self {
            Self::Direct { agent, .. } | Self::Worktree { agent, .. } => *agent,
            Self::Grill { configuration, .. } => configuration.agent,
        }
    }

    fn session_kind(&self) -> &'static str {
        match self {
            Self::Grill { .. } => "grill",
            Self::Direct { .. } | Self::Worktree { .. } => "run",
        }
    }

    fn uses_pstack_workflow(&self) -> bool {
        match self {
            Self::Direct { workflow, .. } | Self::Worktree { workflow, .. } => {
                *workflow == crate::domain::Workflow::Pstack
            }
            Self::Grill { .. } => false,
        }
    }
}

#[derive(Clone)]
struct WorktreeRunIdentity {
    workspace: Workspace,
    worktree: Worktree,
    repository: Repository,
}

#[derive(Clone)]
struct RunLaunchSnapshot {
    state: DomainState,
    item: Item,
    project: Project,
    context: Context,
    workspace: Workspace,
    machine: Machine,
    cli_configuration_profile: Option<crate::domain::CliConfigurationProfile>,
    terminal_runtime: Arc<dyn TerminalRuntime>,
    machine_access: Arc<dyn crate::machine_access::MachineAccess>,
    preferred_executable: Option<PathBuf>,
    input: RunLaunchInput,
    prompt: String,
    direct_checkout: Option<DirectCheckoutSnapshot>,
    worktree: Option<WorktreeRunIdentity>,
    run_id: i64,
    session_name: String,
    gate_channel: String,
    started_at: i64,
}

struct RunLaunchObservation {
    snapshot: RunLaunchSnapshot,
    working_directory: String,
    checkouts: Vec<RunCheckout>,
}

impl RunLaunchSnapshot {
    fn event(&self, session_name: String, pane_id: String, checkouts: Vec<RunCheckout>) -> Event {
        match &self.input {
            RunLaunchInput::Direct {
                item_id,
                workspace_id,
                primary_repository_id,
                agent,
                configuration,
                implementation_queue,
                execution_profile,
                workflow,
                prompt_selection,
                allow_dirty,
                allow_shared_checkouts,
                ..
            } => {
                let working_directory = checkouts
                    .iter()
                    .find(|checkout| checkout.repository_id == *primary_repository_id)
                    .map(|checkout| checkout.path.clone())
                    .unwrap_or_default();
                Event::StartRun(RunStart {
                    item_id: *item_id,
                    workspace_id: *workspace_id,
                    machine_id: self.machine.id,
                    agent: *agent,
                    configuration: configuration.clone(),
                    execution_profile: *execution_profile,
                    workflow: Some(*workflow),
                    prompt: self.prompt.clone(),
                    working_directory,
                    session_name,
                    pane_id,
                    started_at: self.started_at,
                    prompt_selection: Some(prompt_selection.clone()),
                    strategy: RunStartStrategy::Direct {
                        repository_id: *primary_repository_id,
                        checkouts,
                        allow_dirty: *allow_dirty,
                        allow_shared_checkouts: *allow_shared_checkouts,
                        implementation_queue: implementation_queue.clone(),
                    },
                })
            }
            RunLaunchInput::Grill {
                item_id,
                workspace_id,
                primary_repository_id,
                configuration,
                allow_dirty,
                allow_shared_checkouts,
                ..
            } => {
                let working_directory = checkouts
                    .iter()
                    .find(|checkout| checkout.repository_id == *primary_repository_id)
                    .map(|checkout| checkout.path.clone())
                    .unwrap_or_default();
                Event::StartRun(RunStart {
                    item_id: *item_id,
                    workspace_id: *workspace_id,
                    machine_id: self.machine.id,
                    agent: configuration.agent,
                    configuration: Some(configuration.clone()),
                    execution_profile: ExecutionProfile::Grill,
                    workflow: None,
                    prompt: self.prompt.clone(),
                    working_directory,
                    session_name,
                    pane_id,
                    started_at: self.started_at,
                    prompt_selection: None,
                    strategy: RunStartStrategy::Grill {
                        repository_id: *primary_repository_id,
                        skill_snapshot: crate::domain::grill_skill_snapshot().into(),
                        checkouts,
                        allow_dirty: *allow_dirty,
                        allow_shared_checkouts: *allow_shared_checkouts,
                    },
                })
            }
            RunLaunchInput::Worktree {
                item_id,
                workspace_id,
                worktree_id,
                agent,
                configuration,
                execution_profile,
                workflow,
                prompt_selection,
                ..
            } => Event::StartRun(RunStart {
                item_id: *item_id,
                workspace_id: *workspace_id,
                machine_id: self.machine.id,
                agent: *agent,
                configuration: configuration.clone(),
                execution_profile: *execution_profile,
                workflow: Some(*workflow),
                prompt: self.prompt.clone(),
                working_directory: self
                    .worktree
                    .as_ref()
                    .map(|identity| identity.worktree.path.clone())
                    .unwrap_or_default(),
                session_name,
                pane_id,
                started_at: self.started_at,
                prompt_selection: Some(prompt_selection.clone()),
                strategy: RunStartStrategy::Worktree {
                    worktree_id: *worktree_id,
                },
            }),
        }
    }

    fn is_current(&self, runtime: &Runtime) -> bool {
        if runtime.state.next_run_id != self.run_id
            || !runtime
                .state
                .items
                .iter()
                .any(|current| current == &self.item)
            || !runtime
                .state
                .projects
                .iter()
                .any(|current| current == &self.project)
            || !runtime
                .state
                .contexts
                .iter()
                .any(|current| current == &self.context)
            || !runtime
                .state
                .workspaces
                .iter()
                .any(|current| current == &self.workspace)
            || !runtime
                .state
                .machines
                .iter()
                .any(|current| machine_execution_identity_matches(current, &self.machine))
        {
            return false;
        }
        if let Some(snapshot) = &self.direct_checkout {
            if !runtime.direct_checkout_snapshot_is_current(snapshot) {
                return false;
            }
        }
        if let Some(identity) = &self.worktree {
            if !runtime
                .state
                .worktrees
                .iter()
                .any(|current| current == &identity.worktree)
                || !runtime
                    .state
                    .repositories
                    .iter()
                    .any(|current| current == &identity.repository)
            {
                return false;
            }
        }
        true
    }

    fn observe(self) -> Result<RunLaunchObservation, String> {
        let (working_directory, checkouts) = match &self.input {
            RunLaunchInput::Direct {
                primary_repository_id,
                expected_checkouts,
                ..
            }
            | RunLaunchInput::Grill {
                primary_repository_id,
                expected_checkouts,
                ..
            } => {
                let checkout = self
                    .direct_checkout
                    .as_ref()
                    .ok_or_else(|| "Direct checkout snapshot is unavailable".to_owned())?
                    .clone()
                    .observe()?;
                let checkouts = checkout
                    .checkouts
                    .iter()
                    .map(|checkout| RunCheckout {
                        repository_id: checkout.repository_id,
                        path: checkout.path.clone(),
                        branch: checkout.branch.clone(),
                        is_dirty: checkout.is_dirty,
                    })
                    .collect::<Vec<_>>();
                if !run_checkout_previews_match(
                    expected_checkouts,
                    &checkouts,
                    self.context.check_dirty_checkouts,
                ) {
                    return Err(match &self.input {
                        RunLaunchInput::Direct { .. } => "A Direct checkout changed after the preview (branch or dirty state); review the Direct Run preview again before starting".into(),
                        _ => "A Grill checkout changed after the preview (branch or dirty state); review the Grill preview again before starting".into(),
                    });
                }
                let working_directory = checkouts
                    .iter()
                    .find(|checkout| checkout.repository_id == *primary_repository_id)
                    .map(|checkout| checkout.path.clone())
                    .ok_or_else(|| {
                        "Choose a configured Repository checkout before starting".to_owned()
                    })?;
                (working_directory, checkouts)
            }
            RunLaunchInput::Worktree { .. } => {
                let identity = self
                    .worktree
                    .as_ref()
                    .ok_or_else(|| "Worktree snapshot is unavailable".to_owned())?;
                let checkout_path = self
                    .machine_access
                    .resolve_path(&self.machine, &identity.worktree.path);
                let inspection = GitCli::system()
                    .inspect_checkout_on_machine(&self.machine, &checkout_path)
                    .map_err(|error| {
                        format!(
                            "Could not inspect Worktree {} on Machine {}: {error}",
                            identity.worktree.path, self.machine.name
                        )
                    })?;
                if inspection.current_branch != identity.worktree.branch {
                    return Err(
                        "The Worktree branch changed after it was approved; review the Worktree before starting a Run".into(),
                    );
                }
                match inspection.remote_url.as_deref() {
                    Some(remote_url) if remote_url == identity.repository.remote_url => {}
                    Some(_) => {
                        return Err(
                            "The Worktree remote does not match its registered Repository".into(),
                        )
                    }
                    None => {
                        return Err(
                            "The Worktree has no configured remote for its registered Repository"
                                .into(),
                        )
                    }
                }
                (identity.worktree.path.clone(), Vec::new())
            }
        };
        Ok(RunLaunchObservation {
            snapshot: self,
            working_directory,
            checkouts,
        })
    }
}

fn machine_execution_identity_matches(current: &Machine, expected: &Machine) -> bool {
    current.id == expected.id
        && current.context_id == expected.context_id
        && current.name == expected.name
        && current.socket_name == expected.socket_name
        && current.transport == expected.transport
}

fn run_checkout_previews_match(
    expected: &[RunCheckout],
    observed: &[RunCheckout],
    check_dirty_checkouts: bool,
) -> bool {
    expected.len() == observed.len()
        && expected.iter().zip(observed).all(|(expected, observed)| {
            expected.repository_id == observed.repository_id
                && expected.path == observed.path
                && expected.branch == observed.branch
                && (!check_dirty_checkouts || expected.is_dirty == observed.is_dirty)
        })
}

fn worktree_run_identity(
    runtime: &mut Runtime,
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
) -> Result<(Machine, WorktreeRunIdentity), String> {
    let workspace = runtime
        .state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id && workspace.item_id == item_id)
        .cloned()
        .ok_or_else(|| format!("Workspace {workspace_id} does not belong to Item {item_id}"))?;
    let worktree = runtime
        .state
        .worktrees
        .iter()
        .find(|worktree| worktree.id == worktree_id && worktree.workspace_id == workspace_id)
        .cloned()
        .ok_or_else(|| format!("Worktree {worktree_id} is not registered for this Item"))?;
    let repository = runtime
        .state
        .repositories
        .iter()
        .find(|repository| repository.id == worktree.repository_id)
        .cloned()
        .ok_or_else(|| format!("Repository {} does not exist", worktree.repository_id))?;
    let machine = runtime.machine_for_item(item_id, Some(worktree.machine_id))?;
    Ok((
        machine,
        WorktreeRunIdentity {
            workspace,
            worktree,
            repository,
        },
    ))
}

fn run_launch_snapshot(
    runtime: &mut Runtime,
    input: RunLaunchInput,
) -> Result<RunLaunchSnapshot, String> {
    match &input {
        RunLaunchInput::Direct {
            configuration: Some(configuration),
            agent,
            ..
        }
        | RunLaunchInput::Worktree {
            configuration: Some(configuration),
            agent,
            ..
        } => {
            crate::domain::validate_grill_configuration(configuration)
                .map_err(|error| error.to_string())?;
            if configuration.agent != *agent {
                return Err("Run model configuration must match the selected agent".into());
            }
        }
        RunLaunchInput::Grill { configuration, .. } => {
            crate::domain::validate_grill_configuration(configuration)
                .map_err(|error| error.to_string())?
        }
        _ => {}
    }
    let (machine, direct_checkout, worktree) = match &input {
        RunLaunchInput::Direct {
            item_id,
            workspace_id,
            machine_id,
            ..
        }
        | RunLaunchInput::Grill {
            item_id,
            workspace_id,
            machine_id,
            ..
        } => {
            let checkout = direct_checkout_snapshot(runtime, *item_id, *workspace_id, *machine_id)?;
            (checkout.machine.clone(), Some(checkout), None)
        }
        RunLaunchInput::Worktree {
            item_id,
            workspace_id,
            worktree_id,
            ..
        } => {
            let (machine, identity) =
                worktree_run_identity(runtime, *item_id, *workspace_id, *worktree_id)?;
            (machine, None, Some(identity))
        }
    };
    let item = runtime
        .state
        .items
        .iter()
        .find(|item| item.id == input.item_id())
        .cloned()
        .ok_or_else(|| format!("Item {} does not exist", input.item_id()))?;
    let project = runtime
        .state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .cloned()
        .ok_or_else(|| format!("Project {} does not exist", item.project_id))?;
    let context = runtime
        .state
        .contexts
        .iter()
        .find(|context| context.id == project.context_id)
        .cloned()
        .ok_or_else(|| format!("Context {} does not exist", project.context_id))?;
    let profile_id = context.cli_configuration_profile_id(input.agent());
    let cli_configuration_profile = profile_id
        .map(|profile_id| {
            runtime
                .state
                .cli_configuration_profiles
                .iter()
                .find(|profile| {
                    profile.id == profile_id
                        && profile.machine_id == machine.id
                        && profile.provider == input.agent()
                })
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Selected {} configuration profile {profile_id} is unavailable on Machine {}",
                        agent_display_name(input.agent()),
                        machine.name
                    )
                })
        })
        .transpose()?;
    let workspace = direct_checkout
        .as_ref()
        .map(|snapshot| snapshot.workspace.clone())
        .or_else(|| worktree.as_ref().map(|snapshot| snapshot.workspace.clone()))
        .ok_or_else(|| "Run Workspace snapshot is unavailable".to_owned())?;
    let run_id = runtime.state.next_run_id;
    let session_name = format!(
        "mission-item-{}-{}-{}",
        input.item_id(),
        input.session_kind(),
        run_id
    );
    let gate_channel = format!("mission-launch-{run_id}-{session_name}");
    let prompt = match &input {
        RunLaunchInput::Grill { prompt, .. } => {
            crate::domain::clean_name(prompt.clone(), crate::domain::DomainError::EmptyRunPrompt)
                .map_err(|error| error.to_string())?
        }
        RunLaunchInput::Direct {
            implementation_queue: Some(queue),
            ..
        } => {
            let ticket = queue
                .entries
                .first()
                .ok_or_else(|| "Implementation Queue has no tickets".to_owned())?;
            implementation_queue_entry_prompt(ticket, &queue.spec_url)
        }
        RunLaunchInput::Direct { prompt, .. } | RunLaunchInput::Worktree { prompt, .. } => {
            prompt.clone()
        }
    };
    let preferred_executable = runtime.preferred_agent_executable(&machine, input.agent())?;
    let snapshot = RunLaunchSnapshot {
        state: runtime.state.clone(),
        item,
        project,
        context,
        workspace,
        machine: machine.clone(),
        cli_configuration_profile,
        terminal_runtime: Arc::clone(&runtime.terminal_runtime),
        machine_access: Arc::clone(&runtime.machine_access),
        preferred_executable,
        input,
        prompt,
        direct_checkout,
        worktree,
        run_id,
        session_name,
        gate_channel,
        started_at: current_unix_seconds(),
    };
    Ok(snapshot)
}

pub(crate) async fn launch(
    request: RunLaunchRequest,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    let queue_attachment = request.queue_attachment.clone();
    start_run_with_state(request.into_input(), queue_attachment, state).await
}

async fn start_run_with_state(
    input: RunLaunchInput,
    queue_attachment: Option<ImplementationQueueEntryAttachment>,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    let serial = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        Arc::clone(&runtime.run_launch_lock)
    };
    let _serial_guard = serial.lock().await;
    let snapshot = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        run_launch_snapshot(&mut runtime, input)?
    };
    let observation_snapshot = snapshot.clone();
    let observation = tauri::async_runtime::spawn_blocking(move || observation_snapshot.observe())
        .await
        .map_err(|error| format!("Run checkout inspection worker failed: {error}"))??;
    let preflight_snapshot = observation.snapshot.clone();
    let preflight_result = tauri::async_runtime::spawn_blocking(move || {
        let terminal_runtime = Arc::clone(&preflight_snapshot.terminal_runtime);
        let machine = preflight_snapshot.machine.clone();
        let preferred_executable = preflight_snapshot.preferred_executable.clone();
        let profile = preflight_snapshot.cli_configuration_profile.clone();
        terminal_runtime.preflight_agent_run(
            &machine,
            preflight_snapshot.input.agent(),
            preflight_snapshot.run_id,
            preferred_executable.as_deref(),
            profile.as_ref(),
        )
    })
    .await
    .map_err(|error| format!("Run preflight worker failed: {error}"))?;
    let mut readiness = preflight_result.readiness.clone();
    let preflight_values = (|| {
        if let Some(error) = readiness.error.clone() {
            return Err(format!(
                "Run preflight failed on Machine {}. The Run was not started locally: {error}",
                observation.snapshot.machine.name
            ));
        }
        let executable = preflight_result.executable.clone().ok_or_else(|| {
            let error = format!(
                "{} executable did not resolve on Machine {}",
                agent_display_name(observation.snapshot.input.agent()),
                observation.snapshot.machine.name
            );
            readiness.error = Some(error.clone());
            error
        })?;
        let state_file = preflight_result.state_file.clone().ok_or_else(|| {
            let error = format!(
                "Agent state file path was not prepared on Machine {}",
                observation.snapshot.machine.name
            );
            readiness.error = Some(error.clone());
            error
        })?;
        Ok((executable, state_file))
    })();
    {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        if !observation.snapshot.is_current(&runtime) {
            return Err("The Item, Workspace, Repository, Worktree, or Machine changed while the Run was being prepared; review it again".into());
        }
        if let Err(error) = &preflight_values {
            if readiness.error.is_none() {
                readiness.error = Some(error.clone());
            }
            runtime
                .machine_readiness
                .insert(observation.snapshot.machine.id, readiness);
            return Err(error.clone());
        }
        runtime
            .machine_readiness
            .insert(observation.snapshot.machine.id, readiness);
        let (executable, _) = preflight_values.as_ref().expect("checked preflight values");
        if let Err(error) = runtime.store_agent_executable(
            &observation.snapshot.machine,
            observation.snapshot.input.agent(),
            executable,
        ) {
            let readiness = runtime
                .machine_readiness
                .get_mut(&observation.snapshot.machine.id)
                .expect("preflight readiness was just stored");
            readiness.error = Some(error.clone());
            return Err(format!(
                "Run preflight failed on Machine {}. The Run was not started locally: {error}",
                observation.snapshot.machine.name
            ));
        }
        runtime.observe_machine(
            observation.snapshot.machine.id,
            MachineObservation::Available,
        )?;
    }
    let (executable, state_file) = preflight_values?;
    let profile_directory = preflight_result.profile_directory.clone();
    {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        if !observation.snapshot.is_current(&runtime) {
            return Err("The Item, Workspace, Repository, Worktree, or Machine changed while the Run was being prepared; review it again".into());
        }
    }
    let (agent, model, effort) = match &observation.snapshot.input {
        RunLaunchInput::Grill { configuration, .. } => (
            configuration.agent,
            Some(configuration.model.clone()),
            Some(configuration.effort.clone()),
        ),
        RunLaunchInput::Direct {
            configuration: Some(configuration),
            ..
        }
        | RunLaunchInput::Worktree {
            configuration: Some(configuration),
            ..
        } => (
            configuration.agent,
            Some(configuration.model.clone()),
            Some(configuration.effort.clone()),
        ),
        _ => (observation.snapshot.input.agent(), None, None),
    };
    let run_id = observation.snapshot.run_id;
    let terminal_runtime = Arc::clone(&observation.snapshot.terminal_runtime);
    let machine = observation.snapshot.machine.clone();
    let session_name = observation.snapshot.session_name.clone();
    let gate_channel = observation.snapshot.gate_channel.clone();
    let working_directory = observation.working_directory.clone();
    let prompt = observation.snapshot.prompt.clone();
    if observation.snapshot.input.uses_pstack_workflow() {
        let provision_access = Arc::clone(&observation.snapshot.machine_access);
        let provision_machine = observation.snapshot.machine.clone();
        let root = tauri::async_runtime::spawn_blocking(move || {
            provision_access.provision_pstack_tree(&provision_machine)
        })
        .await
        .map_err(|error| format!("pstack provisioning worker failed: {error}"))??;
        let context = &observation.snapshot.context;
        let claude_profile = context.claude_profile_id.and_then(|profile_id| {
            observation
                .snapshot
                .state
                .cli_configuration_profiles
                .iter()
                .find(|profile| profile.id == profile_id)
        });
        let codex_profile = context.codex_profile_id.and_then(|profile_id| {
            observation
                .snapshot
                .state
                .cli_configuration_profiles
                .iter()
                .find(|profile| profile.id == profile_id)
        });
        let role_path = crate::domain::pstack_role_file_path(&root, context);
        let role_contents = crate::domain::compose_pstack_role_file(
            observation.snapshot.input.agent(),
            &context.pstack_roles,
            claude_profile,
            codex_profile,
        );
        let role_access = Arc::clone(&observation.snapshot.machine_access);
        let role_machine = observation.snapshot.machine.clone();
        let role_file_path = PathBuf::from(&role_path);
        tauri::async_runtime::spawn_blocking(move || {
            role_access.write_file(&role_machine, &role_file_path, role_contents.as_bytes())
        })
        .await
        .map_err(|error| format!("pstack role-file worker failed: {error}"))??;
    }
    let pane_id = tauri::async_runtime::spawn_blocking(move || {
        let launch = AgentLaunchContext {
            run_id,
            state_file: &state_file,
            agent,
            model: model.as_deref(),
            effort: effort.as_deref(),
            profile_directory: profile_directory.as_deref(),
        };
        terminal_runtime.launch_agent(
            &machine,
            &session_name,
            &gate_channel,
            Path::new(&working_directory),
            &executable,
            &prompt,
            launch,
        )
    })
    .await
    .map_err(|error| format!("Agent launch worker failed: {error}"))??;

    let record_result = match state.lock() {
        Ok(mut runtime) => {
            if !observation.snapshot.is_current(&runtime) {
                Err("The Item, Workspace, Repository, Worktree, or Machine changed before the Run could be recorded; review it again".to_owned())
            } else {
                let decision = decide(
                    runtime.state.clone(),
                    observation.snapshot.event(
                        observation.snapshot.session_name.clone(),
                        pane_id,
                        observation.checkouts.clone(),
                    ),
                )
                .map_err(|error| error.to_string());
                match decision.and_then(|start_decision| {
                    let run = start_decision
                        .state
                        .runs
                        .iter()
                        .find(|run| run.id == observation.snapshot.run_id)
                        .cloned()
                        .ok_or_else(|| "Run creation produced no Run".to_owned())?;
                    let combined = if let Some(attachment) = &queue_attachment {
                        let attach_decision = decide(
                            start_decision.state.clone(),
                            Event::SetImplementationQueueEntryRun {
                                queue_id: attachment.queue_id,
                                position: attachment.position,
                                run_id: run.id,
                            },
                        )
                        .map_err(|error| error.to_string())?;
                        let mut effects = start_decision.effects;
                        effects.extend(attach_decision.effects);
                        crate::domain::Decision {
                            state: attach_decision.state,
                            effects,
                        }
                    } else {
                        start_decision
                    };
                    runtime.commit(combined).map(|()| run)
                }) {
                    Ok(run) => Ok(run),
                    Err(error) => Err(error),
                }
            }
        }
        Err(_) => {
            Err("Mission Manager state is unavailable after the gated Pane was created".to_owned())
        }
    };
    let run = match record_result {
        Ok(run) => run,
        Err(error) => {
            let terminal_runtime = Arc::clone(&observation.snapshot.terminal_runtime);
            let machine = observation.snapshot.machine.clone();
            let session_name = observation.snapshot.session_name.clone();
            let cleanup = tauri::async_runtime::spawn_blocking(move || {
                terminal_runtime.kill_session(&machine, &session_name)
            })
            .await
            .map_err(|worker_error| format!("Launch cleanup worker failed: {worker_error}"));
            let cleanup_error = match cleanup {
                Ok(result) => result.err(),
                Err(error) => Some(error),
            };
            return Err(format_commit_error(error, cleanup_error));
        }
    };

    let terminal_runtime = Arc::clone(&observation.snapshot.terminal_runtime);
    let machine = observation.snapshot.machine.clone();
    let gate_channel = observation.snapshot.gate_channel.clone();
    let release = tauri::async_runtime::spawn_blocking(move || {
        terminal_runtime.release_agent_launch(&machine, &gate_channel)
    })
    .await
    .map_err(|error| format!("Launch release worker failed: {error}"));
    let release_error = match release {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(error) => Some(error),
    };
    if let Some(error) = release_error {
        return Err(format!(
            "Run {} is recorded but the agent was not released: {error}",
            run.id
        ));
    }
    Ok(run)
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) async fn start_direct_run_with_queue_state(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    agent: AgentKind,
    configuration: Option<GrillConfiguration>,
    implementation_queue: Option<crate::domain::ImplementationQueueStart>,
    execution_profile: ExecutionProfile,
    workflow: crate::domain::Workflow,
    prompt: String,
    prompt_selection: RunPromptSelection,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    start_direct_run_with_queue_attachment_state(
        item_id,
        workspace_id,
        machine_id,
        primary_repository_id,
        agent,
        configuration,
        implementation_queue,
        execution_profile,
        workflow,
        prompt,
        prompt_selection,
        expected_checkouts,
        allow_dirty,
        allow_shared_checkouts,
        None,
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_direct_run_with_queue_attachment_state(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    agent: AgentKind,
    configuration: Option<GrillConfiguration>,
    implementation_queue: Option<crate::domain::ImplementationQueueStart>,
    execution_profile: ExecutionProfile,
    workflow: crate::domain::Workflow,
    prompt: String,
    prompt_selection: RunPromptSelection,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    queue_attachment: Option<ImplementationQueueEntryAttachment>,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    launch(
        RunLaunchRequest {
            item_id,
            workspace_id,
            queue_attachment,
            strategy: RunLaunchStrategy::Direct {
                machine_id,
                primary_repository_id,
                agent,
                configuration,
                implementation_queue,
                execution_profile,
                workflow,
                prompt,
                prompt_selection,
                expected_checkouts,
                allow_dirty,
                allow_shared_checkouts,
            },
        },
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
#[cfg(test)]
pub(crate) async fn start_direct_run_with_state(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    agent: AgentKind,
    configuration: Option<GrillConfiguration>,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    start_direct_run_with_queue_state(
        item_id,
        workspace_id,
        machine_id,
        primary_repository_id,
        agent,
        configuration,
        None,
        execution_profile,
        crate::domain::Workflow::MattPocock,
        prompt,
        prompt_selection,
        expected_checkouts,
        allow_dirty,
        allow_shared_checkouts,
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) async fn start_grill_run_with_state(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    configuration: GrillConfiguration,
    language: GrillLanguage,
    prompt: String,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    launch(
        RunLaunchRequest {
            item_id,
            workspace_id,
            queue_attachment: None,
            strategy: RunLaunchStrategy::Grill {
                machine_id,
                primary_repository_id,
                configuration,
                language,
                prompt,
                expected_checkouts,
                allow_dirty,
                allow_shared_checkouts,
            },
        },
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) async fn start_worktree_run_with_state(
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
    agent: AgentKind,
    configuration: Option<GrillConfiguration>,
    execution_profile: ExecutionProfile,
    workflow: crate::domain::Workflow,
    prompt: String,
    prompt_selection: RunPromptSelection,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    launch(
        RunLaunchRequest {
            item_id,
            workspace_id,
            queue_attachment: None,
            strategy: RunLaunchStrategy::Worktree {
                worktree_id,
                agent,
                configuration,
                execution_profile,
                workflow,
                prompt,
                prompt_selection,
            },
        },
        state,
    )
    .await
}
