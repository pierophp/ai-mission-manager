//! Work feature command facade.
//!
//! This slice owns Item, External Object/Link, Project Repository execution, Run, grilling,
//! and Needs Attention workflows.
//! The reducer remains the pure `domain::state_transition` seam; this module
//! supplies the Runtime and Tauri adapters that persist its decisions and
//! keep Git/filesystem operations behind their existing adapters.
//! The implementation is split into internal modules so this facade keeps
//! command registration local without becoming a replacement monolith.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex},
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    agent_state::{read_state_file, state_file_path, AgentStateRecord},
    app::{
        current_unix_seconds, DirectRunCheckoutPreview, DirectRunPreview, DirectRunSharedRun,
        PaneTab, RunDeletionResult, RunStateChangedEvent, Runtime, TerminalAttachment,
        TerminalExitEvent, TerminalOutputEvent,
    },
    domain::{
        compose_grill_continuation_prompt as build_grill_continuation_prompt,
        compose_grill_prompt as build_grill_prompt, compose_run_prompt as build_run_prompt, decide,
        discover_downstream_issue_candidates, downstream_issue_is_new, external_link_view,
        format_grill_response, grill_transcript_extends, home_view, normalize_machine_path,
        parse_grill_question_group, parse_grill_question_group_since, run_is_active, search_items,
        suggest_untracked_runs, worktree_path, AgentKind, AgentPaneObservation, AuditAction,
        ConfirmedDownstreamIssue, Context, DomainState, Event, ExecutionProfile,
        ExternalChangePolicy, ExternalLinkView, ExternalObjectInput, ExternalObjectKind,
        ExternalProvider, ExternalSnapshot, GrillAnswer, GrillConfiguration,
        GrillContinuationAction, GrillPhase, HomeView, Item, ItemRelation, ItemRelationKind,
        ItemStatus, ItemView, Machine, MachineObservation, MachineTransport, Project, Repository,
        RepositoryLocation, Run, RunCheckout, RunPaneStatus, RunPromptSelection, RunState,
        RunSuggestion, Workspace, WorkspaceRepository, WorkspaceRepositoryInput, Worktree,
    },
    git::GitCli,
    provider::{
        classify_url, github_repository_name, resolve_gh_executable, GithubCli, IssueDocument,
    },
    terminal::{
        open_pane_in_terminal, terminal_transport, AgentLaunchContext, ExternalPaneIdentity,
        MachineObservationFailure, RunReconciliationResult, TerminalRuntime, TmuxControlPane,
    },
};

use crate::features::deletion::{
    ExternalLinkDeletionResult, ExternalObjectDeletionPreview, ExternalObjectDeletionResult,
    ItemDeletionPreview, ItemDeletionResult, WorktreeRemovalReport, WorktreeRemovalResult,
};
use crate::features::structure::{machine_home_directory, resolve_machine_path};

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn worktree_run_event(
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
    machine_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    working_directory: String,
    session_name: String,
    pane_id: String,
    started_at: i64,
    prompt_selection: RunPromptSelection,
) -> Event {
    Event::StartWorktreeRun {
        item_id,
        workspace_id,
        worktree_id,
        machine_id,
        agent,
        execution_profile,
        prompt,
        working_directory,
        session_name,
        pane_id,
        started_at,
        prompt_selection,
    }
}

fn format_commit_error(error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) => format!("{error}; {cleanup_error}"),
        None => error,
    }
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

fn locked<T>(
    state: State<'_, Mutex<Runtime>>,
    operation: impl FnOnce(&mut Runtime) -> Result<T, String>,
) -> Result<T, String> {
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    operation(&mut runtime)
}

