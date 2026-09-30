use super::*;
use std::sync::Condvar;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FakeMachineOutcome {
    Available,
    Unreachable,
    TmuxQueryFailed,
    TmuxUnavailable,
    AgentUnavailable,
    StateDirectoryUnwritable,
    HookProvisioningFailed,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FakeTerminalCommand {
    PreflightAgentRun {
        machine_id: i64,
        agent: AgentKind,
        run_id: i64,
        profile: Option<CliConfigurationProfile>,
    },
    CheckMachine {
        machine_id: i64,
    },
    ObserveMachine {
        machine_id: i64,
    },
    ReadAgentStateFiles {
        machine_id: i64,
        run_ids: Vec<i64>,
    },
    CapturePaneTranscript {
        machine_id: i64,
        pane_id: String,
    },
    ListPanes {
        machine_id: i64,
        session_name: String,
    },
    ListAgentPanes {
        machine_id: i64,
    },
    LaunchAgent {
        machine_id: i64,
        session_name: String,
        gate_channel: String,
        run_id: i64,
        profile_directory: Option<PathBuf>,
    },
    ReleaseAgentLaunch {
        machine_id: i64,
        gate_channel: String,
    },
    KillSession {
        machine_id: i64,
        session_name: String,
    },
    KillPane {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    KillPaneWithTimeout {
        machine_id: i64,
        pane_id: String,
        timeout: Duration,
    },
    SendPaneInput {
        machine_id: i64,
        pane_id: String,
        input: Vec<u8>,
    },
    InterruptPane {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    AttachConnection {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    ConnectionInput {
        machine_id: i64,
        pane_id: String,
        input: Vec<u8>,
    },
    ConnectionResize {
        machine_id: i64,
        pane_id: String,
        columns: u16,
        rows: u16,
    },
    ConnectionSnapshot {
        machine_id: i64,
        pane_id: String,
    },
    CloseConnection {
        machine_id: i64,
        pane_id: String,
    },
}

#[cfg(test)]
type FakeTranscriptCaptures =
    Arc<Mutex<std::collections::HashMap<(i64, String), Result<Vec<u8>, String>>>>;

#[cfg(test)]
pub(crate) struct FakeTerminalRuntime {
    outcomes: std::collections::HashMap<i64, FakeMachineOutcome>,
    commands: Arc<Mutex<Vec<FakeTerminalCommand>>>,
    hook_configs: Arc<Mutex<std::collections::HashMap<(i64, String), String>>>,
    observed_panes: Arc<Mutex<std::collections::HashMap<i64, Vec<ObservedPane>>>>,
    pane_state_records: Arc<Mutex<std::collections::HashMap<i64, Vec<ObservedPaneAgentState>>>>,
    state_file_records: Arc<Mutex<std::collections::HashMap<i64, Vec<AgentStateRecord>>>>,
    transcript_captures: FakeTranscriptCaptures,
    release_failures: Arc<Mutex<std::collections::HashMap<i64, String>>>,
    dirty_checkout_on_launch: Arc<Mutex<bool>>,
    checkout_remote_on_launch: Arc<Mutex<Option<String>>>,
    observation_gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
    send_gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
    release_gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
}

#[cfg(test)]
#[derive(Default)]
struct FakeObservationGateState {
    enabled: bool,
    started: bool,
    released: bool,
}

#[cfg(test)]
pub(crate) struct FakeObservationBlock {
    gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
}

#[cfg(test)]
impl FakeObservationBlock {
    pub(crate) fn wait_until_started(&self, timeout: Duration) -> bool {
        let (state, changed) = &*self.gate;
        let state = state
            .lock()
            .expect("fake observation gate should remain available");
        if state.started {
            return true;
        }
        let (state, _) = changed
            .wait_timeout_while(state, timeout, |state| !state.started)
            .expect("fake observation gate should remain available");
        state.started
    }

    pub(crate) fn release(&self) {
        let (state, changed) = &*self.gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        state.enabled = false;
        state.released = true;
        changed.notify_all();
    }
}

#[cfg(test)]
impl Drop for FakeObservationBlock {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
impl FakeTerminalRuntime {
    pub(crate) fn new(outcomes: impl IntoIterator<Item = (i64, FakeMachineOutcome)>) -> Self {
        Self {
            outcomes: outcomes.into_iter().collect(),
            commands: Arc::new(Mutex::new(Vec::new())),
            hook_configs: Arc::new(Mutex::new(std::collections::HashMap::new())),
            observed_panes: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pane_state_records: Arc::new(Mutex::new(std::collections::HashMap::new())),
            state_file_records: Arc::new(Mutex::new(std::collections::HashMap::new())),
            transcript_captures: Arc::new(Mutex::new(std::collections::HashMap::new())),
            release_failures: Arc::new(Mutex::new(std::collections::HashMap::new())),
            dirty_checkout_on_launch: Arc::new(Mutex::new(false)),
            checkout_remote_on_launch: Arc::new(Mutex::new(None)),
            observation_gate: Arc::new((
                Mutex::new(FakeObservationGateState::default()),
                Condvar::new(),
            )),
            send_gate: Arc::new((
                Mutex::new(FakeObservationGateState::default()),
                Condvar::new(),
            )),
            release_gate: Arc::new((
                Mutex::new(FakeObservationGateState::default()),
                Condvar::new(),
            )),
        }
    }

    pub(crate) fn commands(&self) -> Vec<FakeTerminalCommand> {
        self.commands
            .lock()
            .expect("fake command log should remain available")
            .clone()
    }

    pub(crate) fn block_observations(&self) -> FakeObservationBlock {
        Self::block_gate(&self.observation_gate)
    }

    pub(crate) fn block_sends(&self) -> FakeObservationBlock {
        Self::block_gate(&self.send_gate)
    }

    pub(crate) fn block_launch_release(&self) -> FakeObservationBlock {
        Self::block_gate(&self.release_gate)
    }

    fn block_gate(gate: &Arc<(Mutex<FakeObservationGateState>, Condvar)>) -> FakeObservationBlock {
        let gate = Arc::clone(gate);
        let (state, _) = &*gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        state.enabled = true;
        state.started = false;
        state.released = false;
        drop(state);
        FakeObservationBlock { gate }
    }

    fn wait_for_gate(gate: &Arc<(Mutex<FakeObservationGateState>, Condvar)>) {
        let (state, changed) = &**gate;
        let mut state = state
            .lock()
            .expect("fake operation gate should remain available");
        if state.enabled {
            state.started = true;
            changed.notify_all();
            let _state = changed
                .wait_while(state, |state| !state.released)
                .expect("fake operation gate should remain available");
        }
    }

    pub(crate) fn set_observed_panes(&self, machine_id: i64, panes: Vec<ObservedPane>) {
        self.observed_panes
            .lock()
            .expect("fake observed pane list should remain available")
            .insert(machine_id, panes);
    }

    pub(crate) fn set_agent_state_records(&self, machine_id: i64, records: Vec<AgentStateRecord>) {
        self.state_file_records
            .lock()
            .expect("fake agent state records should remain available")
            .insert(machine_id, records);
    }

    pub(crate) fn set_pane_agent_state_records(
        &self,
        machine_id: i64,
        records: Vec<ObservedPaneAgentState>,
    ) {
        self.pane_state_records
            .lock()
            .expect("fake Pane agent state records should remain available")
            .insert(machine_id, records);
    }

    pub(crate) fn set_transcript_capture_failure(
        &self,
        machine_id: i64,
        pane_id: impl Into<String>,
        error: impl Into<String>,
    ) {
        self.transcript_captures
            .lock()
            .expect("fake transcript capture map should remain available")
            .insert((machine_id, pane_id.into()), Err(error.into()));
    }

    pub(crate) fn command_log(&self) -> Arc<Mutex<Vec<FakeTerminalCommand>>> {
        Arc::clone(&self.commands)
    }

    pub(crate) fn fail_launch_release(&self, machine_id: i64, error: impl Into<String>) {
        self.release_failures
            .lock()
            .expect("fake release failures should remain available")
            .insert(machine_id, error.into());
    }

    pub(crate) fn dirty_checkout_after_launch(&self) {
        *self
            .dirty_checkout_on_launch
            .lock()
            .expect("fake launch mutation should remain available") = true;
    }

    pub(crate) fn change_checkout_remote_after_launch(&self, remote_url: impl Into<String>) {
        *self
            .checkout_remote_on_launch
            .lock()
            .expect("fake launch mutation should remain available") = Some(remote_url.into());
    }

    pub(crate) fn set_agent_hook_config(
        &self,
        machine_id: i64,
        agent: AgentKind,
        contents: impl Into<String>,
    ) {
        self.hook_configs
            .lock()
            .expect("fake hook config should remain available")
            .insert((machine_id, agent.slug().into()), contents.into());
    }

    pub(crate) fn agent_hook_config(&self, machine_id: i64, agent: AgentKind) -> Option<String> {
        self.hook_configs
            .lock()
            .expect("fake hook config should remain available")
            .get(&(machine_id, agent.slug().into()))
            .cloned()
    }

    fn outcome(&self, machine: &Machine) -> FakeMachineOutcome {
        self.outcomes
            .get(&machine.id)
            .copied()
            .unwrap_or(FakeMachineOutcome::Unreachable)
    }

    fn record(&self, command: FakeTerminalCommand) {
        self.commands
            .lock()
            .expect("fake terminal command log should remain available")
            .push(command);
    }

    fn available_pane() -> PaneSummary {
        PaneSummary {
            pane_id: "%1".into(),
            pane_index: 0,
            pid: 1,
            columns: 80,
            rows: 24,
            title: "fake pane".into(),
            current_command: "fake-agent".into(),
            current_path: "/fake/worktree".into(),
        }
    }

    fn simulate_hook_provisioning(
        &self,
        machine: &Machine,
        readiness: &mut MachineReadiness,
        run: Option<(AgentKind, i64)>,
    ) {
        if readiness.reachable != Some(true)
            || (run.is_some() && readiness.state_directory_writable != Some(true))
            || self.outcome(machine) == FakeMachineOutcome::HookProvisioningFailed
        {
            return;
        }
        let script = Path::new("/fake/home").join(AGENT_STATE_HOOK_RELATIVE_PATH);
        for agent in [AgentKind::Claude, AgentKind::Codex] {
            let key = (machine.id, agent.slug().to_owned());
            let existing = self
                .hook_configs
                .lock()
                .expect("fake hook config should remain available")
                .get(&key)
                .cloned();
            let result = agent_state::merge_provider_hooks(
                existing.as_deref(),
                &script,
                agent,
                agent_state::agent_hook_events(agent),
            )
            .and_then(|merged| {
                let contents = String::from_utf8(merged).map_err(|error| error.to_string())?;
                self.hook_configs
                    .lock()
                    .expect("fake hook config should remain available")
                    .insert(key, contents);
                Ok(())
            });
            let status = match result {
                Ok(()) => hook_provisioning_success(),
                Err(error) => hook_provisioning_failure(error),
            };
            match agent {
                AgentKind::Claude => readiness.claude_hooks = status,
                AgentKind::Codex => readiness.codex_hooks = status,
            }
        }
        readiness.last_provisioning_error = [
            readiness.claude_hooks.error.as_deref(),
            readiness.codex_hooks.error.as_deref(),
        ]
        .into_iter()
        .flatten()
        .next()
        .map(str::to_owned);
        if readiness.error.is_none() {
            readiness.error = readiness.last_provisioning_error.clone();
        }
    }
}

#[cfg(test)]
impl TerminalRuntime for FakeTerminalRuntime {
    fn attach_connection(
        &self,
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
        _on_output: Box<dyn Fn(Vec<u8>) + Send>,
        _on_agent_state: Box<dyn Fn(AgentStateRecord) + Send>,
        _on_exit: Box<dyn Fn(Option<i32>) + Send>,
    ) -> Result<Box<dyn TerminalConnection>, String> {
        self.record(FakeTerminalCommand::AttachConnection {
            machine_id: machine.id,
            session_name: session_name.into(),
            pane_id: pane_id.into(),
        });
        Ok(Box::new(FakeTerminalConnection {
            machine_id: machine.id,
            pane_id: pane_id.into(),
            commands: Arc::clone(&self.commands),
        }))
    }

    fn preflight_agent_run(
        &self,
        machine: &Machine,
        agent: AgentKind,
        run_id: i64,
        _preferred_executable: Option<&Path>,
        profile: Option<&CliConfigurationProfile>,
    ) -> MachineRunPreflight {
        self.record(FakeTerminalCommand::PreflightAgentRun {
            machine_id: machine.id,
            agent,
            run_id,
            profile: profile.cloned(),
        });
        let mut preflight =
            fake_machine_preflight(machine, self.outcome(machine), Some((agent, run_id)));
        preflight.profile_directory = profile.map(|profile| PathBuf::from(&profile.directory));
        self.simulate_hook_provisioning(machine, &mut preflight.readiness, Some((agent, run_id)));
        preflight
    }

    fn check_machine(&self, machine: &Machine) -> MachineReadiness {
        self.record(FakeTerminalCommand::CheckMachine {
            machine_id: machine.id,
        });
        let mut preflight = fake_machine_preflight(machine, self.outcome(machine), None);
        self.simulate_hook_provisioning(machine, &mut preflight.readiness, None);
        preflight.readiness
    }

    fn observe_machine(&self, machine: &Machine, state_run_ids: &[i64]) -> ObservedMachine {
        self.record(FakeTerminalCommand::ObserveMachine {
            machine_id: machine.id,
        });
        let (state, changed) = &*self.observation_gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        if state.enabled {
            state.started = true;
            changed.notify_all();
            state = changed
                .wait_while(state, |state| !state.released)
                .expect("fake observation gate should remain available");
        }
        drop(state);
        let panes = match self.outcome(machine) {
            FakeMachineOutcome::Available
            | FakeMachineOutcome::AgentUnavailable
            | FakeMachineOutcome::StateDirectoryUnwritable
            | FakeMachineOutcome::HookProvisioningFailed => Ok(self
                .observed_panes
                .lock()
                .expect("fake observed pane list should remain available")
                .get(&machine.id)
                .cloned()
                .unwrap_or_else(|| {
                    vec![ObservedPane {
                        session_name: format!("session-{}", machine.id),
                        pane_id: "%1".into(),
                    }]
                })),
            FakeMachineOutcome::Unreachable | FakeMachineOutcome::TmuxUnavailable => {
                Err(MachineObservationError {
                    kind: MachineObservationFailureKind::Unreachable,
                    message: format!("fake Machine {} is unreachable", machine.name),
                })
            }
            FakeMachineOutcome::TmuxQueryFailed => Err(MachineObservationError {
                kind: MachineObservationFailureKind::TmuxQueryFailed,
                message: format!("fake tmux query failed on Machine {}", machine.name),
            }),
        };
        let requested = state_run_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        self.record(FakeTerminalCommand::ReadAgentStateFiles {
            machine_id: machine.id,
            run_ids: state_run_ids.to_vec(),
        });
        let state_records = if matches!(
            &panes,
            Err(MachineObservationError {
                kind: MachineObservationFailureKind::Unreachable,
                ..
            })
        ) {
            Vec::new()
        } else {
            self.state_file_records
                .lock()
                .expect("fake agent state records should remain available")
                .get(&machine.id)
                .into_iter()
                .flatten()
                .filter(|record| {
                    record
                        .run_id
                        .parse::<i64>()
                        .is_ok_and(|run_id| requested.contains(&run_id))
                })
                .cloned()
                .collect()
        };
        let pane_state_records = self
            .pane_state_records
            .lock()
            .expect("fake Pane agent state records should remain available")
            .get(&machine.id)
            .into_iter()
            .flatten()
            .filter(|pane_state| {
                pane_state
                    .record
                    .run_id
                    .parse::<i64>()
                    .is_ok_and(|run_id| requested.contains(&run_id))
            })
            .cloned()
            .collect();
        ObservedMachine {
            panes,
            pane_state_records,
            state_file_records: state_records,
        }
    }

    fn capture_pane_transcript(&self, machine: &Machine, pane_id: &str) -> Result<Vec<u8>, String> {
        self.record(FakeTerminalCommand::CapturePaneTranscript {
            machine_id: machine.id,
            pane_id: pane_id.into(),
        });
        self.transcript_captures
            .lock()
            .expect("fake transcript capture map should remain available")
            .get(&(machine.id, pane_id.into()))
            .cloned()
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    fn list_panes(
        &self,
        machine: &Machine,
        session_name: &str,
    ) -> Result<Vec<PaneSummary>, String> {
        self.record(FakeTerminalCommand::ListPanes {
            machine_id: machine.id,
            session_name: session_name.into(),
        });
        match self.outcome(machine) {
            FakeMachineOutcome::Available
            | FakeMachineOutcome::AgentUnavailable
            | FakeMachineOutcome::StateDirectoryUnwritable
            | FakeMachineOutcome::HookProvisioningFailed => Ok(vec![Self::available_pane()]),
            FakeMachineOutcome::Unreachable | FakeMachineOutcome::TmuxUnavailable => {
                Err(format!("fake Machine {} is unreachable", machine.name))
            }
            FakeMachineOutcome::TmuxQueryFailed => Err(format!(
                "fake tmux query failed on Machine {}",
                machine.name
            )),
        }
    }

    fn list_agent_panes(&self, machine: &Machine) -> Result<Vec<AgentPaneSummary>, String> {
        self.record(FakeTerminalCommand::ListAgentPanes {
            machine_id: machine.id,
        });
        let (state, changed) = &*self.observation_gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        if state.enabled {
            state.started = true;
            changed.notify_all();
            state = changed
                .wait_while(state, |state| !state.released)
                .expect("fake observation gate should remain available");
        }
        drop(state);
        match self.outcome(machine) {
            FakeMachineOutcome::Available
            | FakeMachineOutcome::AgentUnavailable
            | FakeMachineOutcome::StateDirectoryUnwritable
            | FakeMachineOutcome::HookProvisioningFailed => Ok(Vec::new()),
            FakeMachineOutcome::Unreachable | FakeMachineOutcome::TmuxUnavailable => {
                Err(format!("fake Machine {} is unreachable", machine.name))
            }
            FakeMachineOutcome::TmuxQueryFailed => Err(format!(
                "fake tmux query failed on Machine {}",
                machine.name
            )),
        }
    }

    fn send_pane_input(
        &self,
        machine: &Machine,
        pane_id: &str,
        input: &[u8],
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::SendPaneInput {
            machine_id: machine.id,
            pane_id: pane_id.into(),
            input: input.into(),
        });
        Self::wait_for_gate(&self.send_gate);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        gate_channel: &str,
        root: &Path,
        _executable: &Path,
        _prompt: &str,
        launch: AgentLaunchContext<'_>,
    ) -> Result<String, String> {
        self.record(FakeTerminalCommand::LaunchAgent {
            machine_id: machine.id,
            session_name: session_name.into(),
            gate_channel: gate_channel.into(),
            run_id: launch.run_id,
            profile_directory: launch.profile_directory.map(Path::to_path_buf),
        });
        let dirty_after_launch = *self
            .dirty_checkout_on_launch
            .lock()
            .expect("fake launch mutation should remain available");
        if dirty_after_launch {
            std::fs::write(
                root.join(".fake-terminal-launch-dirt"),
                b"dirty after launch",
            )
            .map_err(|error| format!("Could not simulate post-launch checkout dirt: {error}"))?;
        }
        let remote_after_launch = self
            .checkout_remote_on_launch
            .lock()
            .expect("fake launch mutation should remain available")
            .clone();
        if let Some(remote_url) = remote_after_launch {
            let output = Command::new("git")
                .args(["-C", &root.to_string_lossy(), "remote", "set-url", "origin"])
                .arg(remote_url)
                .output()
                .map_err(|error| {
                    format!("Could not simulate post-launch remote change: {error}")
                })?;
            if !output.status.success() {
                return Err(format!(
                    "Could not simulate post-launch remote change: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        Ok(format!("%fake-{}", launch.run_id))
    }

    fn release_agent_launch(&self, machine: &Machine, gate_channel: &str) -> Result<(), String> {
        self.record(FakeTerminalCommand::ReleaseAgentLaunch {
            machine_id: machine.id,
            gate_channel: gate_channel.into(),
        });
        self.release_failures
            .lock()
            .expect("fake release failures should remain available")
            .get(&machine.id)
            .cloned()
            .map_or(Ok(()), Err)?;
        Self::wait_for_gate(&self.release_gate);
        Ok(())
    }

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String> {
        self.record(FakeTerminalCommand::KillSession {
            machine_id: machine.id,
            session_name: session_name.into(),
        });
        Ok(())
    }

    fn kill_pane(
        &self,
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::KillPane {
            machine_id: machine.id,
            session_name: session_name.into(),
            pane_id: pane_id.into(),
        });
        Ok(())
    }

    fn kill_pane_with_timeout(
        &self,
        machine: &Machine,
        pane_id: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::KillPaneWithTimeout {
            machine_id: machine.id,
            pane_id: pane_id.into(),
            timeout,
        });
        Ok(())
    }

    fn interrupt_pane(
        &self,
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::InterruptPane {
            machine_id: machine.id,
            session_name: session_name.into(),
            pane_id: pane_id.into(),
        });
        Ok(())
    }
}

struct FakeTerminalConnection {
    machine_id: i64,
    pane_id: String,
    commands: Arc<Mutex<Vec<FakeTerminalCommand>>>,
}

impl TerminalConnection for FakeTerminalConnection {
    fn send_input(&self, input: &[u8]) -> Result<(), String> {
        self.record(FakeTerminalCommand::ConnectionInput {
            machine_id: self.machine_id,
            pane_id: self.pane_id.clone(),
            input: input.to_vec(),
        });
        Ok(())
    }

    fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
        self.record(FakeTerminalCommand::ConnectionResize {
            machine_id: self.machine_id,
            pane_id: self.pane_id.clone(),
            columns,
            rows,
        });
        Ok(())
    }

    fn capture_pane_snapshot(
        &self,
        after_capture: Box<dyn FnOnce() + Send>,
    ) -> Result<Vec<u8>, String> {
        self.record(FakeTerminalCommand::ConnectionSnapshot {
            machine_id: self.machine_id,
            pane_id: self.pane_id.clone(),
        });
        after_capture();
        Ok(Vec::new())
    }

    fn close(&self) -> Result<(), String> {
        self.record(FakeTerminalCommand::CloseConnection {
            machine_id: self.machine_id,
            pane_id: self.pane_id.clone(),
        });
        Ok(())
    }
}

impl FakeTerminalConnection {
    fn record(&self, command: FakeTerminalCommand) {
        self.commands.lock().expect("fake calls lock").push(command);
    }
}

#[cfg(test)]
mod connection_tests {
    use super::*;

    #[test]
    fn fake_terminal_connection_covers_snapshot_input_resize_and_close() {
        let runtime = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
        let machine = Machine {
            id: 1,
            context_id: 1,
            name: "test".into(),
            socket_name: "test".into(),
            transport: MachineTransport::Local,
            last_observed: crate::domain::MachineObservation::Unknown,
            last_observed_at: None,
        };
        let connection = runtime
            .attach_connection(
                &machine,
                "session",
                "%1",
                Box::new(|_| {}),
                Box::new(|_| {}),
                Box::new(|_| {}),
            )
            .expect("fake connection should attach");
        connection
            .capture_pane_snapshot(Box::new(|| {}))
            .expect("fake snapshot should be available");
        connection
            .send_input(b"hello")
            .expect("fake input should be accepted");
        connection
            .resize(120, 40)
            .expect("fake resize should be accepted");
        connection.close().expect("fake connection should close");

        assert!(runtime
            .commands()
            .contains(&FakeTerminalCommand::AttachConnection {
                machine_id: 1,
                session_name: "session".into(),
                pane_id: "%1".into(),
            }));
        assert!(runtime
            .commands()
            .contains(&FakeTerminalCommand::ConnectionInput {
                machine_id: 1,
                pane_id: "%1".into(),
                input: b"hello".to_vec(),
            }));
        assert!(runtime
            .commands()
            .contains(&FakeTerminalCommand::ConnectionResize {
                machine_id: 1,
                pane_id: "%1".into(),
                columns: 120,
                rows: 40,
            }));
        assert!(runtime
            .commands()
            .contains(&FakeTerminalCommand::CloseConnection {
                machine_id: 1,
                pane_id: "%1".into(),
            }));
    }
}

#[cfg(test)]
fn fake_machine_preflight(
    machine: &Machine,
    outcome: FakeMachineOutcome,
    run: Option<(AgentKind, i64)>,
) -> MachineRunPreflight {
    let mut readiness = MachineReadiness {
        reachable: Some(true),
        ..MachineReadiness::default()
    };
    if outcome == FakeMachineOutcome::Unreachable {
        readiness.reachable = Some(false);
        readiness.error = Some(format!("Could not reach Machine {}", machine.name));
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }
    if outcome == FakeMachineOutcome::TmuxUnavailable {
        readiness.tmux_available = Some(false);
        readiness.error = Some(format!(
            "Machine {} does not have tmux available",
            machine.name
        ));
        if run.is_some() {
            return MachineRunPreflight {
                readiness,
                ..MachineRunPreflight::default()
            };
        }
    } else {
        readiness.tmux_available = Some(true);
    }

    if let Some((agent, _)) = run {
        set_executable_readiness(
            &mut readiness,
            agent,
            Some(outcome != FakeMachineOutcome::AgentUnavailable),
        );
    }
    if let (FakeMachineOutcome::AgentUnavailable, Some((agent, _))) = (outcome, run) {
        readiness.error = Some(format!(
            "{} executable is unavailable on Machine {}",
            agent_display_name(agent),
            machine.name
        ));
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }

    if outcome == FakeMachineOutcome::StateDirectoryUnwritable {
        readiness.state_directory_writable = Some(false);
        readiness.error = Some(format!(
            "Agent state directory is not writable on Machine {}",
            machine.name
        ));
        if run.is_some() {
            return MachineRunPreflight {
                readiness,
                ..MachineRunPreflight::default()
            };
        }
    } else {
        readiness.state_directory_writable = Some(true);
    }

    if outcome == FakeMachineOutcome::HookProvisioningFailed {
        let error = "fake hook provisioning failed".to_owned();
        readiness.claude_hooks = hook_provisioning_failure(error.clone());
        readiness.codex_hooks = hook_provisioning_failure(error.clone());
        readiness.last_provisioning_error = Some(error.clone());
        readiness.error = Some(error);
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }
    readiness.claude_hooks = hook_provisioning_success();
    readiness.codex_hooks = hook_provisioning_success();
    let state_file =
        run.map(|(_, run_id)| state_file_for_machine_run(Path::new("/fake/home"), run_id));
    let executable = run.map(|(agent, _)| PathBuf::from(format!("/fake/bin/{}", agent.slug())));
    MachineRunPreflight {
        readiness,
        executable,
        state_file,
        profile_directory: None,
    }
}
