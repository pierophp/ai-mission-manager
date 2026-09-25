use super::*;

#[derive(Clone)]
struct UntrackedAgentSnapshot {
    state: DomainState,
    machine: Machine,
    machine_home: String,
    terminal_runtime: Arc<dyn TerminalRuntime>,
}

struct UntrackedAgentObservation {
    snapshot: UntrackedAgentSnapshot,
    canonical: RunSuggestion,
}

impl UntrackedAgentSnapshot {
    fn observe(self, suggestion: &RunSuggestion) -> Result<UntrackedAgentObservation, String> {
        let pane = self
            .terminal_runtime
            .list_agent_panes(&self.machine)?
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
                machine_id: self.machine.id,
                agent: pane.agent,
                session_name: pane.session_name,
                pane_id: pane.pane_id,
                current_path: pane.current_path,
                machine_home: self.machine_home.clone(),
            }],
        )
        .into_iter()
        .find(|candidate| candidate == suggestion)
        .ok_or_else(|| {
            "The suggested agent no longer matches its registered working location".to_owned()
        })?;
        Ok(UntrackedAgentObservation {
            snapshot: self,
            canonical,
        })
    }
}

impl UntrackedAgentObservation {
    fn is_current(&self, runtime: &Runtime) -> bool {
        runtime
            .state
            .machines
            .iter()
            .any(|machine| machine == &self.snapshot.machine)
            && suggest_untracked_runs(
                &runtime.state,
                &[AgentPaneObservation {
                    machine_id: self.canonical.machine_id,
                    agent: self.canonical.agent,
                    session_name: self.canonical.session_name.clone(),
                    pane_id: self.canonical.pane_id.clone(),
                    current_path: self.canonical.current_path.clone(),
                    machine_home: self.snapshot.machine_home.clone(),
                }],
            )
            .into_iter()
            .any(|candidate| candidate == self.canonical)
    }
}

fn untracked_agent_snapshot(
    runtime: &Runtime,
    suggestion: &RunSuggestion,
) -> Result<UntrackedAgentSnapshot, String> {
    let machine = runtime
        .state
        .machines
        .iter()
        .find(|machine| machine.id == suggestion.machine_id)
        .cloned()
        .ok_or_else(|| format!("Machine {} does not exist", suggestion.machine_id))?;
    Ok(UntrackedAgentSnapshot {
        state: runtime.state.clone(),
        machine_home: machine_home_directory(&machine),
        machine,
        terminal_runtime: Arc::clone(&runtime.terminal_runtime),
    })
}

pub(crate) async fn attach_run_with_state(
    suggestion: RunSuggestion,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    let snapshot = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        untracked_agent_snapshot(&runtime, &suggestion)?
    };
    let worker_snapshot = snapshot.clone();
    let observation =
        tauri::async_runtime::spawn_blocking(move || worker_snapshot.observe(&suggestion))
            .await
            .map_err(|error| format!("Agent attachment observation worker failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if !observation.is_current(&runtime) {
        return Err("The suggested agent or its registered working location changed while it was inspected; refresh suggestions".into());
    }
    let canonical = observation.canonical;
    let repository_id = canonical
        .repository_id
        .ok_or_else(|| "The suggested Item execution location has no Repository".to_owned())?;
    let workspace_id = canonical
        .workspace_id
        .ok_or_else(|| "The suggested Item execution location has no Workspace".to_owned())?;
    let decision = decide(
        runtime.state.clone(),
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
            machine_home: observation.snapshot.machine_home,
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
    runtime.commit(decision)?;
    Ok(run)
}

async fn operate_untracked_agent_with_state(
    suggestion: RunSuggestion,
    delete: bool,
    state: &Mutex<Runtime>,
) -> Result<(), String> {
    let snapshot = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        untracked_agent_snapshot(&runtime, &suggestion)?
    };
    let worker_snapshot = snapshot.clone();
    let observation =
        tauri::async_runtime::spawn_blocking(move || worker_snapshot.observe(&suggestion))
            .await
            .map_err(|error| format!("Agent observation worker failed: {error}"))??;
    let terminal_runtime = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        if !observation.is_current(&runtime) {
            return Err("The suggested agent or its registered working location changed while it was inspected; refresh suggestions".into());
        }
        Arc::clone(&runtime.terminal_runtime)
    };
    let machine = observation.snapshot.machine.clone();
    let canonical = observation.canonical.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if delete {
            terminal_runtime.kill_pane(&machine, &canonical.session_name, &canonical.pane_id)
        } else {
            terminal_runtime.interrupt_pane(&machine, &canonical.session_name, &canonical.pane_id)
        }
    })
    .await
    .map_err(|error| format!("Agent control worker failed: {error}"))?
    .map_err(|error| {
        if delete {
            format!("Could not delete the untracked agent Pane: {error}")
        } else {
            format!("Could not stop the untracked agent: {error}")
        }
    })?;
    let runtime = state.lock().map_err(|_| {
        format!(
            "The untracked agent Pane was {}, but Mission Manager state is unavailable",
            if delete { "deleted" } else { "interrupted" }
        )
    })?;
    if !observation.is_current(&runtime) {
        return Err("The agent Pane was controlled, but its registered working location changed before the operation completed".into());
    }
    Ok(())
}

pub(crate) async fn stop_untracked_agent_with_state(
    suggestion: RunSuggestion,
    state: &Mutex<Runtime>,
) -> Result<(), String> {
    operate_untracked_agent_with_state(suggestion, false, state).await
}

pub(crate) async fn delete_untracked_agent_with_state(
    suggestion: RunSuggestion,
    state: &Mutex<Runtime>,
) -> Result<(), String> {
    operate_untracked_agent_with_state(suggestion, true, state).await
}

pub(crate) async fn stop_run_with_state(
    run_id: i64,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    let (run, machine, terminal_runtime) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let run = runtime
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let machine = runtime
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        (run, machine, Arc::clone(&runtime.terminal_runtime))
    };
    let worker_machine = machine.clone();
    let session_name = run.session_name.clone();
    let pane_id = run.pane_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        terminal_runtime.kill_pane(&worker_machine, &session_name, &pane_id)
    })
    .await
    .map_err(|error| format!("Run stop worker failed: {error}"))?
    .map_err(|error| format!("Could not stop Run {run_id}: {error}"))?;
    let mut runtime = state
        .lock()
        .map_err(|_| {
            format!(
                "Run {run_id}'s Pane was stopped, but Mission Manager state is unavailable and the Run may still appear active"
            )
        })?;
    let target_is_current = runtime.state.runs.iter().any(|current| {
        current.id == run.id
            && current.machine_id == run.machine_id
            && current.session_name == run.session_name
            && current.pane_id == run.pane_id
            && current.agent == run.agent
    });
    let machine_is_current = runtime
        .state
        .machines
        .iter()
        .any(|current| current == &machine);
    if !target_is_current || !machine_is_current {
        return Err(format!(
            "Run {run_id}'s Pane was stopped, but its application identity changed before the stop could be recorded"
        ));
    }
    let decision = decide(
        runtime.state.clone(),
        Event::SetRunPaneStatus {
            run_id,
            status: RunPaneStatus::Missing,
        },
    )
    .map_err(|error| {
        format!("Run {run_id}'s Pane was stopped, but its status could not be updated: {error}")
    })?;
    let stopped = decision
        .state
        .runs
        .iter()
        .find(|candidate| candidate.id == run_id)
        .cloned()
        .ok_or_else(|| "Run stop produced no Run".to_owned())?;
    runtime
        .commit_with_audit(decision, &[AuditAction::RunStopped { run_id }])
        .map_err(|error| {
            format!(
                "Run {run_id}'s Pane was stopped, but its status could not be persisted: {error}"
            )
        })?;
    Ok(stopped)
}

#[derive(Clone)]
struct GrillPaneSnapshot {
    run: Run,
    machine: Machine,
}

impl GrillPaneSnapshot {
    fn is_current(&self, runtime: &Runtime) -> bool {
        runtime
            .state
            .runs
            .iter()
            .any(|current| current == &self.run)
            && runtime
                .state
                .machines
                .iter()
                .any(|current| current == &self.machine)
    }
}

pub(crate) async fn recover_run_states_with_state(state: &Mutex<Runtime>) -> Result<(), String> {
    let snapshots = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.recover_run_state_records()?;
        std::mem::take(&mut runtime.pending_grill_transcript_captures)
            .into_iter()
            .filter_map(|run_id| {
                let run = runtime
                    .state
                    .runs
                    .iter()
                    .find(|run| run.id == run_id)?
                    .clone();
                if run.execution_profile != ExecutionProfile::Grill
                    || !matches!(run.state, RunState::Blocked | RunState::Finished)
                {
                    return None;
                }
                let machine = runtime
                    .state
                    .machines
                    .iter()
                    .find(|machine| machine.id == run.machine_id)?
                    .clone();
                Some((
                    GrillPaneSnapshot { run, machine },
                    Arc::clone(&runtime.terminal_runtime),
                ))
            })
            .collect::<Vec<_>>()
    };
    for (snapshot, terminal_runtime) in snapshots {
        if snapshot.run.execution_profile != ExecutionProfile::Grill {
            continue;
        }
        let worker_snapshot = snapshot.clone();
        let transcript = tauri::async_runtime::spawn_blocking(move || {
            terminal_runtime
                .capture_pane_transcript(&worker_snapshot.machine, &worker_snapshot.run.pane_id)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })
        .await
        .map_err(|error| format!("Grill transcript capture worker failed: {error}"))?;
        let transcript = match transcript {
            Ok(transcript) => transcript,
            Err(error) => {
                eprintln!(
                    "Could not retain transcript for waiting Grill Run {}: {error}",
                    snapshot.run.id
                );
                continue;
            }
        };
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        if snapshot.is_current(&runtime) {
            runtime.apply_grill_transcript(snapshot.run.id, transcript)?;
        }
    }
    Ok(())
}

