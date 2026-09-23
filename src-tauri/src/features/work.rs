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
    env,
    path::{Path, PathBuf},
    sync::Mutex,
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    agent_state::{provision_hooks, read_state_file, state_file_path, AgentStateRecord},
    app::{
        current_unix_seconds, DirectRunCheckoutPreview, DirectRunPreview, DirectRunSharedRun,
        PaneTab, RunDeletionResult, RunStateChangedEvent, Runtime, TerminalAttachment,
        TerminalExitEvent, TerminalOutputEvent,
    },
    domain::{
        compose_grill_continuation_prompt as build_grill_continuation_prompt,
        compose_grill_prompt as build_grill_prompt, compose_run_prompt as build_run_prompt, decide,
        discover_downstream_issue_candidates, external_link_view, format_grill_response, home_view,
        normalize_machine_path, parse_grill_question_group, parse_grill_question_group_since,
        run_is_active, search_items, suggest_untracked_runs, worktree_path, AgentKind,
        AgentPaneObservation, AuditAction,
        ConfirmedDownstreamIssue, Event, ExecutionProfile, ExternalChangePolicy, ExternalLinkView,
        ExternalObjectInput, ExternalObjectKind, ExternalProvider, ExternalSnapshot, GrillAnswer,
        GrillConfiguration, GrillContinuationAction, GrillPhase, HomeView, Item, ItemRelation,
        ItemRelationKind, ItemStatus, ItemView, Machine, MachineObservation, MachineTransport,
        Repository, RepositoryLocation, Run, RunCheckout, RunPaneStatus, RunPromptSelection,
        RunState, RunSuggestion, Workspace, WorkspaceRepository, WorkspaceRepositoryInput,
        Worktree,
    },
    git::GitCli,
    provider::{classify_url, resolve_gh_executable, GithubCli},
    terminal::{
        capture_pane, capture_pane_transcript, list_agent_panes, list_panes, open_pane_in_terminal,
        probe_machine, send_input_to_pane, terminal_transport, AgentLaunchContext,
        ExternalPaneIdentity, TerminalRuntime, TmuxControlPane, TmuxRuntime,
    },
};

use crate::features::deletion::{
    ExternalLinkDeletionResult, ExternalObjectDeletionPreview, ExternalObjectDeletionResult,
    ItemDeletionPreview, ItemDeletionResult, WorktreeRemovalReport, WorktreeRemovalResult,
};
use crate::features::structure::{machine_home_directory, resolve_machine_path};

fn format_commit_error(error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) => format!("{error}; {cleanup_error}"),
        None => error,
    }
}

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