pub(crate) async fn get_home(
    context_id: Option<i64>,
    now: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HomeView, String> {
    runs::recover_run_states_with_state(state.inner()).await?;
    locked(state, |runtime| {
        Ok(home_view(&runtime.state, context_id, &now))
    })
}

pub(crate) fn search_items_command(
    query: String,
    context_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ItemView>, String> {
    locked(state, |runtime| {
        Ok(search_items(&runtime.state, &query, context_id))
    })
}

pub(crate) fn list_inbox_items(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Item>, String> {
    locked(state, |runtime| {
        Ok(runtime
            .state
            .items
            .iter()
            .filter(|item| item.status == ItemStatus::Inbox)
            .cloned()
            .collect())
    })
}

pub(crate) fn prepare_item_deletion(
    item_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionPreview, String> {
    locked(state, |runtime| runtime.prepare_item_deletion(item_id))
}

pub(crate) fn delete_item(
    item_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemDeletionResult, String> {
    locked(state, |runtime| runtime.delete_item(item_id, confirmed))
}

pub(crate) fn create_item(
    title: String,
    context_id: i64,
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    locked(state, |runtime| {
        runtime.create_item(title, context_id, project_id)
    })
}

pub(crate) fn set_item_status(
    item_id: i64,
    status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    locked(state, |runtime| {
        runtime.update_item(Event::SetItemStatus { item_id, status }, item_id)
    })
}

pub(crate) fn set_item_title(
    item_id: i64,
    title: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    locked(state, |runtime| {
        runtime.update_item(Event::SetItemTitle { item_id, title }, item_id)
    })
}

pub(crate) fn set_item_notes(
    item_id: i64,
    notes: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    locked(state, |runtime| {
        runtime.update_item(Event::SetItemNotes { item_id, notes }, item_id)
    })
}

pub(crate) fn add_item_reminder(
    item_id: i64,
    remind_at: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    locked(state, |runtime| {
        runtime.update_item(Event::AddItemReminder { item_id, remind_at }, item_id)
    })
}

pub(crate) fn remove_item_reminder(
    item_id: i64,
    reminder_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    locked(state, |runtime| {
        runtime.update_item(
            Event::RemoveItemReminder {
                item_id,
                reminder_id,
            },
            item_id,
        )
    })
}

pub(crate) fn set_item_relation(
    from_item_id: i64,
    to_item_id: i64,
    kind: ItemRelationKind,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemRelation, String> {
    locked(state, |runtime| {
        runtime.set_item_relation(from_item_id, to_item_id, kind)
    })
}

pub(crate) fn set_link_attention_policy(
    link_id: i64,
    policy: Option<ExternalChangePolicy>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    locked(state, |runtime| {
        runtime.set_link_attention_policy(link_id, policy)
    })
}

pub(crate) fn set_link_watch_until(
    link_id: i64,
    watch_until: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    locked(state, |runtime| {
        runtime.set_link_schedule(
            Event::SetLinkWatchUntil {
                link_id,
                watch_until,
            },
            link_id,
            "Setting watch period",
        )
    })
}

pub(crate) fn set_link_review_at(
    link_id: i64,
    review_at: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    locked(state, |runtime| {
        runtime.set_link_schedule(
            Event::SetLinkReviewAt { link_id, review_at },
            link_id,
            "Setting review date",
        )
    })
}

pub(crate) fn clear_link_review_at(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    locked(state, |runtime| {
        runtime.set_link_schedule(
            Event::ClearLinkReviewAt { link_id },
            link_id,
            "Clearing review date",
        )
    })
}

pub(crate) fn mark_link_reviewed(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    locked(state, |runtime| runtime.mark_link_reviewed(link_id))
}

pub(crate) fn prepare_external_object_deletion(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalObjectDeletionPreview, String> {
    locked(state, |runtime| {
        runtime.prepare_external_object_deletion(external_object_id)
    })
}

pub(crate) fn delete_external_object(
    external_object_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalObjectDeletionResult, String> {
    locked(state, |runtime| {
        runtime.delete_external_object(external_object_id, confirmed)
    })
}

pub(crate) fn unlink_external_link(
    link_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkDeletionResult, String> {
    locked(state, |runtime| {
        runtime.unlink_external_link(link_id, confirmed)
    })
}

pub(crate) async fn link_external_object(
    item_id: i64,
    url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    external::link_external_object_with_state(item_id, url, state.inner()).await
}

pub(crate) async fn create_github_issue(
    item_id: i64,
    repository_id: i64,
    title: String,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    external::create_github_issue_with_state(item_id, repository_id, title, body, state.inner())
        .await
}

pub(crate) async fn add_external_comment(
    link_id: i64,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    external::add_external_comment_with_state(link_id, body, state.inner()).await
}

pub(crate) async fn fetch_issue_document(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<IssueDocument, String> {
    external::fetch_issue_document_with_state(external_object_id, state.inner()).await
}

pub(crate) async fn refresh_external_object(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalSnapshot, String> {
    external::refresh_external_object_with_state(external_object_id, state.inner()).await
}

pub(crate) async fn poll_external_objects(
    state: State<'_, Mutex<Runtime>>,
) -> Result<PollResult, String> {
    external::poll_external_objects_with_state(state.inner()).await
}

pub(crate) fn create_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    path: String,
    branch: String,
    base_branch: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    locked(state, |runtime| {
        runtime.create_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            branch,
            base_branch,
        )
    })
}

pub(crate) async fn prepare_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    reuse_existing_branch: bool,
    confirm_dirty_attachment: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    workspace::prepare_worktree_with_state(
        workspace_id,
        repository_id,
        machine_id,
        reuse_existing_branch,
        confirm_dirty_attachment,
        state.inner(),
    )
    .await
}

pub(crate) async fn prepare_worktree_removal(
    worktree_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalReport, String> {
    crate::features::deletion::prepare_worktree_removal_with_state(worktree_id, state.inner()).await
}

pub(crate) async fn remove_worktree(
    worktree_id: i64,
    confirmed: bool,
    destructive_confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalResult, String> {
    crate::features::deletion::remove_worktree_with_state(
        worktree_id,
        confirmed,
        destructive_confirmed,
        state.inner(),
    )
    .await
}

pub(crate) async fn attach_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    path: String,
    confirm_dirty_attachment: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    workspace::attach_worktree_with_state(
        workspace_id,
        repository_id,
        machine_id,
        path,
        confirm_dirty_attachment,
        state.inner(),
    )
    .await
}

mod external;
mod items;
pub(crate) mod runs;
mod workspace;

#[cfg(test)]
mod tests;

pub(crate) fn compose_run_prompt(
    item_id: i64,
    execution_profile: ExecutionProfile,
    prompt_selection: RunPromptSelection,
    custom_prompt: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<String, String> {
    locked(state, |runtime| {
        runtime.compose_run_prompt(item_id, execution_profile, prompt_selection, custom_prompt)
    })
}

pub(crate) fn compose_grill_prompt(
    item_id: i64,
    configuration: GrillConfiguration,
    initial_prompt: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<String, String> {
    locked(state, |runtime| {
        runtime.compose_grill_prompt(item_id, configuration, initial_prompt)
    })
}

pub(crate) async fn prepare_direct_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    runs::prepare_direct_run_with_state(item_id, workspace_id, machine_id, state.inner()).await
}

pub(crate) async fn prepare_grill_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    prepare_direct_run(item_id, workspace_id, machine_id, state).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_direct_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    primary_repository_id: i64,
    agent: AgentKind,
    configuration: Option<GrillConfiguration>,
    implementation_queue: Option<crate::domain::ImplementationQueueStart>,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    expected_checkouts: Vec<RunCheckout>,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    runs::start_direct_run_with_queue_state(
        item_id,
        workspace_id,
        machine_id,
        primary_repository_id,
        agent,
        configuration,
        implementation_queue,
        execution_profile,
        prompt,
        prompt_selection,
        expected_checkouts,
        allow_dirty,
        allow_shared_checkouts,
        state.inner(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_grill_run(
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
    runs::start_grill_run_with_state(
        item_id,
        workspace_id,
        machine_id,
        primary_repository_id,
        configuration,
        initial_prompt,
        expected_checkouts,
        allow_dirty,
        allow_shared_checkouts,
        state.inner(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_worktree_run(
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    runs::start_worktree_run_with_state(
        item_id,
        workspace_id,
        worktree_id,
        agent,
        execution_profile,
        prompt,
        prompt_selection,
        state.inner(),
    )
    .await
}

pub(crate) async fn list_run_suggestions(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<RunSuggestion>, String> {
    runs::list_run_suggestions_with_state(state.inner()).await
}

pub(crate) async fn attach_run(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    runs::attach_run_with_state(suggestion, state.inner()).await
}

pub(crate) async fn stop_untracked_agent(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    runs::stop_untracked_agent_with_state(suggestion, state.inner()).await
}

pub(crate) async fn delete_untracked_agent(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    runs::delete_untracked_agent_with_state(suggestion, state.inner()).await
}

pub(crate) async fn open_terminal(
    app: &AppHandle,
    run_id: i64,
    terminal_id: String,
    session_name: String,
    pane_id: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<TerminalAttachment, String> {
    runs::open_terminal_with_state(
        app,
        run_id,
        terminal_id,
        session_name,
        pane_id,
        state.inner(),
    )
    .await
}

pub(crate) async fn terminal_input(
    terminal_id: String,
    input: Vec<u8>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    let connection = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        Arc::clone(
            runtime
                .terminal_connections
                .get(&terminal_id)
                .ok_or_else(|| "The embedded terminal is not attached".to_owned())?,
        )
    };
    tauri::async_runtime::spawn_blocking(move || connection.send_input(&input))
        .await
        .map_err(|error| format!("Terminal input worker failed: {error}"))?
}

pub(crate) async fn submit_grill_answers(
    run_id: i64,
    answers: Vec<GrillAnswer>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    runs::submit_grill_answers_with_state(run_id, answers, state.inner()).await
}

pub(crate) async fn continue_grill(
    run_id: i64,
    action: GrillContinuationAction,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    runs::continue_grill_with_state(run_id, action, state.inner()).await
}

pub(crate) async fn terminal_resize(
    terminal_id: String,
    columns: u16,
    rows: u16,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    let connection = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        Arc::clone(
            runtime
                .terminal_connections
                .get(&terminal_id)
                .ok_or_else(|| "The embedded terminal is not attached".to_owned())?,
        )
    };
    tauri::async_runtime::spawn_blocking(move || connection.resize(columns, rows))
        .await
        .map_err(|error| format!("Terminal resize worker failed: {error}"))?
}

pub(crate) async fn close_terminal(
    terminal_id: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    let connection = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        // Closing also invalidates any attach worker that has not finished yet.
        runtime.invalidate_terminal_open_request(&terminal_id);
        runtime.terminal_connection_generations.remove(&terminal_id);
        runtime.terminal_connections.remove(&terminal_id)
    };
    if let Some(connection) = connection {
        tauri::async_runtime::spawn_blocking(move || connection.close())
            .await
            .map_err(|error| format!("Terminal close worker failed: {error}"))??;
    }
    Ok(())
}

pub(crate) async fn open_external_terminal(
    run_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    runs::open_external_terminal_with_state(run_id, state.inner()).await
}

pub(crate) async fn stop_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    runs::stop_run_with_state(run_id, state.inner()).await
}

pub(crate) async fn check_implementation_queue(
    queue_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    runs::check_implementation_queue_with_state(queue_id, state.inner()).await
}

pub(crate) async fn skip_implementation_queue_entry(
    queue_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    runs::skip_implementation_queue_entry_with_state(queue_id, state.inner()).await
}

pub(crate) async fn cancel_implementation_queue(
    queue_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    runs::cancel_implementation_queue_with_state(queue_id, state.inner()).await
}

pub(crate) fn finish_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    locked(state, |runtime| runtime.finish_run(run_id))
}

struct ReconciliationFlight(Arc<std::sync::atomic::AtomicBool>);

impl Drop for ReconciliationFlight {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(crate) async fn reconcile_runs_with_state(
    state: &Mutex<Runtime>,
    app: Option<AppHandle>,
) -> Result<RunReconciliationResult, String> {
    let (snapshot, flight) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let in_progress = Arc::clone(&runtime.reconciliation_in_progress);
        if in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(RunReconciliationResult::default());
        }
        let flight = ReconciliationFlight(in_progress);
        (runtime.reconciliation_snapshot(), flight)
    };

    let (observations, _flight) = tauri::async_runtime::spawn_blocking(move || {
        let observations = snapshot.observe();
        (observations, flight)
    })
    .await
    .map_err(|error| format!("Run reconciliation worker failed: {error}"))?;

    let applied = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.apply_reconciliation(observations)?
    };
    if let Some(app) = app {
        for event in applied.changed_runs {
            let _ = app.emit("run-state-changed", event);
        }
    }
    Ok(applied.result)
}

pub(crate) fn delete_run(
    run_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RunDeletionResult, String> {
    locked(state, |runtime| runtime.delete_run(run_id, confirmed))
}