pub(crate) async fn list_run_suggestions_with_state(
    state: &Mutex<Runtime>,
) -> Result<Vec<RunSuggestion>, String> {
    recover_run_states_with_state(state).await?;
    let (snapshot, machines, terminal_runtime) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let machines = runtime
            .state
            .machines
            .iter()
            .filter(|machine| {
                runtime
                    .state
                    .contexts
                    .iter()
                    .any(|context| context.execution_machine_id == Some(machine.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        (
            runtime.state.clone(),
            machines,
            Arc::clone(&runtime.terminal_runtime),
        )
    };
    let worker_runtime = Arc::clone(&terminal_runtime);
    let observations = tauri::async_runtime::spawn_blocking(move || {
        let mut observations = Vec::new();
        for machine in machines {
            match worker_runtime.list_agent_panes(&machine) {
                Ok(panes) => {
                    observations.extend(panes.into_iter().map(|pane| AgentPaneObservation {
                        machine_id: machine.id,
                        agent: pane.agent,
                        session_name: pane.session_name,
                        pane_id: pane.pane_id,
                        current_path: pane.current_path,
                        machine_home: machine_home_directory(&machine),
                    }))
                }
                Err(error) => eprintln!(
                    "Could not inspect Machine {} for agent Panes: {error}",
                    machine.name
                ),
            }
        }
        observations
    })
    .await
    .map_err(|error| format!("Run suggestion worker failed: {error}"))?;
    let suggestions = suggest_untracked_runs(&snapshot, &observations);
    let runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if runtime.state != snapshot {
        return Err("Run or Machine state changed while agent Panes were inspected; refresh suggestions again".into());
    }
    Ok(suggestions)
}

pub(crate) async fn submit_grill_answers_with_state(
    run_id: i64,
    answers: Vec<GrillAnswer>,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    let operation_lock = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.grill_operation_lock(run_id)
    };
    let operation_guard = operation_lock.lock_owned().await;
    let (snapshot, input, response, terminal_runtime) = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let run = runtime
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
            runtime.state.clone(),
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
        runtime.commit(answers_decision)?;
        let machine = runtime
            .state
            .machines
            .iter()
            .find(|machine| machine.id == answered_run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", answered_run.machine_id))?;
        let mut input = response.as_bytes().to_vec();
        input.push(b'\n');
        (
            GrillPaneSnapshot {
                run: answered_run,
                machine,
            },
            input,
            response,
            Arc::clone(&runtime.terminal_runtime),
        )
    };
    let worker_snapshot = snapshot.clone();
    let (send_result, _operation_guard) = tauri::async_runtime::spawn_blocking(move || {
        let result = terminal_runtime.send_pane_input(
            &worker_snapshot.machine,
            &worker_snapshot.run.pane_id,
            &input,
        );
        (result, operation_guard)
    })
    .await
    .map_err(|error| format!("Grill answer send worker failed: {error}"))?;
    send_result.map_err(|error| error.to_string())?;
    let mut runtime = state
        .lock()
        .map_err(|_| {
            format!(
                "The Grill response for Run {run_id} was sent, but Mission Manager state is unavailable and its Run record may be incomplete"
            )
        })?;
    if !snapshot.is_current(&runtime) {
        return Err(format!(
            "Run {run_id} changed while its Grill response was being sent; the response may already have reached the agent"
        ));
    }
    let working = decide(
        runtime.state.clone(),
        Event::UpdateRunState {
            run_id,
            state: RunState::Working,
        },
    )
    .map_err(|error| error.to_string())?;
    runtime.commit(working).map_err(|error| {
        format!("The Grill response for Run {run_id} was sent, but its Working state could not be persisted: {error}")
    })?;
    let decision = decide(
        runtime.state.clone(),
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
    runtime.commit(decision).map_err(|error| {
        format!("The Grill response for Run {run_id} was sent, but its response record could not be persisted: {error}")
    })?;
    Ok(submitted_run)
}

pub(crate) async fn continue_grill_with_state(
    run_id: i64,
    action: GrillContinuationAction,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    let operation_lock = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.grill_operation_lock(run_id)
    };
    let operation_guard = operation_lock.lock_owned().await;
    // Taken before the prompt is sent, so every Issue the action creates is
    // newer than it.
    let started_at = crate::app::current_unix_seconds();
    let (snapshot, prompt, terminal_runtime) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let run = runtime
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let prompt = build_grill_continuation_prompt(&runtime.state, run_id, action)
            .map_err(|error| error.to_string())?;
        decide(
            runtime.state.clone(),
            Event::ContinueGrill {
                run_id,
                action,
                started_at,
            },
        )
        .map_err(|error| error.to_string())?;
        let machine = runtime
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        (
            GrillPaneSnapshot { run, machine },
            prompt,
            Arc::clone(&runtime.terminal_runtime),
        )
    };
    let worker_snapshot = snapshot.clone();
    let (send_result, _operation_guard) = tauri::async_runtime::spawn_blocking(move || {
        let mut input = prompt.into_bytes();
        input.push(b'\n');
        let result = terminal_runtime.send_pane_input(
            &worker_snapshot.machine,
            &worker_snapshot.run.pane_id,
            &input,
        );
        (result, operation_guard)
    })
    .await
    .map_err(|error| format!("Grill continuation send worker failed: {error}"))?;
    send_result.map_err(|error| error.to_string())?;
    let mut runtime = state
        .lock()
        .map_err(|_| {
            format!(
                "The Grill continuation for Run {run_id} was sent, but Mission Manager state is unavailable and its Run record may be incomplete"
            )
        })?;
    if !snapshot.is_current(&runtime) {
        return Err(format!(
            "Run {run_id} changed while its Grill continuation was being sent; the continuation may already have reached the agent"
        ));
    }
    let decision = decide(
        runtime.state.clone(),
        Event::ContinueGrill {
            run_id,
            action,
            started_at,
        },
    )
    .map_err(|error| error.to_string())?;
    let continued = decision
        .state
        .runs
        .iter()
        .find(|candidate| candidate.id == run_id)
        .cloned()
        .ok_or_else(|| format!("Run {run_id} does not exist"))?;
    runtime.commit(decision).map_err(|error| {
        format!("The Grill continuation for Run {run_id} was sent, but its state could not be persisted: {error}")
    })?;
    Ok(continued)
}
use std::sync::Arc;

use crate::terminal::{MachineObservationError, ObservedMachine};

fn read_run_state_record(
    run_id: i64,
    legacy_agent_state_directory: &Path,
) -> Option<AgentStateRecord> {
    let legacy_path = state_file_path(legacy_agent_state_directory, run_id);
    match read_state_file(&legacy_path) {
        Ok(record) => Some(record),
        Err(error) => {
            if legacy_path.exists() {
                eprintln!(
                    "Could not recover legacy state for Run {run_id} from {}: {error}",
                    legacy_path.display()
                );
            }
            None
        }
    }
}

fn confirm_downstream_issues(
    run_id: i64,
    action: GrillContinuationAction,
    action_started_at: Option<i64>,
    transcript: &str,
    executable: impl FnOnce() -> Option<PathBuf>,
) -> Vec<ConfirmedDownstreamIssue> {
    if !matches!(
        action,
        GrillContinuationAction::ToSpec | GrillContinuationAction::ToTickets
    ) {
        return Vec::new();
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
        return Vec::new();
    }
    let Some(executable) = executable() else {
        return Vec::new();
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
        if !downstream_issue_is_new(&snapshot, action_started_at) {
            continue;
        }
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
    confirmed
}

fn downstream_confirmation_transcript<'a>(
    live_transcript: Option<&'a Result<String, String>>,
    saved_transcript: &'a str,
) -> Option<&'a str> {
    match live_transcript {
        Some(Ok(transcript)) => Some(transcript),
        Some(Err(_)) if !saved_transcript.trim().is_empty() => Some(saved_transcript),
        _ => None,
    }
}

