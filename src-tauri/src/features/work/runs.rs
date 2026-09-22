use super::*;

impl Runtime {
    pub(crate) fn compose_run_prompt(
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

    pub(crate) fn compose_grill_prompt(
        &self,
        item_id: i64,
        configuration: GrillConfiguration,
        initial_prompt: String,
    ) -> Result<String, String> {
        build_grill_prompt(&self.state, item_id, &configuration, &initial_prompt)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn inspect_direct_checkouts(
        &mut self,
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
    ) -> Result<(Machine, Vec<DirectRunCheckoutPreview>), String> {
        if !self
            .state
            .workspaces
            .iter()
            .any(|workspace| workspace.id == workspace_id && workspace.item_id == item_id)
        {
            return Err(format!(
                "Project Repository execution setup does not belong to Item {item_id}"
            ));
        }
        let machine = self.machine_for_item(item_id, machine_id)?;
        let machine_home = machine_home_directory(&machine);
        let git = GitCli::system();
        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| format!("Item {item_id} does not exist"))?;
        let repositories = self
            .state
            .repositories
            .iter()
            .filter(|repository| repository.project_id == item.project_id)
            .cloned()
            .collect::<Vec<_>>();
        if repositories.is_empty() {
            return Err("Register a Repository under this Project before starting a Run".into());
        }
        let mut previews = Vec::with_capacity(repositories.len());

        for repository in repositories {
            let location = self
                .state
                .repository_locations
                .iter()
                .find(|location| {
                    location.repository_id == repository.id && location.machine_id == machine.id
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

    pub(crate) fn prepare_direct_run(
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
                && run_is_active(run)
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
                .ok_or_else(|| "Project has no configured Repositories".to_owned())?,
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
    pub(crate) fn start_direct_run(
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
            .ok_or_else(|| "Project has no configured Repositories".to_owned())?;
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
                agent: None,
                model: None,
                effort: None,
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_grill_run(
        &mut self,
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
        primary_repository_id: i64,
        configuration: GrillConfiguration,
        initial_prompt: String,
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
                "A Grill checkout changed after the preview (branch or dirty state); review the Grill preview again before starting"
                    .into(),
            );
        }
        let working_directory = current_checkouts
            .iter()
            .find(|checkout| checkout.repository_id == primary_repository_id)
            .map(|checkout| checkout.path.clone())
            .ok_or_else(|| "Choose a selected Repository checkout for the Grill".to_owned())?;
        if !allow_dirty {
            let dirty_repository_ids = current_checkouts
                .iter()
                .filter(|checkout| checkout.is_dirty)
                .map(|checkout| checkout.repository_id)
                .collect::<Vec<_>>();
            if !dirty_repository_ids.is_empty() {
                return Err(format!(
                    "Grill checkout(s) are dirty: {}; review the preview and confirm before starting",
                    dirty_repository_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        if !allow_shared_checkouts {
            let shared_paths = self
                .state
                .runs
                .iter()
                .filter(|run| {
                    run.machine_id == machine.id
                        && run_is_active(run)
                        && run.pane_status != RunPaneStatus::Missing
                })
                .flat_map(|run| {
                    current_checkouts.iter().filter_map(move |checkout| {
                        run.direct_checkouts
                            .iter()
                            .any(|active| active.path == checkout.path)
                            .then_some(checkout.path.clone())
                    })
                })
                .collect::<Vec<_>>();
            if !shared_paths.is_empty() {
                return Err(format!(
                    "Grill checkout(s) are already used by an active Run: {}; review the preview and confirm before starting",
                    shared_paths.join(", ")
                ));
            }
        }
        let prompt = self.compose_grill_prompt(item_id, configuration.clone(), initial_prompt)?;
        let run_id = self.state.next_run_id;
        let session_name = format!("mission-item-{item_id}-grill-{run_id}");
        let preflight = decide(
            self.state.clone(),
            Event::StartGrillRun {
                item_id,
                workspace_id,
                repository_id: primary_repository_id,
                machine_id: machine.id,
                configuration: configuration.clone(),
                prompt: prompt.clone(),
                skill_snapshot: crate::domain::GRILL_SKILL_SNAPSHOT.into(),
                working_directory: working_directory.clone(),
                session_name: format!("{session_name}-preflight"),
                pane_id: "%preflight".into(),
                started_at: current_unix_seconds(),
                checkouts: current_checkouts.clone(),
            },
        )
        .map_err(|error| error.to_string())?;

        if let Err(error) = probe_machine(&machine) {
            return Err(format!(
                "Could not reach Machine {}. The Grill was not started locally: {error}",
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
            .agent_executable(&machine, configuration.agent)
            .map_err(|error| format!("{}: {error}", agent_display_name(configuration.agent)))?;
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
                agent: Some(configuration.agent),
                model: Some(&configuration.model),
                effort: Some(&configuration.effort),
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
                "A Grill checkout changed before the Run was recorded; review the Grill preview again"
                    .into(),
                cleanup.err(),
            ));
        }
        let decision = match decide(
            self.state.clone(),
            Event::StartGrillRun {
                item_id,
                workspace_id,
                repository_id: primary_repository_id,
                machine_id: machine.id,
                configuration,
                prompt,
                skill_snapshot: crate::domain::GRILL_SKILL_SNAPSHOT.into(),
                working_directory,
                session_name: session_name.clone(),
                pane_id,
                started_at: current_unix_seconds(),
                checkouts: after_launch,
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
            .ok_or_else(|| "Grill Run creation produced no Run".to_owned())?;
        if let Err(error) = self.commit(decision) {
            let cleanup = terminal.kill_session(&machine, &run.session_name);
            return Err(format_commit_error(error, cleanup.err()));
        }
        let _ = (preflight, allow_dirty, allow_shared_checkouts);
        Ok(run)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_worktree_run(
        &mut self,
        item_id: i64,
        workspace_id: i64,
        worktree_id: i64,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
        prompt: String,
        prompt_selection: RunPromptSelection,
    ) -> Result<Run, String> {
        let worktree = self
            .state
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id && worktree.workspace_id == workspace_id)
            .cloned()
            .ok_or_else(|| format!("Worktree {worktree_id} is not registered for this Item"))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == worktree.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", worktree.machine_id))?;
        let working_directory = worktree.path.clone();
        let run_id = self.state.next_run_id;
        let session_name = format!("mission-item-{item_id}-run-{run_id}");
        let preflight = decide(
            self.state.clone(),
            worktree_run_event(
                item_id,
                workspace_id,
                worktree_id,
                machine.id,
                agent,
                execution_profile,
                prompt.clone(),
                working_directory.clone(),
                format!("{session_name}-preflight"),
                "%preflight".into(),
                current_unix_seconds(),
                prompt_selection.clone(),
            ),
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
                agent: None,
                model: None,
                effort: None,
            },
        )?;
        let decision = match decide(
            self.state.clone(),
            worktree_run_event(
                item_id,
                workspace_id,
                worktree_id,
                machine.id,
                agent,
                execution_profile,
                prompt,
                working_directory,
                session_name.clone(),
                pane_id,
                current_unix_seconds(),
                prompt_selection,
            ),
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
            .ok_or_else(|| "Worktree Run creation produced no Run".to_owned())?;
        if let Err(error) = self.commit(decision) {
            let cleanup = terminal.kill_session(&machine, &run.session_name);
            return Err(format_commit_error(error, cleanup.err()));
        }
        let _ = preflight;
        Ok(run)
    }

    pub(crate) fn list_run_suggestions(&mut self) -> Result<Vec<RunSuggestion>, String> {
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
                        machine_home: machine_home_directory(machine),
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

    pub(crate) fn attach_run(&mut self, suggestion: RunSuggestion) -> Result<Run, String> {
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
                machine_home: machine_home_directory(&machine),
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
            let repository_id = canonical.repository_id.ok_or_else(|| {
                "The suggested Item execution location has no Repository".to_owned()
            })?;
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
                    machine_home: machine_home_directory(&machine),
                    session_name: canonical.session_name,
                    pane_id: canonical.pane_id,
                    attached_at: current_unix_seconds(),
                },
            )
        } else {
            return Err(
                "The suggested agent is not in a registered Item execution location".into(),
            );
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

    pub(crate) fn open_terminal(
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
                    if matches!(record.state, RunState::Blocked | RunState::Finished) {
                        match runtime.capture_grill_transcript(run_id) {
                            Ok(true) => {
                                let _ = state_app.emit("run-questions-changed", run_id);
                            }
                            Ok(false) => {}
                            Err(error) => {
                                eprintln!(
                                    "Could not retain transcript for waiting Grill Run {run_id}: {error}"
                                );
                            }
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
        if run.pane_status != RunPaneStatus::Available {
            let decision = decide(
                self.state.clone(),
                Event::SetRunPaneStatus {
                    run_id,
                    status: RunPaneStatus::Available,
                },
            )
            .map_err(|error| error.to_string())?;
            if let Err(error) = self.commit(decision) {
                let _ = connection.close();
                return Err(error);
            }
        }
        if run.execution_profile == ExecutionProfile::Grill {
            if let Err(error) = self.capture_grill_transcript(run_id) {
                eprintln!(
                    "Could not capture transcript while reopening Grill Run {run_id}: {error}"
                );
            }
        }
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

    pub(crate) fn list_run_panes(&mut self, run_id: i64) -> Result<Vec<PaneTab>, String> {
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
        let panes = list_panes(machine, &run.session_name)?;
        Ok(panes
            .into_iter()
            .map(|pane| PaneTab::from_summary(&run, &run.session_name, pane, true))
            .collect())
    }

    pub(crate) fn terminal_input(&self, terminal_id: &str, input: Vec<u8>) -> Result<(), String> {
        self.terminal_connections
            .get(terminal_id)
            .ok_or_else(|| "The embedded terminal is not attached".to_owned())?
            .send_input(&input)
    }

    pub(crate) fn terminal_resize(
        &self,
        terminal_id: &str,
        columns: u16,
        rows: u16,
    ) -> Result<(), String> {
        self.terminal_connections
            .get(terminal_id)
            .ok_or_else(|| "The embedded terminal is not attached".to_owned())?
            .resize(columns, rows)
    }

    pub(crate) fn close_terminal(&mut self, terminal_id: &str) -> Result<(), String> {
        if let Some(connection) = self.terminal_connections.remove(terminal_id) {
            connection.close()?;
        }
        Ok(())
    }

    pub(crate) fn capture_grill_transcript(&mut self, run_id: i64) -> Result<bool, String> {
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        if run.execution_profile != ExecutionProfile::Grill {
            return Ok(false);
        }
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        let transcript = match capture_pane_transcript(&machine, &run.pane_id) {
            Ok(transcript) => String::from_utf8_lossy(&transcript).into_owned(),
            Err(error) => {
                if run.grill_action.is_some() && !run.transcript.trim().is_empty() {
                    self.capture_downstream_issues(run_id, &run.transcript)?;
                }
                return Err(error);
            }
        };
        let captured_question_group =
            parse_grill_question_group_since(&run.transcript, &transcript);
        let question_group = match (run.grill_question_group.as_ref(), captured_question_group) {
            (Some(previous), None)
                if run.state == RunState::Working || run.grill_response.is_none() =>
            {
                Some(previous.clone())
            }
            (Some(previous), Some(captured))
                if previous.questions.len() == captured.questions.len()
                    && previous
                        .questions
                        .iter()
                        .zip(captured.questions.iter())
                        .all(|(previous, captured)| {
                            previous.number == captured.number
                                && captured.prompt.starts_with(&previous.prompt)
                        }) =>
            {
                Some(previous.clone())
            }
            (_, captured) => captured,
        };
        let question_group_is_pending = question_group.is_some()
            && (run.grill_response.is_none() || run.grill_question_group != question_group);
        let finished_phase_is_synchronized = run.state != RunState::Finished
            || run.grill_phase == Some(GrillPhase::Finished)
            || run.grill_phase
                == Some(if question_group_is_pending {
                    GrillPhase::WaitingForAnswers
                } else {
                    GrillPhase::AwaitingNextAction
                });
        if run.transcript == transcript
            && run.grill_question_group == question_group
            && finished_phase_is_synchronized
        {
            self.capture_downstream_issues(run_id, &transcript)?;
            return Ok(false);
        }
        let captured_transcript = transcript.clone();
        let decision = decide(
            self.state.clone(),
            Event::RecordRunTranscript {
                run_id,
                transcript,
                question_group,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.capture_downstream_issues(run_id, &captured_transcript)?;
        Ok(true)
    }

    pub(crate) fn capture_downstream_issues(
        &mut self,
        run_id: i64,
        transcript: &str,
    ) -> Result<(), String> {
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let Some(action) = run.grill_action else {
            return Ok(());
        };
        if !matches!(
            action,
            GrillContinuationAction::ToSpec | GrillContinuationAction::ToTickets
        ) {
            return Ok(());
        }

        let candidates = discover_downstream_issue_candidates(transcript)
            .into_iter()
            .filter(|candidate| {
                candidate
                    .run_id
                    .is_none_or(|candidate_run_id| candidate_run_id == run_id)
                    && candidate
                        .action
                        .is_none_or(|candidate_action| candidate_action == action)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(());
        }

        let executable = match self.gh_executable_path() {
            Ok(executable) => executable,
            Err(error) => {
                eprintln!("Could not confirm downstream GitHub Issues for Run {run_id}: {error}");
                return Ok(());
            }
        };
        let github = GithubCli::new(executable);
        let mut confirmed = Vec::new();
        for candidate in candidates {
            let object = match classify_url(&candidate.url) {
                Ok(object)
                    if object.provider == ExternalProvider::GitHub
                        && object.kind == ExternalObjectKind::Issue =>
                {
                    object
                }
                Ok(_) | Err(_) => continue,
            };
            let snapshot = match github.fetch(&object, current_unix_seconds()) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    eprintln!(
                        "Could not confirm downstream GitHub Issue {} for Run {run_id}: {error}",
                        candidate.url
                    );
                    continue;
                }
            };
            if confirmed.iter().any(|issue: &ConfirmedDownstreamIssue| {
                issue.object.external_key == object.external_key
            }) {
                continue;
            }
            confirmed.push(ConfirmedDownstreamIssue {
                object,
                snapshot,
                discovery: candidate.discovery,
            });
        }
        if confirmed.is_empty() {
            return Ok(());
        }

        let decision = decide(
            self.state.clone(),
            Event::CaptureDownstreamIssues {
                run_id,
                action,
                issues: confirmed,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)
    }

    pub(crate) fn submit_grill_answers(
        &mut self,
        run_id: i64,
        answers: Vec<GrillAnswer>,
    ) -> Result<Run, String> {
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        if run.execution_profile != ExecutionProfile::Grill {
            return Err(format!("Run {run_id} is not a Grill Run"));
        }
        let can_submit_answers = run.state == RunState::Blocked
            || (run.state == RunState::Finished
                && run.grill_phase == Some(GrillPhase::WaitingForAnswers));
        if !can_submit_answers {
            return Err(format!("Run {run_id} is not waiting for Grill answers"));
        }
        let answers_decision = decide(
            self.state.clone(),
            Event::RecordGrillAnswers { run_id, answers },
        )
        .map_err(|error| error.to_string())?;
        let answered_run = answers_decision
            .state
            .runs
            .iter()
            .find(|candidate| candidate.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let response = format_grill_response(&answered_run.grill_answers)
            .map_err(|error| error.to_string())?;
        self.commit(answers_decision)?;

        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == answered_run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", answered_run.machine_id))?;
        let mut input = response.into_bytes();
        input.push(b'\n');
        send_input_to_pane(&machine, &answered_run.pane_id, &input)?;

        let working = decide(
            self.state.clone(),
            Event::UpdateRunState {
                run_id,
                state: RunState::Working,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(working)?;

        let response = String::from_utf8(input)
            .map_err(|_| "The grouped Grill response was not valid UTF-8".to_owned())?;
        let response = response.trim_end_matches('\n').to_owned();
        let decision = decide(
            self.state.clone(),
            Event::RecordGrillResponse { run_id, response },
        )
        .map_err(|error| error.to_string())?;
        let submitted_run = decision
            .state
            .runs
            .iter()
            .find(|candidate| candidate.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        self.commit(decision)?;
        Ok(submitted_run)
    }

    pub(crate) fn continue_grill(
        &mut self,
        run_id: i64,
        action: GrillContinuationAction,
    ) -> Result<Run, String> {
        let run = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let prompt = build_grill_continuation_prompt(&self.state, run_id, action)
            .map_err(|error| error.to_string())?;
        let decision = decide(self.state.clone(), Event::ContinueGrill { run_id, action })
            .map_err(|error| error.to_string())?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        let mut input = prompt.into_bytes();
        input.push(b'\n');
        send_input_to_pane(&machine, &run.pane_id, &input)?;

        let continued = decision
            .state
            .runs
            .iter()
            .find(|candidate| candidate.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        self.commit(decision)?;
        Ok(continued)
    }

    pub(crate) fn stop_run(&mut self, run_id: i64) -> Result<Run, String> {
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

    pub(crate) fn finish_run(&mut self, run_id: i64) -> Result<Run, String> {
        let decision = decide(self.state.clone(), Event::FinishRun { run_id })
            .map_err(|error| error.to_string())?;
        let finished = decision
            .state
            .runs
            .iter()
            .find(|candidate| candidate.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        self.commit_with_audit(decision, &[AuditAction::RunFinished { run_id }])?;
        Ok(finished)
    }

    pub(crate) fn open_external_terminal(&self, run_id: i64) -> Result<(), String> {
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

    pub(crate) fn provision_agent_hooks(&self) -> Result<(), String> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set; agent hooks cannot be provisioned".to_owned())?;
        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the Mission Manager executable: {error}"))?;
        provision_hooks(&home, &executable)
    }

    pub(crate) fn recover_run_states(&mut self) -> Result<(), String> {
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
            self.apply_agent_state_record(record_run_id, record.clone())?;
            if matches!(record.state, RunState::Blocked | RunState::Finished) {
                if let Err(error) = self.capture_grill_transcript(record_run_id) {
                    eprintln!(
                        "Could not retain transcript for waiting Grill Run {record_run_id}: {error}"
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn reconcile_runs(&mut self) -> Result<(), String> {
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
            let phase_needs_recovery =
                run.execution_profile == ExecutionProfile::Grill && run.grill_phase.is_none();
            if pane_status == run.pane_status && !phase_needs_recovery {
                if pane_status == RunPaneStatus::Available
                    && run.execution_profile == ExecutionProfile::Grill
                {
                    if let Err(error) = self.capture_grill_transcript(run.id) {
                        eprintln!(
                            "Could not reconcile transcript for Grill Run {}: {error}",
                            run.id
                        );
                    }
                }
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
            if pane_status == RunPaneStatus::Available
                && run.execution_profile == ExecutionProfile::Grill
            {
                if let Err(error) = self.capture_grill_transcript(run.id) {
                    eprintln!(
                        "Could not reconcile transcript for Grill Run {}: {error}",
                        run.id
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn apply_agent_state_record(
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
}

fn agent_display_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "Claude Code",
        AgentKind::Codex => "Codex",
    }
}