pub(crate) fn get_home(
    context_id: Option<i64>,
    now: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HomeView, String> {
    locked(state, |runtime| {
        // Agent-owned state files can change the blocked-run projection between
        // app launches and Home refreshes. Keep that existing recovery hook at
        // the work projection seam until the Run slice is extracted.
        runtime.recover_run_states()?;
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

pub(crate) fn link_external_object(
    item_id: i64,
    url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    locked(state, |runtime| runtime.link_external_object(item_id, url))
}

pub(crate) fn create_github_issue(
    item_id: i64,
    repository: String,
    title: String,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    locked(state, |runtime| {
        runtime.create_github_issue(item_id, repository, title, body)
    })
}

pub(crate) fn add_external_comment(
    link_id: i64,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    locked(state, |runtime| runtime.add_external_comment(link_id, body))
}

pub(crate) fn refresh_external_object(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalSnapshot, String> {
    locked(state, |runtime| {
        runtime.refresh_external_object(external_object_id)
    })
}

pub(crate) fn poll_external_objects(
    state: State<'_, Mutex<Runtime>>,
) -> Result<PollResult, String> {
    locked(state, |runtime| Ok(runtime.poll_external_objects()))
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

pub(crate) fn prepare_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    reuse_existing_branch: bool,
    confirm_dirty_attachment: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    locked(state, |runtime| {
        runtime.prepare_worktree(
            workspace_id,
            repository_id,
            machine_id,
            reuse_existing_branch,
            confirm_dirty_attachment,
        )
    })
}

pub(crate) fn prepare_worktree_removal(
    worktree_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalReport, String> {
    locked(state, |runtime| {
        runtime.prepare_worktree_removal(worktree_id)
    })
}

pub(crate) fn remove_worktree(
    worktree_id: i64,
    confirmed: bool,
    destructive_confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorktreeRemovalResult, String> {
    locked(state, |runtime| {
        runtime.remove_worktree(worktree_id, confirmed, destructive_confirmed)
    })
}

pub(crate) fn attach_worktree(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    path: String,
    confirm_dirty_attachment: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Worktree, String> {
    locked(state, |runtime| {
        runtime.attach_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            confirm_dirty_attachment,
        )
    })
}

mod external;
mod items;
mod runs;
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

pub(crate) fn prepare_direct_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    locked(state, |runtime| {
        runtime.prepare_direct_run(item_id, workspace_id, machine_id)
    })
}

pub(crate) fn prepare_grill_run(
    item_id: i64,
    workspace_id: i64,
    machine_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<DirectRunPreview, String> {
    prepare_direct_run(item_id, workspace_id, machine_id, state)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_direct_run(
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
    locked(state, |runtime| {
        runtime.start_direct_run(
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
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_grill_run(
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
    locked(state, |runtime| {
        runtime.start_grill_run(
            item_id,
            workspace_id,
            machine_id,
            primary_repository_id,
            configuration,
            initial_prompt,
            expected_checkouts,
            allow_dirty,
            allow_shared_checkouts,
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_worktree_run(
    item_id: i64,
    workspace_id: i64,
    worktree_id: i64,
    agent: AgentKind,
    execution_profile: ExecutionProfile,
    prompt: String,
    prompt_selection: RunPromptSelection,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    locked(state, |runtime| {
        runtime.start_worktree_run(
            item_id,
            workspace_id,
            worktree_id,
            agent,
            execution_profile,
            prompt,
            prompt_selection,
        )
    })
}

pub(crate) fn list_run_suggestions(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<RunSuggestion>, String> {
    locked(state, |runtime| runtime.list_run_suggestions())
}

pub(crate) fn attach_run(
    suggestion: RunSuggestion,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    locked(state, |runtime| runtime.attach_run(suggestion))
}

pub(crate) fn open_terminal(
    app: &AppHandle,
    run_id: i64,
    terminal_id: String,
    session_name: String,
    pane_id: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<TerminalAttachment, String> {
    locked(state, |runtime| {
        runtime.open_terminal(app, run_id, terminal_id, session_name, pane_id)
    })
}

pub(crate) fn terminal_input(
    terminal_id: String,
    input: Vec<u8>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    locked(state, |runtime| runtime.terminal_input(&terminal_id, input))
}

pub(crate) fn submit_grill_answers(
    run_id: i64,
    answers: Vec<GrillAnswer>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    locked(state, |runtime| {
        runtime.submit_grill_answers(run_id, answers)
    })
}

pub(crate) fn continue_grill(
    run_id: i64,
    action: GrillContinuationAction,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Run, String> {
    locked(state, |runtime| runtime.continue_grill(run_id, action))
}

pub(crate) fn terminal_resize(
    terminal_id: String,
    columns: u16,
    rows: u16,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    locked(state, |runtime| {
        runtime.terminal_resize(&terminal_id, columns, rows)
    })
}

pub(crate) fn close_terminal(
    terminal_id: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    locked(state, |runtime| runtime.close_terminal(&terminal_id))
}

pub(crate) fn open_external_terminal(
    run_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    locked(state, |runtime| runtime.open_external_terminal(run_id))
}

pub(crate) fn stop_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    locked(state, |runtime| runtime.stop_run(run_id))
}

pub(crate) fn finish_run(run_id: i64, state: State<'_, Mutex<Runtime>>) -> Result<Run, String> {
    locked(state, |runtime| runtime.finish_run(run_id))
}

pub(crate) fn reconcile_runs(state: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    locked(state, |runtime| runtime.reconcile_runs())
}

pub(crate) fn delete_run(
    run_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<RunDeletionResult, String> {
    locked(state, |runtime| runtime.delete_run(run_id, confirmed))
}