fn grill_transcript_identity_matches(
    snapshot_action: &Option<GrillContinuationAction>,
    snapshot_transcript: &str,
    current_action: &Option<GrillContinuationAction>,
    current_transcript: &str,
) -> bool {
    snapshot_action == current_action && snapshot_transcript == current_transcript
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentStateApplication {
    pub accepted: bool,
    pub state_changed: bool,
    pub run_changed: bool,
}

impl AgentStateApplication {
    const IGNORED: Self = Self {
        accepted: false,
        state_changed: false,
        run_changed: false,
    };
}

pub(crate) struct ReconciliationSnapshot {
    runs: Vec<ReconciliationRunIdentity>,
    machines: Vec<Machine>,
    terminal_runtime: Arc<dyn crate::terminal::TerminalRuntime>,
    gh_executable_path: Option<PathBuf>,
}

#[derive(Clone)]
struct ReconciliationRunIdentity {
    id: i64,
    machine_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    session_name: String,
    pane_id: String,
    grill_action: Option<GrillContinuationAction>,
    grill_action_started_at: Option<i64>,
    transcript: String,
}

struct MachineReconciliationObservation {
    machine_id: i64,
    machine_name: String,
    observation: ObservedMachine,
}

struct RunReconciliationObservation {
    run: ReconciliationRunIdentity,
    pane_status: Option<RunPaneStatus>,
    state_record: Option<AgentStateRecord>,
    transcript: Option<Result<String, String>>,
    confirmed_downstream_issues: Vec<ConfirmedDownstreamIssue>,
}

pub(crate) struct ReconciliationObservations {
    machines: Vec<MachineReconciliationObservation>,
    runs: Vec<RunReconciliationObservation>,
}

pub(crate) struct ReconciliationApply {
    pub result: RunReconciliationResult,
    pub changed_runs: Vec<RunStateChangedEvent>,
}

impl ReconciliationSnapshot {
    pub(crate) fn observe(self) -> ReconciliationObservations {
        let machines = self.observe_machines();

        let runs = self
            .runs
            .iter()
            .map(|run| {
                let machine_observation = machines
                    .iter()
                    .find(|observation| observation.machine_id == run.machine_id);
                let state_record = machine_observation
                    .map(|observation| observed_state_record(&observation.observation, run));
                let state_record = state_record.flatten();
                let pane_status = observed_pane_status(
                    machines
                        .iter()
                        .find(|observation| observation.machine_id == run.machine_id),
                    run,
                );
                let should_capture_transcript = run.execution_profile == ExecutionProfile::Grill
                    && pane_status.is_some()
                    && (pane_status == Some(RunPaneStatus::Available)
                        || state_record.as_ref().is_some_and(|record| {
                            matches!(record.state, RunState::Blocked | RunState::Finished)
                        }));
                let transcript = if should_capture_transcript {
                    self.machines
                        .iter()
                        .find(|machine| machine.id == run.machine_id)
                        .map(|machine| {
                            self.terminal_runtime
                                .capture_pane_transcript(machine, &run.pane_id)
                                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                        })
                } else {
                    None
                };
                let confirmation_transcript =
                    downstream_confirmation_transcript(transcript.as_ref(), &run.transcript);
                let confirmed_downstream_issues = match (run.grill_action, confirmation_transcript)
                {
                    (Some(action), Some(transcript)) => confirm_downstream_issues(
                        run.id,
                        action,
                        run.grill_action_started_at,
                        transcript,
                        || {
                            self.gh_executable_path
                                .as_deref()
                                .map(PathBuf::from)
                                .or_else(|| resolve_gh_executable(None).ok())
                        },
                    ),
                    _ => Vec::new(),
                };
                RunReconciliationObservation {
                    run: run.clone(),
                    pane_status,
                    state_record,
                    transcript,
                    confirmed_downstream_issues,
                }
            })
            .collect();
        ReconciliationObservations { machines, runs }
    }

    fn observe_machines(&self) -> Vec<MachineReconciliationObservation> {
        let mut machines = Vec::new();
        let mut observed_machine_ids = HashSet::new();
        for run in &self.runs {
            if !observed_machine_ids.insert(run.machine_id) {
                continue;
            }
            let Some(machine) = self
                .machines
                .iter()
                .find(|machine| machine.id == run.machine_id)
            else {
                continue;
            };
            let mut state_run_ids = self
                .runs
                .iter()
                .filter(|candidate| candidate.machine_id == machine.id)
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>();
            state_run_ids.sort_unstable();
            state_run_ids.dedup();
            machines.push(MachineReconciliationObservation {
                machine_id: machine.id,
                machine_name: machine.name.clone(),
                observation: self
                    .terminal_runtime
                    .observe_machine(machine, &state_run_ids),
            });
        }
        machines
    }
}

fn observed_pane_status(
    machine: Option<&MachineReconciliationObservation>,
    run: &ReconciliationRunIdentity,
) -> Option<RunPaneStatus> {
    match machine.map(|observation| &observation.observation.panes) {
        Some(Ok(panes))
            if panes.iter().any(|pane| {
                pane.session_name == run.session_name && pane.pane_id == run.pane_id
            }) =>
        {
            Some(RunPaneStatus::Available)
        }
        Some(Ok(panes)) if panes.iter().any(|pane| pane.pane_id == run.pane_id) => None,
        Some(Ok(_)) => Some(RunPaneStatus::Missing),
        Some(Err(_)) | None => Some(RunPaneStatus::Unknown),
    }
}

fn observed_state_record(
    observation: &ObservedMachine,
    run: &ReconciliationRunIdentity,
) -> Option<AgentStateRecord> {
    if matches!(
        &observation.panes,
        Err(MachineObservationError {
            kind: crate::terminal::MachineObservationFailureKind::Unreachable,
            ..
        })
    ) {
        return None;
    }
    let pane_record = observation.pane_state_records.iter().filter(|pane_state| {
        pane_state.session_name == run.session_name
            && pane_state.pane_id == run.pane_id
            && pane_state.record.run_id.parse::<i64>().ok() == Some(run.id)
            && pane_state.record.agent == run.agent
            && pane_state
                .record
                .sequence
                .is_none_or(|sequence| sequence >= 0)
    });
    let file_record = observation.state_file_records.iter().filter(|record| {
        record.run_id.parse::<i64>().ok() == Some(run.id)
            && record.agent == run.agent
            && record.sequence.is_none_or(|sequence| sequence >= 0)
    });
    let pane_record = pane_record
        .map(|pane_state| &pane_state.record)
        .max_by_key(|record| record.sequence.unwrap_or(-1));
    let file_record = file_record.max_by_key(|record| record.sequence.unwrap_or(-1));
    newer_agent_state_record(pane_record, file_record).cloned()
}

fn newer_agent_state_record<'a>(
    pane_record: Option<&'a AgentStateRecord>,
    file_record: Option<&'a AgentStateRecord>,
) -> Option<&'a AgentStateRecord> {
    match (pane_record, file_record) {
        (Some(pane), Some(file)) => {
            if file.sequence > pane.sequence {
                Some(file)
            } else {
                Some(pane)
            }
        }
        (Some(pane), None) => Some(pane),
        (None, Some(file)) => Some(file),
        (None, None) => None,
    }
}

struct TerminalOpenSnapshot {
    run: Run,
    machine: Machine,
    terminal_runtime: Arc<dyn crate::terminal::TerminalRuntime>,
    terminal_id: String,
    generation: u64,
    session_name: String,
    pane_id: String,
}

struct TerminalOpenObservation {
    run: Run,
    machine: Machine,
    terminal_id: String,
    generation: u64,
    session_name: String,
    pane_id: String,
    snapshot: Vec<u8>,
    panes: Vec<PaneTab>,
    transcript: Option<Result<String, String>>,
    connection: TmuxControlPane,
    callback_gate: Arc<TerminalCallbackGate>,
}

#[derive(Default)]
struct TerminalCallbackGateState {
    active: bool,
    snapshot_captured: bool,
    pending_state_records: Vec<AgentStateRecord>,
    pending_terminal_events: Vec<DeferredTerminalEvent>,
}

pub(crate) enum DeferredTerminalEvent {
    Output(Vec<u8>),
    Exit(Option<i32>),
}

#[derive(Default)]
pub(crate) struct TerminalCallbackGate {
    state: Mutex<TerminalCallbackGateState>,
    terminal_event_dispatch: Mutex<()>,
    state_dispatch: Mutex<()>,
}

impl TerminalCallbackGate {
    pub(crate) fn dispatch_output_or_queue(&self, data: Vec<u8>, dispatch: impl FnOnce(Vec<u8>)) {
        let _dispatch_guard = self.lock_terminal_event_dispatch();
        let dispatch_data = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.active {
                Some(data)
            } else if state.snapshot_captured {
                state
                    .pending_terminal_events
                    .push(DeferredTerminalEvent::Output(data));
                None
            } else {
                None
            }
        };
        if let Some(data) = dispatch_data {
            dispatch(data);
        }
        // Output received before the snapshot barrier is represented by the
        // returned pane snapshot. It must not be replayed on top of it.
    }

    pub(crate) fn mark_snapshot_captured(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .pending_terminal_events
            .retain(|event| !matches!(event, DeferredTerminalEvent::Output(_)));
        state.snapshot_captured = true;
    }

    fn dispatch_exit_or_queue(&self, code: Option<i32>, dispatch: impl FnOnce(Option<i32>)) {
        let _dispatch_guard = self.lock_terminal_event_dispatch();
        let dispatch_now = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.active {
                true
            } else {
                state
                    .pending_terminal_events
                    .push(DeferredTerminalEvent::Exit(code));
                false
            }
        };
        if dispatch_now {
            dispatch(code);
        }
    }

    fn queue_state_record_or_dispatch(&self, record: AgentStateRecord) -> Option<AgentStateRecord> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active {
            Some(record)
        } else {
            state.pending_state_records.push(record);
            None
        }
    }

    fn lock_state_dispatch(&self) -> std::sync::MutexGuard<'_, ()> {
        self.state_dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn lock_terminal_event_dispatch(&self) -> std::sync::MutexGuard<'_, ()> {
        self.terminal_event_dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn dispatch_state_or_queue(
        &self,
        record: AgentStateRecord,
        dispatch: impl FnOnce(AgentStateRecord),
    ) {
        let _dispatch_guard = self.lock_state_dispatch();
        if let Some(record) = self.queue_state_record_or_dispatch(record) {
            dispatch(record);
        }
    }

    pub(crate) fn activate_and_drain(&self) -> (Vec<AgentStateRecord>, Vec<DeferredTerminalEvent>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active = true;
        (
            std::mem::take(&mut state.pending_state_records),
            std::mem::take(&mut state.pending_terminal_events),
        )
    }
}

fn apply_terminal_state_record_to_runtime(
    runtime: &mut Runtime,
    app: &AppHandle,
    record: &AgentStateRecord,
) -> Option<i64> {
    let run_id = record.run_id.parse::<i64>().ok()?;
    match runtime.apply_agent_state_record(run_id, record.clone()) {
        Ok(application) => {
            if application.state_changed {
                let _ = app.emit(
                    "run-state-changed",
                    RunStateChangedEvent {
                        run_id,
                        state: record.state,
                    },
                );
            }
            (application.accepted && matches!(record.state, RunState::Blocked | RunState::Finished))
                .then_some(run_id)
        }
        Err(error) => {
            eprintln!("Could not persist state for Run {run_id}: {error}");
            None
        }
    }
}

fn reconcile_after_terminal_state(app: &AppHandle, run_id: i64, run_is_grill: bool) {
    let reconcile_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(runtime_state) = reconcile_app.try_state::<Mutex<Runtime>>() else {
            return;
        };
        if let Err(error) = crate::features::work::reconcile_runs_with_state(
            runtime_state.inner(),
            Some(reconcile_app.clone()),
        )
        .await
        {
            eprintln!("Could not reconcile Run {run_id} after its state changed: {error}");
        } else if run_is_grill {
            let _ = reconcile_app.emit("run-questions-changed", run_id);
        }
    });
}

fn apply_terminal_state_record(app: &AppHandle, run_is_grill: bool, record: AgentStateRecord) {
    let Some(app_state) = app.try_state::<Mutex<Runtime>>() else {
        return;
    };
    let accepted_run_id = {
        let Ok(mut runtime) = app_state.lock() else {
            return;
        };
        apply_terminal_state_record_to_runtime(&mut runtime, app, &record)
    };
    if let Some(run_id) = accepted_run_id {
        reconcile_after_terminal_state(app, run_id, run_is_grill);
    }
}

impl Runtime {
    fn begin_terminal_open_request(&mut self, terminal_id: &str) -> u64 {
        let generation = self
            .terminal_open_request_generations
            .entry(terminal_id.to_owned())
            .or_insert(0);
        *generation = generation
            .checked_add(1)
            .expect("Terminal open request generation should not overflow");
        *generation
    }

    pub(crate) fn invalidate_terminal_open_request(&mut self, terminal_id: &str) {
        self.begin_terminal_open_request(terminal_id);
    }

    fn terminal_open_request_is_current(&self, terminal_id: &str, generation: u64) -> bool {
        self.terminal_open_request_generations
            .get(terminal_id)
            .is_some_and(|current| *current == generation)
    }

    fn terminal_connection_is_current(&self, terminal_id: &str, generation: u64) -> bool {
        self.terminal_open_request_is_current(terminal_id, generation)
            && self
                .terminal_connection_generations
                .get(terminal_id)
                .is_some_and(|current| *current == generation)
    }
}

impl TerminalOpenSnapshot {
    fn observe(self, app: &AppHandle) -> Result<TerminalOpenObservation, String> {
        let panes = self
            .terminal_runtime
            .list_panes(&self.machine, &self.session_name)?;
        if !panes.iter().any(|pane| pane.pane_id == self.pane_id) {
            return Err(format!(
                "Pane {} is not available in session {}",
                self.pane_id, self.session_name
            ));
        }

        let output_app = app.clone();
        let callback_gate = Arc::new(TerminalCallbackGate::default());
        let output_gate = Arc::clone(&callback_gate);
        let state_gate = Arc::clone(&output_gate);
        let exit_gate = Arc::clone(&output_gate);
        let output_terminal_id = self.terminal_id.clone();
        let output_pane_id = self.pane_id.clone();
        let exit_app = app.clone();
        let exit_terminal_id = self.terminal_id.clone();
        let exit_pane_id = self.pane_id.clone();
        let output_generation = self.generation;
        let exit_generation = self.generation;
        let state_app = app.clone();
        let run_is_grill = self.run.execution_profile == ExecutionProfile::Grill;
        let connection = TmuxControlPane::attach(
            &self.machine,
            &self.session_name,
            &self.pane_id,
            move |data| {
                output_gate.dispatch_output_or_queue(data, |data| {
                    let _ = output_app.emit(
                        "terminal-output",
                        TerminalOutputEvent {
                            terminal_id: output_terminal_id.clone(),
                            generation: output_generation,
                            pane_id: output_pane_id.clone(),
                            data,
                        },
                    );
                });
            },
            move |record| {
                state_gate.dispatch_state_or_queue(record, |record| {
                    apply_terminal_state_record(&state_app, run_is_grill, record);
                });
            },
            move |code| {
                exit_gate.dispatch_exit_or_queue(code, |code| {
                    let _ = exit_app.emit(
                        "terminal-exit",
                        TerminalExitEvent {
                            terminal_id: exit_terminal_id.clone(),
                            generation: exit_generation,
                            pane_id: exit_pane_id.clone(),
                            code,
                        },
                    );
                });
            },
        )?;

        let snapshot_gate = Arc::clone(&callback_gate);
        let snapshot = match connection.capture_pane_snapshot(move || {
            snapshot_gate.mark_snapshot_captured();
        }) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.close();
                return Err(error);
            }
        };
        let pane_summaries = match self
            .terminal_runtime
            .list_panes(&self.machine, &self.session_name)
        {
            Ok(panes) => panes,
            Err(error) => {
                let _ = connection.close();
                return Err(error);
            }
        };
        if !pane_summaries
            .iter()
            .any(|pane| pane.pane_id == self.pane_id)
        {
            let _ = connection.close();
            return Err(format!(
                "Pane {} is no longer available in session {}",
                self.pane_id, self.session_name
            ));
        }
        let panes = pane_summaries
            .into_iter()
            .map(|pane| PaneTab::from_summary(&self.run, &self.session_name, pane, true))
            .collect();
        let transcript = (self.run.execution_profile == ExecutionProfile::Grill).then(|| {
            self.terminal_runtime
                // A sibling tab can be selected for display, but Grill state
                // belongs to the Run's agent pane and must only read that pane.
                .capture_pane_transcript(&self.machine, &self.run.pane_id)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        });
        Ok(TerminalOpenObservation {
            run: self.run,
            machine: self.machine,
            terminal_id: self.terminal_id,
            generation: self.generation,
            session_name: self.session_name,
            pane_id: self.pane_id,
            snapshot,
            panes,
            transcript,
            connection,
            callback_gate,
        })
    }
}

pub(crate) async fn open_terminal_with_state(
    app: &AppHandle,
    run_id: i64,
    terminal_id: String,
    session_name: String,
    pane_id: String,
    state: &Mutex<Runtime>,
) -> Result<TerminalAttachment, String> {
    let snapshot = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        if terminal_id.trim().is_empty() {
            return Err("A terminal identity is required".to_owned());
        }
        let generation = runtime.begin_terminal_open_request(&terminal_id);
        // The user may choose a sibling Pane in this Run's session, so the
        // request is scoped by Run/session here and the selected Pane is
        // verified against the live session during observation.
        let run = runtime
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id && run.session_name == session_name)
            .cloned()
            .ok_or_else(|| "The Pane does not belong to that Run".to_owned())?;
        let machine = runtime
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        TerminalOpenSnapshot {
            run,
            machine,
            terminal_runtime: Arc::clone(&runtime.terminal_runtime),
            terminal_id,
            generation,
            session_name,
            pane_id,
        }
    };
    let worker_app = app.clone();
    let observation = tauri::async_runtime::spawn_blocking(move || snapshot.observe(&worker_app))
        .await
        .map_err(|error| format!("Terminal open worker failed: {error}"))??;
    let mut connection = Some(observation.connection);
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    let current_run = runtime
        .state
        .runs
        .iter()
        .find(|run| run.id == observation.run.id);
    let current_machine = runtime
        .state
        .machines
        .iter()
        .find(|machine| machine.id == observation.machine.id);
    let identities_match = current_run.is_some_and(|run| {
        run.machine_id == observation.run.machine_id
            && run.session_name == observation.run.session_name
            && run.pane_id == observation.run.pane_id
            && run.execution_profile == observation.run.execution_profile
    }) && current_machine.is_some_and(|machine| {
        machine.context_id == observation.machine.context_id
            && machine.name == observation.machine.name
            && machine.socket_name == observation.machine.socket_name
            && machine.transport == observation.machine.transport
    });
    if !identities_match
        || !runtime
            .terminal_open_request_is_current(&observation.terminal_id, observation.generation)
    {
        drop(runtime);
        let _ = connection
            .expect("new terminal connection should remain owned")
            .close();
        return Err(
            "The Run, Machine, or terminal request changed while opening; open it again".into(),
        );
    }
    let current_run = current_run.expect("Run identity was validated").clone();
    if current_run.pane_status != RunPaneStatus::Available {
        if !runtime
            .terminal_open_request_is_current(&observation.terminal_id, observation.generation)
        {
            drop(runtime);
            let _ = connection
                .take()
                .expect("new terminal connection should remain owned")
                .close();
            return Err("A newer terminal open superseded this request".into());
        }
        let decision = decide(
            runtime.state.clone(),
            Event::SetRunPaneStatus {
                run_id: observation.run.id,
                status: RunPaneStatus::Available,
            },
        )
        .map_err(|error| error.to_string())?;
        if let Err(error) = runtime.commit(decision) {
            drop(runtime);
            let _ = connection
                .expect("new terminal connection should remain owned")
                .close();
            return Err(error);
        }
    }
    if observation.transcript.is_some()
        && !runtime
            .terminal_open_request_is_current(&observation.terminal_id, observation.generation)
    {
        drop(runtime);
        let _ = connection
            .take()
            .expect("new terminal connection should remain owned")
            .close();
        return Err("A newer terminal open superseded this request".into());
    }
    let transcript_identity_matches = observation.transcript.is_none()
        || grill_transcript_identity_matches(
            &observation.run.grill_action,
            &observation.run.transcript,
            &current_run.grill_action,
            &current_run.transcript,
        );
    if !transcript_identity_matches {
        eprintln!(
            "Skipping captured Grill transcript for Run {} because its action or saved transcript changed while opening",
            observation.run.id
        );
    }
    let questions_changed = if transcript_identity_matches {
        match observation.transcript {
            Some(Ok(transcript)) => {
                match runtime.apply_grill_transcript_record(observation.run.id, transcript) {
                    Ok(changed) => changed,
                    Err(error) => {
                        eprintln!(
                            "Could not capture transcript while reopening Grill Run {}: {error}",
                            observation.run.id
                        );
                        false
                    }
                }
            }
            Some(Err(error)) => {
                eprintln!(
                    "Could not capture transcript while reopening Grill Run {}: {error}",
                    observation.run.id
                );
                false
            }
            None => false,
        }
    } else {
        false
    };
    if !runtime.terminal_open_request_is_current(&observation.terminal_id, observation.generation) {
        drop(runtime);
        let _ = connection
            .take()
            .expect("new terminal connection should remain owned")
            .close();
        return Err("A newer terminal open superseded this request".into());
    }
    let previous = runtime.terminal_connections.insert(
        observation.terminal_id.clone(),
        Arc::new(
            connection
                .take()
                .expect("new terminal connection should remain owned"),
        ),
    );
    runtime
        .terminal_connection_generations
        .insert(observation.terminal_id.clone(), observation.generation);
    let callback_gate = Arc::clone(&observation.callback_gate);
    let run_is_grill = observation.run.execution_profile == ExecutionProfile::Grill;
    let run_id = observation.run.id;
    let attachment = TerminalAttachment {
        terminal_id: observation.terminal_id,
        generation: observation.generation,
        session_name: observation.session_name,
        pane_id: observation.pane_id,
        snapshot: observation.snapshot,
        panes: observation.panes,
    };
    drop(runtime);
    if let Some(previous) = previous {
        let _ = previous.close();
    }
    let terminal_event_guard = callback_gate.lock_terminal_event_dispatch();
    let state_dispatch_guard = callback_gate.lock_state_dispatch();
    let (activated, pending_state_records, pending_terminal_events, stale_connection) = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        if runtime.terminal_connection_is_current(&attachment.terminal_id, observation.generation) {
            let (state_records, terminal_events) = callback_gate.activate_and_drain();
            (true, state_records, terminal_events, None)
        } else if runtime
            .terminal_connection_generations
            .get(&attachment.terminal_id)
            .is_some_and(|generation| *generation == observation.generation)
        {
            runtime
                .terminal_connection_generations
                .remove(&attachment.terminal_id);
            (
                false,
                Vec::new(),
                Vec::new(),
                runtime.terminal_connections.remove(&attachment.terminal_id),
            )
        } else {
            (false, Vec::new(), Vec::new(), None)
        }
    };
    if let Some(stale_connection) = stale_connection {
        let _ = stale_connection.close();
    }
    if !activated {
        drop(state_dispatch_guard);
        drop(terminal_event_guard);
        return Err("A newer terminal open superseded this request".into());
    }
    for event in pending_terminal_events {
        match event {
            DeferredTerminalEvent::Output(data) => {
                let _ = app.emit(
                    "terminal-output",
                    TerminalOutputEvent {
                        terminal_id: attachment.terminal_id.clone(),
                        generation: attachment.generation,
                        pane_id: attachment.pane_id.clone(),
                        data,
                    },
                );
            }
            DeferredTerminalEvent::Exit(code) => {
                let _ = app.emit(
                    "terminal-exit",
                    TerminalExitEvent {
                        terminal_id: attachment.terminal_id.clone(),
                        generation: attachment.generation,
                        pane_id: attachment.pane_id.clone(),
                        code,
                    },
                );
            }
        }
    }
    for record in pending_state_records {
        apply_terminal_state_record(app, run_is_grill, record);
    }
    drop(state_dispatch_guard);
    drop(terminal_event_guard);
    if questions_changed {
        let _ = app.emit("run-questions-changed", run_id);
    }
    Ok(attachment)
}

pub(crate) async fn open_external_terminal_with_state(
    run_id: i64,
    state: &Mutex<Runtime>,
) -> Result<(), String> {
    let (run, machine, terminal_runtime) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let run = runtime
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| format!("Run {run_id} does not exist"))?;
        let machine = runtime
            .state
            .machines
            .iter()
            .find(|machine| machine.id == run.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", run.machine_id))?;
        (run, machine, Arc::clone(&runtime.terminal_runtime))
    };
    let observed_run = run.clone();
    let observed_machine = machine.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let panes = terminal_runtime
            .list_panes(&observed_machine, &observed_run.session_name)
            .map_err(|error| {
                format!(
                    "Pane {} is not available in session {}: {error}",
                    observed_run.pane_id, observed_run.session_name
                )
            })?;
        if !panes
            .iter()
            .any(|pane| pane.pane_id == observed_run.pane_id)
        {
            return Err(format!(
                "Pane {} is not available in session {}",
                observed_run.pane_id, observed_run.session_name
            ));
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("External terminal worker failed: {error}"))??;
    let identities_match = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let current_run = runtime
            .state
            .runs
            .iter()
            .find(|current| current.id == run.id);
        let current_machine = runtime
            .state
            .machines
            .iter()
            .find(|current| current.id == machine.id);
        current_run.is_some_and(|current| {
            current.machine_id == run.machine_id
                && current.session_name == run.session_name
                && current.pane_id == run.pane_id
        }) && current_machine.is_some_and(|current| {
            current.context_id == machine.context_id
                && current.name == machine.name
                && current.socket_name == machine.socket_name
                && current.transport == machine.transport
        })
    };
    if !identities_match {
        return Err("The Run or Machine changed while its terminal was opening".into());
    }
    let identity = ExternalPaneIdentity {
        tmux_path: "tmux".into(),
        socket_name: machine.socket_name.clone(),
        session_name: run.session_name.clone(),
        pane_id: run.pane_id.clone(),
        transport: terminal_transport(&machine),
    };
    tauri::async_runtime::spawn_blocking(move || open_pane_in_terminal(&identity))
        .await
        .map_err(|error| format!("External terminal worker failed: {error}"))??;
    Ok(())
}

#[derive(Clone)]
struct DirectCheckoutSnapshot {
    item_id: i64,
    item_project_id: i64,
    workspace: Workspace,
    machine_id_request: Option<i64>,
    machine: Machine,
    repositories: Vec<(Repository, RepositoryLocation, PathBuf)>,
}

struct DirectCheckoutObservation {
    snapshot: DirectCheckoutSnapshot,
    checkouts: Vec<DirectRunCheckoutPreview>,
}

fn direct_repository_identity_set_matches(
    item_project_id: i64,
    snapshot_repositories: &[(Repository, RepositoryLocation, PathBuf)],
    current_repositories: &[Repository],
) -> bool {
    let expected = snapshot_repositories
        .iter()
        .map(|(repository, _, _)| repository.id)
        .collect::<std::collections::HashSet<_>>();
    let current = current_repositories
        .iter()
        .filter(|repository| repository.project_id == item_project_id)
        .map(|repository| repository.id)
        .collect::<std::collections::HashSet<_>>();
    expected == current
}

impl DirectCheckoutSnapshot {
    fn observe(self) -> Result<DirectCheckoutObservation, String> {
        let git = GitCli::system();
        let mut checkouts = Vec::with_capacity(self.repositories.len());
        for (repository, _, path) in &self.repositories {
            let inspection = git
                .inspect_checkout_on_machine(&self.machine, path)
                .map_err(|error| {
                    format!(
                        "Could not inspect Repository {} on Machine {}: {error}",
                        repository.name, self.machine.name
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
            checkouts.push(DirectRunCheckoutPreview {
                repository_id: repository.id,
                repository_name: repository.name.clone(),
                path: path.to_string_lossy().into_owned(),
                branch: inspection.current_branch,
                is_dirty: inspection.is_dirty,
            });
        }
        Ok(DirectCheckoutObservation {
            snapshot: self,
            checkouts,
        })
    }
}

fn direct_checkout_snapshot(
    runtime: &mut Runtime,
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
) -> Result<DirectCheckoutSnapshot, String> {
    let workspace = runtime
        .state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id && workspace.item_id == item_id)
        .cloned()
        .ok_or_else(|| {
            format!("Project Repository execution setup does not belong to Item {item_id}")
        })?;
    let item = runtime
        .state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .cloned()
        .ok_or_else(|| format!("Item {item_id} does not exist"))?;
    let machine = runtime.machine_for_item(item_id, machine_id)?;
    let machine_home = machine_home_directory(&machine);
    let repositories = runtime
        .state
        .repositories
        .iter()
        .filter(|repository| repository.project_id == item.project_id)
        .cloned()
        .collect::<Vec<_>>();
    if repositories.is_empty() {
        return Err("Register a Repository under this Project before starting a Run".into());
    }
    let mut inputs = Vec::with_capacity(repositories.len());
    for repository in repositories {
        let location = runtime
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == repository.id && location.machine_id == machine.id
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Repository {} has no checkout registered on Machine {}",
                    repository.name, machine.name
                )
            })?;
        let path = match machine.transport {
            MachineTransport::Local => resolve_machine_path(&location.checkout_path, &machine_home),
            MachineTransport::Ssh { .. } => PathBuf::from(&location.checkout_path),
        };
        inputs.push((repository, location, path));
    }
    Ok(DirectCheckoutSnapshot {
        item_id,
        item_project_id: item.project_id,
        workspace,
        machine_id_request: machine_id,
        machine,
        repositories: inputs,
    })
}

impl Runtime {
    fn direct_checkout_snapshot_is_current(&self, snapshot: &DirectCheckoutSnapshot) -> bool {
        let Some(item) = self
            .state
            .items
            .iter()
            .find(|item| item.id == snapshot.item_id)
        else {
            return false;
        };
        if item.project_id != snapshot.item_project_id
            || !self
                .state
                .workspaces
                .iter()
                .any(|current| current == &snapshot.workspace)
            || !self.state.machines.iter().any(|current| {
                current.id == snapshot.machine.id
                    && current.context_id == snapshot.machine.context_id
                    && current.name == snapshot.machine.name
                    && current.socket_name == snapshot.machine.socket_name
                    && current.transport == snapshot.machine.transport
            })
        {
            return false;
        }
        let Some(context) = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == snapshot.machine.context_id)
        else {
            return false;
        };
        if context.execution_machine_id != Some(snapshot.machine.id)
            || snapshot
                .machine_id_request
                .is_some_and(|requested| requested != snapshot.machine.id)
            || !direct_repository_identity_set_matches(
                snapshot.item_project_id,
                &snapshot.repositories,
                &self.state.repositories,
            )
        {
            return false;
        }
        snapshot
            .repositories
            .iter()
            .all(|(repository, location, _)| {
                self.state
                    .repositories
                    .iter()
                    .any(|current| current == repository)
                    && self
                        .state
                        .repository_locations
                        .iter()
                        .any(|current| current == location)
            })
    }

    fn apply_direct_checkout_observation(
        &self,
        observation: DirectCheckoutObservation,
    ) -> Result<DirectRunPreview, String> {
        if !self.direct_checkout_snapshot_is_current(&observation.snapshot) {
            return Err("The Item, Repository, or Machine changed while checkout state was being inspected; review the preview again".into());
        }
        build_direct_run_preview(
            &self.state,
            observation.snapshot.workspace.id,
            observation.snapshot.machine,
            observation.checkouts,
        )
    }
}

fn build_direct_run_preview(
    state: &crate::domain::DomainState,
    workspace_id: i64,
    machine: Machine,
    checkout_details: Vec<DirectRunCheckoutPreview>,
) -> Result<DirectRunPreview, String> {
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
    for run in state.runs.iter().filter(|run| {
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

pub(crate) async fn prepare_direct_run_with_state(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: &Mutex<Runtime>,
) -> Result<DirectRunPreview, String> {
    let snapshot = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        direct_checkout_snapshot(&mut runtime, item_id, workspace_id, machine_id)?
    };
    let observation = tauri::async_runtime::spawn_blocking(move || snapshot.observe())
        .await
        .map_err(|error| format!("Direct Run preview worker failed: {error}"))??;
    let runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    runtime.apply_direct_checkout_observation(observation)
}

#[derive(Clone)]
enum RunLaunchInput {
    Direct {
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
    },
    Grill {
        item_id: i64,
        workspace_id: i64,
        machine_id: Option<i64>,
        primary_repository_id: i64,
        configuration: GrillConfiguration,
        initial_prompt: String,
        expected_checkouts: Vec<RunCheckout>,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    Worktree {
        item_id: i64,
        workspace_id: i64,
        worktree_id: i64,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
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

    fn expected_checkouts(&self) -> Option<&[RunCheckout]> {
        match self {
            Self::Direct {
                expected_checkouts, ..
            }
            | Self::Grill {
                expected_checkouts, ..
            } => Some(expected_checkouts),
            Self::Worktree { .. } => None,
        }
    }

    fn session_kind(&self) -> &'static str {
        match self {
            Self::Grill { .. } => "grill",
            Self::Direct { .. } | Self::Worktree { .. } => "run",
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
    terminal_runtime: Arc<dyn TerminalRuntime>,
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
                execution_profile,
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
                Event::StartDirectRun {
                    item_id: *item_id,
                    workspace_id: *workspace_id,
                    machine_id: self.machine.id,
                    agent: *agent,
                    execution_profile: *execution_profile,
                    prompt: self.prompt.clone(),
                    working_directory,
                    session_name,
                    pane_id,
                    started_at: self.started_at,
                    prompt_selection: prompt_selection.clone(),
                    checkouts,
                    repository_id: *primary_repository_id,
                    allow_dirty: *allow_dirty,
                    allow_shared_checkouts: *allow_shared_checkouts,
                }
            }
            RunLaunchInput::Grill {
                item_id,
                workspace_id,
                primary_repository_id,
                configuration,
                ..
            } => {
                let working_directory = checkouts
                    .iter()
                    .find(|checkout| checkout.repository_id == *primary_repository_id)
                    .map(|checkout| checkout.path.clone())
                    .unwrap_or_default();
                Event::StartGrillRun {
                    item_id: *item_id,
                    workspace_id: *workspace_id,
                    repository_id: *primary_repository_id,
                    machine_id: self.machine.id,
                    configuration: configuration.clone(),
                    prompt: self.prompt.clone(),
                    skill_snapshot: crate::domain::grill_skill_snapshot().into(),
                    working_directory,
                    session_name,
                    pane_id,
                    started_at: self.started_at,
                    checkouts,
                }
            }
            RunLaunchInput::Worktree {
                item_id,
                workspace_id,
                worktree_id,
                agent,
                execution_profile,
                prompt_selection,
                ..
            } => Event::StartWorktreeRun {
                item_id: *item_id,
                workspace_id: *workspace_id,
                worktree_id: *worktree_id,
                machine_id: self.machine.id,
                agent: *agent,
                execution_profile: *execution_profile,
                prompt: self.prompt.clone(),
                working_directory: self
                    .worktree
                    .as_ref()
                    .map(|identity| identity.worktree.path.clone())
                    .unwrap_or_default(),
                session_name,
                pane_id,
                started_at: self.started_at,
                prompt_selection: prompt_selection.clone(),
            },
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
                if &checkouts != expected_checkouts {
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
                let checkout_path = match self.machine.transport {
                    MachineTransport::Local => resolve_machine_path(
                        &identity.worktree.path,
                        &machine_home_directory(&self.machine),
                    ),
                    MachineTransport::Ssh { .. } => PathBuf::from(&identity.worktree.path),
                };
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
        let event = self.event(
            format!("{}-preflight", self.session_name),
            "%preflight".into(),
            checkouts.clone(),
        );
        if let RunLaunchInput::Grill {
            allow_dirty,
            allow_shared_checkouts,
            ..
        } = &self.input
        {
            validate_grill_launch_approvals(
                &self.state,
                self.machine.id,
                &checkouts,
                *allow_dirty,
                *allow_shared_checkouts,
            )?;
        }
        decide(self.state.clone(), event).map_err(|error| error.to_string())?;
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

fn validate_grill_launch_approvals(
    state: &DomainState,
    machine_id: i64,
    checkouts: &[RunCheckout],
    allow_dirty: bool,
    allow_shared_checkouts: bool,
) -> Result<(), String> {
    if !allow_dirty {
        let dirty_repository_ids = checkouts
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
        let shared_paths = state
            .runs
            .iter()
            .filter(|run| {
                run.machine_id == machine_id
                    && run_is_active(run)
                    && run.pane_status != RunPaneStatus::Missing
            })
            .flat_map(|run| {
                checkouts.iter().filter_map(move |checkout| {
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
    Ok(())
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
        RunLaunchInput::Grill {
            configuration,
            initial_prompt,
            ..
        } => build_grill_prompt(
            &runtime.state,
            input.item_id(),
            configuration,
            initial_prompt,
        )
        .map_err(|error| error.to_string())?,
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
        terminal_runtime: Arc::clone(&runtime.terminal_runtime),
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
    let checkout_values = snapshot
        .input
        .expected_checkouts()
        .unwrap_or_default()
        .to_vec();
    if let RunLaunchInput::Grill {
        allow_dirty,
        allow_shared_checkouts,
        ..
    } = &snapshot.input
    {
        validate_grill_launch_approvals(
            &runtime.state,
            snapshot.machine.id,
            &checkout_values,
            *allow_dirty,
            *allow_shared_checkouts,
        )?;
    }
    let candidate = snapshot.event(
        format!("{}-preflight", snapshot.session_name),
        "%preflight".into(),
        checkout_values,
    );
    decide(runtime.state.clone(), candidate).map_err(|error| error.to_string())?;
    Ok(snapshot)
}

async fn start_run_with_state(
    input: RunLaunchInput,
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
        terminal_runtime.preflight_agent_run(
            &machine,
            preflight_snapshot.input.agent(),
            preflight_snapshot.run_id,
            preferred_executable.as_deref(),
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
        _ => (observation.snapshot.input.agent(), None, None),
    };
    let run_id = observation.snapshot.run_id;
    let terminal_runtime = Arc::clone(&observation.snapshot.terminal_runtime);
    let machine = observation.snapshot.machine.clone();
    let session_name = observation.snapshot.session_name.clone();
    let gate_channel = observation.snapshot.gate_channel.clone();
    let working_directory = observation.working_directory.clone();
    let prompt = observation.snapshot.prompt.clone();
    let pane_id = tauri::async_runtime::spawn_blocking(move || {
        let launch = AgentLaunchContext {
            run_id,
            state_file: &state_file,
            agent,
            model: model.as_deref(),
            effort: effort.as_deref(),
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
                let approval = match &observation.snapshot.input {
                    RunLaunchInput::Grill {
                        allow_dirty,
                        allow_shared_checkouts,
                        ..
                    } => validate_grill_launch_approvals(
                        &runtime.state,
                        observation.snapshot.machine.id,
                        &observation.checkouts,
                        *allow_dirty,
                        *allow_shared_checkouts,
                    ),
                    _ => Ok(()),
                };
                match approval {
                    Err(error) => Err(error),
                    Ok(()) => {
                        let decision = decide(
                            runtime.state.clone(),
                            observation.snapshot.event(
                                observation.snapshot.session_name.clone(),
                                pane_id,
                                observation.checkouts.clone(),
                            ),
                        )
                        .map_err(|error| error.to_string());
                        match decision {
                            Ok(decision) => {
                                let run = decision
                                    .state
                                    .runs
                                    .last()
                                    .cloned()
                                    .ok_or_else(|| "Run creation produced no Run".to_owned());
                                match run {
                                    Ok(run) => match runtime.commit(decision) {
                                        Ok(()) => Ok(run),
                                        Err(error) => Err(error),
                                    },
                                    Err(error) => Err(error),
                                }
                            }
                            Err(error) => Err(error),
                        }
                    }
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
pub(crate) async fn start_direct_run_with_state(
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
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    start_run_with_state(
        RunLaunchInput::Direct {
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
        },
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_grill_run_with_state(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    configuration: GrillConfiguration,
    initial_prompt: String,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    start_run_with_state(
        RunLaunchInput::Grill {
            item_id,
            workspace_id,
            machine_id,
            primary_repository_id,
            configuration,
            initial_prompt,
            expected_checkouts,
            allow_dirty,
            allow_shared_checkouts,
        },
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_worktree_run_with_state(
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    state: &Mutex<Runtime>,
) -> Result<Run, String> {
    start_run_with_state(
        RunLaunchInput::Worktree {
            item_id,
            workspace_id,
            worktree_id,
            agent,
            execution_profile,
            prompt,
            prompt_selection,
        },
        state,
    )
    .await
}

impl Runtime {
    pub(crate) fn reconciliation_snapshot(&self) -> ReconciliationSnapshot {
        let runs = self
            .state
            .runs
            .iter()
            .map(|run| ReconciliationRunIdentity {
                id: run.id,
                machine_id: run.machine_id,
                agent: run.agent,
                execution_profile: run.execution_profile,
                session_name: run.session_name.clone(),
                pane_id: run.pane_id.clone(),
                grill_action: run.grill_action,
                grill_action_started_at: run.grill_action_started_at,
                transcript: run.transcript.clone(),
            })
            .collect::<Vec<_>>();
        let mut machine_ids = HashSet::new();
        let machines = runs
            .iter()
            .filter_map(|run| {
                if !machine_ids.insert(run.machine_id) {
                    return None;
                }
                self.state
                    .machines
                    .iter()
                    .find(|machine| machine.id == run.machine_id)
                    .cloned()
            })
            .collect();
        ReconciliationSnapshot {
            runs,
            machines,
            terminal_runtime: Arc::clone(&self.terminal_runtime),
            gh_executable_path: self.gh_executable_path.clone(),
        }
    }

    pub(crate) fn apply_reconciliation(
        &mut self,
        observations: ReconciliationObservations,
    ) -> Result<ReconciliationApply, String> {
        let mut changed_run_ids = HashSet::new();
        let mut any_changed = false;
        let mut failures = Vec::new();
        for observation in &observations.machines {
            if let Err(error) = &observation.observation.panes {
                failures.push(MachineObservationFailure {
                    machine_id: observation.machine_id,
                    machine_name: observation.machine_name.clone(),
                    kind: error.kind,
                    message: error.message.clone(),
                });
            }
        }

        for observation in observations.runs {
            let Some(current) = self
                .state
                .runs
                .iter()
                .find(|run| run.id == observation.run.id)
            else {
                continue;
            };
            if current.machine_id != observation.run.machine_id
                || current.session_name != observation.run.session_name
                || current.pane_id != observation.run.pane_id
            {
                continue;
            }

            if let Some(record) = observation.state_record {
                if record.run_id.parse::<i64>().ok() == Some(observation.run.id)
                    && record.agent == observation.run.agent
                    && self
                        .apply_agent_state_record(observation.run.id, record)?
                        .run_changed
                {
                    changed_run_ids.insert(observation.run.id);
                    any_changed = true;
                }
            }

            let current_pane_status = self
                .state
                .runs
                .iter()
                .find(|run| run.id == observation.run.id)
                .map(|run| run.pane_status);
            if observation
                .pane_status
                .is_some_and(|status| current_pane_status.is_some_and(|current| current != status))
            {
                let pane_status = observation.pane_status.expect("pane status was checked");
                let decision = decide(
                    self.state.clone(),
                    Event::SetRunPaneStatus {
                        run_id: observation.run.id,
                        status: pane_status,
                    },
                )
                .map_err(|error| error.to_string())?;
                self.commit(decision)?;
                changed_run_ids.insert(observation.run.id);
                any_changed = true;
            }

            if let Some(transcript) = observation.transcript {
                let transcript_identity_is_current = self
                    .state
                    .runs
                    .iter()
                    .find(|run| run.id == observation.run.id)
                    .is_some_and(|current| {
                        current.grill_action == observation.run.grill_action
                            && current.transcript == observation.run.transcript
                    });
                if !transcript_identity_is_current {
                    continue;
                }
                match transcript {
                    Ok(transcript) => {
                        let (run_changed, domain_changed) = self
                            .apply_grill_transcript_with_confirmed_issues(
                                observation.run.id,
                                transcript,
                                observation.run.grill_action,
                                observation.confirmed_downstream_issues,
                            )?;
                        if run_changed {
                            changed_run_ids.insert(observation.run.id);
                        }
                        any_changed |= domain_changed;
                    }
                    Err(error) => {
                        eprintln!(
                            "Could not reconcile transcript for Grill Run {}: {error}",
                            observation.run.id
                        );
                        let downstream_changed = self.apply_confirmed_downstream_issues(
                            observation.run.id,
                            observation.run.grill_action,
                            observation.confirmed_downstream_issues,
                        )?;
                        any_changed |= downstream_changed;
                    }
                }
            }
        }

        let changed_runs = changed_run_ids
            .into_iter()
            .filter_map(|run_id| {
                self.state
                    .runs
                    .iter()
                    .find(|run| run.id == run_id)
                    .map(|run| RunStateChangedEvent {
                        run_id: run.id,
                        state: run.state,
                    })
            })
            .collect::<Vec<_>>();

        Ok(ReconciliationApply {
            result: RunReconciliationResult {
                failures,
                changed: any_changed || !changed_runs.is_empty(),
            },
            changed_runs,
        })
    }

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

    #[cfg(test)]
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
        let transcript = self
            .terminal_runtime
            .capture_pane_transcript(&machine, &run.pane_id)
            .map(|transcript| String::from_utf8_lossy(&transcript).into_owned());
        let transcript = transcript?;
        self.apply_grill_transcript(run_id, transcript)
    }

    pub(crate) fn apply_grill_transcript(
        &mut self,
        run_id: i64,
        transcript: String,
    ) -> Result<bool, String> {
        self.apply_grill_transcript_record(run_id, transcript)
    }

    fn apply_grill_transcript_with_confirmed_issues(
        &mut self,
        run_id: i64,
        transcript: String,
        expected_action: Option<GrillContinuationAction>,
        confirmed: Vec<ConfirmedDownstreamIssue>,
    ) -> Result<(bool, bool), String> {
        let changed = self.apply_grill_transcript_record(run_id, transcript)?;
        let downstream_changed =
            self.apply_confirmed_downstream_issues(run_id, expected_action, confirmed)?;
        Ok((changed, changed || downstream_changed))
    }

    fn apply_grill_transcript_record(
        &mut self,
        run_id: i64,
        transcript: String,
    ) -> Result<bool, String> {
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
        let transcript_extends_previous = grill_transcript_extends(&run.transcript, &transcript);
        let captured_question_group =
            if run.grill_decisions.is_empty() && run.grill_response.is_none() {
                parse_grill_question_group(&transcript)
            } else {
                parse_grill_question_group_since(&run.transcript, &transcript)
            };
        let question_group = match (run.grill_question_group.as_ref(), captured_question_group) {
            (Some(previous), None)
                if run.state == RunState::Working || run.grill_response.is_none() =>
            {
                Some(previous.clone())
            }
            (Some(previous), Some(captured))
                if run.grill_response.is_none()
                    && previous
                        .questions
                        .last()
                        .zip(captured.questions.first())
                        .is_some_and(|(previous, next)| next.number > previous.number) =>
            {
                let mut combined = previous.clone();
                combined.questions.extend(captured.questions);
                Some(combined)
            }
            (Some(previous), Some(captured))
                if previous.questions.len() == captured.questions.len()
                    && run.grill_response.is_none()
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
            (Some(previous), Some(mut captured))
                if run.grill_response.is_some() && !transcript_extends_previous =>
            {
                if captured.questions.starts_with(&previous.questions) {
                    captured.questions.drain(..previous.questions.len());
                    if captured.questions.is_empty() {
                        Some(previous.clone())
                    } else {
                        captured.round = run.grill_decisions.len();
                        Some(captured)
                    }
                } else if previous.questions.starts_with(&captured.questions) {
                    Some(previous.clone())
                } else {
                    captured.round = run.grill_decisions.len();
                    Some(captured)
                }
            }
            (_, captured) => captured.map(|mut group| {
                group.round = run.grill_decisions.len();
                group
            }),
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
            return Ok(false);
        }
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
        Ok(true)
    }

    fn apply_confirmed_downstream_issues(
        &mut self,
        run_id: i64,
        expected_action: Option<GrillContinuationAction>,
        confirmed: Vec<ConfirmedDownstreamIssue>,
    ) -> Result<bool, String> {
        if confirmed.is_empty() {
            return Ok(false);
        }
        let Some(run) = self.state.runs.iter().find(|run| run.id == run_id) else {
            return Ok(false);
        };
        if run.grill_action != expected_action {
            return Ok(false);
        }
        let Some(action @ (GrillContinuationAction::ToSpec | GrillContinuationAction::ToTickets)) =
            run.grill_action
        else {
            return Ok(false);
        };
        let decision = decide(
            self.state.clone(),
            Event::CaptureDownstreamIssues {
                run_id,
                action,
                issues: confirmed,
            },
        )
        .map_err(|error| error.to_string())?;
        let changed = decision.state != self.state;
        if changed {
            self.commit(decision)?;
        }
        Ok(changed)
    }

    #[cfg(test)]
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

        let confirmed = confirm_downstream_issues(
            run_id,
            action,
            run.grill_action_started_at,
            transcript,
            || match self.gh_executable_path() {
                Ok(executable) => Some(executable),
                Err(error) => {
                    eprintln!(
                        "Could not confirm downstream GitHub Issues for Run {run_id}: {error}"
                    );
                    None
                }
            },
        );
        self.apply_confirmed_downstream_issues(run_id, Some(action), confirmed)
            .map(|_| ())
    }

    #[cfg(test)]
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

        self.terminal_runtime
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

    #[cfg(test)]
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
        let panes = self
            .terminal_runtime
            .list_panes(&machine, &run.session_name)
            .map_err(|error| {
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

    pub(crate) fn recover_run_state_records(&mut self) -> Result<(), String> {
        for run in self.state.runs.clone() {
            let record = read_run_state_record(run.id, &self.legacy_agent_state_directory);
            let Some(record) = record else {
                continue;
            };
            let Ok(record_run_id) = record.run_id.parse::<i64>() else {
                continue;
            };
            if record_run_id != run.id || record.agent != run.agent {
                continue;
            }
            let application = self.apply_agent_state_record(record_run_id, record.clone())?;
            if application.accepted
                && matches!(record.state, RunState::Blocked | RunState::Finished)
                && run.execution_profile == ExecutionProfile::Grill
            {
                self.pending_grill_transcript_captures.insert(record_run_id);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn recover_run_states(&mut self) -> Result<(), String> {
        self.recover_run_state_records()?;
        let run_ids = std::mem::take(&mut self.pending_grill_transcript_captures);
        for run_id in run_ids {
            if let Err(error) = self.capture_grill_transcript(run_id) {
                eprintln!(
                    "Could not retain transcript for waiting Grill Run {}: {error}",
                    run_id
                );
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn reconcile_runs(&mut self) -> Result<RunReconciliationResult, String> {
        let snapshot = self.reconciliation_snapshot();
        let observations = snapshot.observe();
        Ok(self.apply_reconciliation(observations)?.result)
    }

    pub(crate) fn apply_agent_state_record(
        &mut self,
        run_id: i64,
        record: AgentStateRecord,
    ) -> Result<AgentStateApplication, String> {
        let Some(run) = self.state.runs.iter().find(|run| run.id == run_id) else {
            return Ok(AgentStateApplication::IGNORED);
        };
        if record.run_id.parse::<i64>().ok() != Some(run_id) || run.agent != record.agent {
            return Ok(AgentStateApplication::IGNORED);
        }
        let accepted = match record.sequence {
            Some(sequence) => {
                sequence >= 0
                    && run
                        .last_applied_agent_state_sequence
                        .is_none_or(|last| sequence > last)
            }
            None => run.last_applied_agent_state_sequence.is_none(),
        };
        if !accepted {
            return Ok(AgentStateApplication::IGNORED);
        }
        let state_changed = run.state != record.state;
        let previous_state = run.state;
        let previous_sequence = run.last_applied_agent_state_sequence;
        let previous_grill_phase = run.grill_phase;
        let decision = decide(
            self.state.clone(),
            Event::ApplyAgentStateReport {
                run_id,
                state: record.state,
                sequence: record.sequence,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        if matches!(record.state, RunState::Blocked | RunState::Finished)
            && self
                .state
                .runs
                .iter()
                .find(|run| run.id == run_id)
                .is_some_and(|run| run.execution_profile == ExecutionProfile::Grill)
        {
            self.pending_grill_transcript_captures.insert(run_id);
        }
        let run_changed = self
            .state
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .is_some_and(|run| {
                run.state != previous_state
                    || run.last_applied_agent_state_sequence != previous_sequence
                    || run.grill_phase != previous_grill_phase
            });
        Ok(AgentStateApplication {
            accepted: true,
            state_changed,
            run_changed,
        })
    }
}

fn agent_display_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "Claude Code",
        AgentKind::Codex => "Codex",
    }
}

#[cfg(test)]
mod identity_tests {
    use super::{
        direct_repository_identity_set_matches, downstream_confirmation_transcript,
        grill_transcript_identity_matches, DeferredTerminalEvent, TerminalCallbackGate,
    };
    use crate::domain::{Repository, RepositoryLocation};
    use crate::{
        agent_state::AgentStateRecord,
        app::Runtime,
        domain::{AgentKind, GrillContinuationAction, RunState},
    };
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    #[test]
    fn direct_checkout_snapshot_rejects_a_new_project_repository() {
        let first = Repository {
            id: 1,
            project_id: 1,
            name: "service-a".into(),
            remote_url: "https://example.com/service-a.git".into(),
            base_branch: "main".into(),
        };
        let second = Repository {
            id: 2,
            project_id: 1,
            name: "service-b".into(),
            remote_url: "https://example.com/service-b.git".into(),
            base_branch: "main".into(),
        };
        let location = RepositoryLocation {
            repository_id: first.id,
            machine_id: 1,
            checkout_path: "/checkouts/service-a".into(),
            worktree_root: "/worktrees".into(),
        };
        let snapshot = vec![(
            first.clone(),
            location,
            PathBuf::from("/checkouts/service-a"),
        )];

        assert!(direct_repository_identity_set_matches(
            1,
            &snapshot,
            std::slice::from_ref(&first)
        ));
        assert!(!direct_repository_identity_set_matches(
            1,
            &snapshot,
            &[first, second]
        ));
    }

    #[test]
    fn downstream_confirmation_uses_saved_transcript_when_live_capture_fails() {
        let failed_capture = Err("pane no longer exists".to_owned());

        assert_eq!(
            downstream_confirmation_transcript(Some(&failed_capture), "saved GitHub Issue URL"),
            Some("saved GitHub Issue URL")
        );
        assert_eq!(
            downstream_confirmation_transcript(Some(&failed_capture), " \n "),
            None
        );
    }

    #[test]
    fn grill_transcript_observation_is_stale_when_action_or_saved_transcript_changes() {
        let action = Some(GrillContinuationAction::ToTickets);
        assert!(grill_transcript_identity_matches(
            &action,
            "saved transcript",
            &action,
            "saved transcript"
        ));
        assert!(!grill_transcript_identity_matches(
            &action,
            "saved transcript",
            &Some(GrillContinuationAction::ToSpec),
            "saved transcript"
        ));
        assert!(!grill_transcript_identity_matches(
            &action,
            "old transcript",
            &action,
            "new transcript"
        ));
    }

    #[test]
    fn late_terminal_open_cannot_win_after_a_newer_request_started() {
        let directory = tempfile::tempdir().expect("temporary app directory should exist");
        let mut runtime =
            Runtime::open(directory.path().join("mission.sqlite")).expect("runtime should open");

        let first = runtime.begin_terminal_open_request("run-7");
        let second = runtime.begin_terminal_open_request("run-7");

        assert!(second > first);
        assert!(!runtime.terminal_open_request_is_current("run-7", first));
        assert!(runtime.terminal_open_request_is_current("run-7", second));
        assert!(!runtime.terminal_connection_is_current("run-7", first));

        runtime.invalidate_terminal_open_request("run-7");
        assert!(!runtime.terminal_open_request_is_current("run-7", second));
    }

    #[test]
    fn terminal_callback_gate_drains_initial_state_and_post_snapshot_output_once() {
        let gate = TerminalCallbackGate::default();
        let record = AgentStateRecord {
            agent: AgentKind::Claude,
            run_id: "7".into(),
            state: RunState::Working,
            updated_at: "1".into(),
            sequence: Some(1),
        };

        assert!(gate
            .queue_state_record_or_dispatch(record.clone())
            .is_none());
        gate.dispatch_output_or_queue(vec![1], |_| panic!("pre-snapshot output is dropped"));
        gate.mark_snapshot_captured();
        let mut events = Vec::new();
        gate.dispatch_output_or_queue(vec![2], |_| panic!("inactive output should queue"));
        gate.dispatch_exit_or_queue(Some(0), |_| panic!("inactive exit should queue"));
        gate.dispatch_output_or_queue(vec![3], |_| panic!("inactive output should queue"));

        let _event_dispatch_guard = gate.lock_terminal_event_dispatch();
        let (records, terminal_events) = gate.activate_and_drain();
        for event in terminal_events {
            match event {
                DeferredTerminalEvent::Output(data) => events.push(format!("output:{}", data[0])),
                DeferredTerminalEvent::Exit(code) => events.push(format!("exit:{code:?}")),
            }
        }
        assert_eq!(records, vec![record.clone()]);
        assert_eq!(events, vec!["output:2", "exit:Some(0)", "output:3"]);
        drop(_event_dispatch_guard);

        assert_eq!(
            gate.queue_state_record_or_dispatch(record.clone()),
            Some(record)
        );
        gate.dispatch_output_or_queue(vec![4], |data| {
            events.push(format!("output:{}", data[0]));
        });
        gate.dispatch_exit_or_queue(None, |code| events.push(format!("exit:{code:?}")));
        let (records, events_after_activation) = gate.activate_and_drain();
        assert!(records.is_empty());
        assert!(events_after_activation.is_empty());
        assert_eq!(
            events,
            vec![
                "output:2",
                "exit:Some(0)",
                "output:3",
                "output:4",
                "exit:None"
            ]
        );
    }

    #[test]
    fn unsequenced_state_callbacks_run_after_the_initial_state_drain() {
        let gate = Arc::new(TerminalCallbackGate::default());
        let applied_states = Arc::new(Mutex::new(Vec::new()));
        let queued = AgentStateRecord {
            agent: AgentKind::Claude,
            run_id: "7".into(),
            state: RunState::Blocked,
            updated_at: "1".into(),
            sequence: None,
        };
        let later = AgentStateRecord {
            state: RunState::Finished,
            updated_at: "2".into(),
            ..queued.clone()
        };
        assert!(gate.queue_state_record_or_dispatch(queued).is_none());

        let dispatch_guard = gate.lock_state_dispatch();
        let (initial_records, initial_events) = gate.activate_and_drain();
        assert!(initial_events.is_empty());
        let callback_gate = Arc::clone(&gate);
        let callback_states = Arc::clone(&applied_states);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let callback = std::thread::spawn(move || {
            started_sender
                .send(())
                .expect("test thread should remain available");
            callback_gate.dispatch_state_or_queue(later, |record| {
                callback_states
                    .lock()
                    .expect("applied state list should remain available")
                    .push(record.state);
            });
        });
        started_receiver
            .recv()
            .expect("live callback should start before the drain is applied");

        for record in initial_records {
            applied_states
                .lock()
                .expect("applied state list should remain available")
                .push(record.state);
        }
        drop(dispatch_guard);
        callback.join().expect("live callback should complete");

        assert_eq!(
            *applied_states
                .lock()
                .expect("applied state list should remain available"),
            vec![RunState::Blocked, RunState::Finished]
        );
    }
}
