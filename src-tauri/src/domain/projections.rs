use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum RunLaunchTargetKind {
    Checkout,
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunLaunchProfileOption {
    pub execution_profile: ExecutionProfile,
    pub configuration: GrillConfiguration,
    pub requires_initial_prompt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunLaunchWorkflowOptions {
    pub workflow: Workflow,
    pub default_profile: ExecutionProfile,
    pub profiles: Vec<RunLaunchProfileOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunLaunchOptions {
    pub default_workflow: Workflow,
    pub workflows: Vec<RunLaunchWorkflowOptions>,
}

pub fn run_launch_options(
    state: &DomainState,
    item_id: i64,
    target: RunLaunchTargetKind,
) -> Result<RunLaunchOptions, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let project = state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })?;
    let context = state
        .contexts
        .iter()
        .find(|context| context.id == project.context_id)
        .ok_or(DomainError::ContextNotFound {
            context_id: project.context_id,
        })?;
    let workflows = [Workflow::MattPocock, Workflow::Pstack]
        .into_iter()
        .map(|workflow| {
            let profiles = [
                ExecutionProfile::Grill,
                ExecutionProfile::Investigate,
                ExecutionProfile::Implement,
                ExecutionProfile::Review,
                ExecutionProfile::Autonomous,
                ExecutionProfile::Plan,
                ExecutionProfile::PstackReview,
                ExecutionProfile::CustomPrompt,
            ]
            .into_iter()
            .filter(|profile| {
                workflow.offers(*profile)
                    && !(target == RunLaunchTargetKind::Worktree
                        && *profile == ExecutionProfile::Grill)
            })
            .map(|execution_profile| RunLaunchProfileOption {
                execution_profile,
                configuration: if workflow == Workflow::Pstack {
                    context.pstack_defaults.clone()
                } else if execution_profile == ExecutionProfile::Grill {
                    context.grill_defaults.clone()
                } else {
                    context.implement_defaults.clone()
                },
                requires_initial_prompt: matches!(
                    execution_profile,
                    ExecutionProfile::Grill
                        | ExecutionProfile::CustomPrompt
                        | ExecutionProfile::PstackReview
                ),
            })
            .collect::<Vec<_>>();
            let default_profile = match workflow {
                Workflow::Pstack => ExecutionProfile::Autonomous,
                Workflow::MattPocock if target == RunLaunchTargetKind::Worktree => {
                    ExecutionProfile::Investigate
                }
                Workflow::MattPocock => ExecutionProfile::Grill,
            };
            RunLaunchWorkflowOptions {
                workflow,
                default_profile,
                profiles,
            }
        })
        .collect();
    Ok(RunLaunchOptions {
        default_workflow: context.default_workflow,
        workflows,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RunDisplayPhase {
    Unknown,
    Working,
    Blocked,
    Finished,
    AwaitingGo,
    GrillStarting,
    GrillWorking,
    GrillWaitingForAnswers,
    GrillAwaitingNextAction,
    GrillRecoverablePaneLoss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunContinuations {
    pub go_plan: bool,
    pub grill_actions: Vec<GrillContinuationAction>,
    pub stop: bool,
    pub finish: bool,
    pub delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunProjection {
    pub run_id: i64,
    pub status: RunStatus,
    pub phase: RunDisplayPhase,
    pub continuations: RunContinuations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Active,
    Finished,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemRunSignals {
    pub grill_waiting: bool,
    pub run_active: bool,
}

pub fn run_projection(run: &Run) -> RunProjection {
    let active = run_is_active(run);
    let grill_actions = [
        GrillContinuationAction::ToSpec,
        GrillContinuationAction::ToTickets,
        GrillContinuationAction::Implement,
    ]
    .into_iter()
    .filter(|action| {
        grill_continuation_available(run.grill_phase, run.grill_action, *action)
            && !(*action == GrillContinuationAction::ToSpec
                && run.grill_phase == Some(GrillPhase::AwaitingNextAction)
                && run.grill_action == Some(GrillContinuationAction::ToSpec))
    })
    .collect();
    RunProjection {
        run_id: run.id,
        status: if active {
            RunStatus::Active
        } else {
            RunStatus::Finished
        },
        phase: run_display_phase(run),
        continuations: RunContinuations {
            go_plan: run.execution_profile == ExecutionProfile::Plan
                && run.plan_phase == Some(PlanPhase::AwaitingGo),
            grill_actions,
            stop: active && run.pane_status != RunPaneStatus::Missing,
            finish: active,
            delete: !active,
        },
    }
}

fn run_display_phase(run: &Run) -> RunDisplayPhase {
    if run.execution_profile == ExecutionProfile::Plan
        && run.plan_phase == Some(PlanPhase::AwaitingGo)
    {
        return RunDisplayPhase::AwaitingGo;
    }
    if run.execution_profile == ExecutionProfile::Grill {
        match run.grill_phase {
            Some(GrillPhase::Starting) => return RunDisplayPhase::GrillStarting,
            Some(GrillPhase::Working) => return RunDisplayPhase::GrillWorking,
            Some(GrillPhase::WaitingForAnswers) => {
                return RunDisplayPhase::GrillWaitingForAnswers;
            }
            Some(GrillPhase::AwaitingNextAction) => {
                return RunDisplayPhase::GrillAwaitingNextAction;
            }
            Some(GrillPhase::RecoverablePaneLoss) => {
                return RunDisplayPhase::GrillRecoverablePaneLoss;
            }
            Some(GrillPhase::Finished) | None => {}
        }
    }
    match run.state {
        RunState::Unknown => RunDisplayPhase::Unknown,
        RunState::Working => RunDisplayPhase::Working,
        RunState::Blocked => RunDisplayPhase::Blocked,
        RunState::Finished => RunDisplayPhase::Finished,
    }
}

fn item_run_signals(runs: &[Run]) -> ItemRunSignals {
    let grill_waiting = runs.iter().any(|run| {
        run.execution_profile == ExecutionProfile::Grill
            && run.grill_phase == Some(GrillPhase::WaitingForAnswers)
            && run.grill_response.is_none()
    });
    let run_active = runs.iter().any(|run| {
        run_is_active(run)
            && run.pane_status != RunPaneStatus::Missing
            && !(run.execution_profile == ExecutionProfile::Grill
                && run.grill_phase == Some(GrillPhase::WaitingForAnswers)
                && run.grill_response.is_none())
    });
    ItemRunSignals {
        grill_waiting,
        run_active,
    }
}

fn supports_implementation_spec(object: &ExternalObject) -> bool {
    matches!(
        (object.provider, object.kind),
        (ExternalProvider::GitHub, ExternalObjectKind::Issue)
            | (
                ExternalProvider::Atlassian,
                ExternalObjectKind::Document | ExternalObjectKind::Issue
            )
    ) || (object.provider == ExternalProvider::Generic && object.external_key.starts_with("local:"))
}

fn supports_implementation_ticket(object: &ExternalObject) -> bool {
    matches!(
        (object.provider, object.kind),
        (ExternalProvider::GitHub, ExternalObjectKind::Issue)
            | (ExternalProvider::Atlassian, ExternalObjectKind::Issue)
    ) || (object.provider == ExternalProvider::Generic && object.external_key.starts_with("local:"))
}

pub fn suggest_untracked_runs(
    state: &DomainState,
    panes: &[AgentPaneObservation],
) -> Vec<RunSuggestion> {
    let mut suggestions = panes
        .iter()
        .filter(|pane| {
            let selected_for_context = state
                .machines
                .iter()
                .find(|machine| machine.id == pane.machine_id)
                .and_then(|machine| {
                    state
                        .contexts
                        .iter()
                        .find(|context| context.id == machine.context_id)
                        .map(|context| context.execution_machine_id == Some(machine.id))
                })
                .unwrap_or(false);
            selected_for_context
                && !state.runs.iter().any(|run| {
                    run.machine_id == pane.machine_id
                        && run.session_name == pane.session_name
                        && run.pane_id == pane.pane_id
                })
        })
        .filter_map(|pane| {
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == pane.machine_id)?;
            let workspace_location = state
                .workspaces
                .iter()
                .filter_map(|workspace| {
                    let worktree = state
                        .worktrees
                        .iter()
                        .filter(|worktree| {
                            worktree.workspace_id == workspace.id
                                && worktree.machine_id == pane.machine_id
                                && path_is_within(
                                    &worktree.path,
                                    &pane.current_path,
                                    &pane.machine_home,
                                )
                        })
                        .max_by_key(|worktree| worktree.path.len());
                    if let Some(worktree) = worktree {
                        return Some((
                            worktree.path.len(),
                            workspace.id,
                            worktree.repository_id,
                            Some(worktree.id),
                            worktree.path.clone(),
                        ));
                    }
                    let project_id = state
                        .items
                        .iter()
                        .find(|item| item.id == workspace.item_id)
                        .map(|item| item.project_id)?;
                    let project_items = state
                        .items
                        .iter()
                        .filter(|item| item.project_id == project_id)
                        .count();
                    state
                        .repositories
                        .iter()
                        .filter(|repository| repository.project_id == project_id)
                        .find_map(|repository| {
                            state
                                .repository_locations
                                .iter()
                                .filter(|location| {
                                    location.repository_id == repository.id
                                        && location.machine_id == pane.machine_id
                                        && project_items == 1
                                        && path_is_within(
                                            &location.checkout_path,
                                            &pane.current_path,
                                            &pane.machine_home,
                                        )
                                })
                                .max_by_key(|location| location.checkout_path.len())
                                .map(|location| {
                                    (
                                        location.checkout_path.len(),
                                        workspace.id,
                                        repository.id,
                                        None,
                                        location.checkout_path.clone(),
                                    )
                                })
                        })
                })
                .max_by_key(|candidate| candidate.0);
            let (_, workspace_id, repository_id, worktree_id, location_path) = workspace_location?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)?;
            let item_id = workspace.item_id;
            let item = state.items.iter().find(|item| item.id == item_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;

            Some(RunSuggestion {
                machine_id: machine.id,
                machine_name: machine.name.clone(),
                agent: pane.agent,
                session_name: pane.session_name.clone(),
                pane_id: pane.pane_id.clone(),
                current_path: pane.current_path.clone(),
                item_id: item.id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id,
                location_path: Some(location_path),
            })
        })
        .collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.machine_id
            .cmp(&right.machine_id)
            .then_with(|| left.session_name.cmp(&right.session_name))
            .then_with(|| left.pane_id.cmp(&right.pane_id))
    });
    suggestions
}

pub(crate) fn path_is_within(root: &str, path: &str, machine_home: &str) -> bool {
    let root = without_macos_private_prefix(root).trim_end_matches('/');
    let path = without_macos_private_prefix(path);
    if root == "/" {
        return true;
    }

    if root == "~" || root.starts_with("~/") {
        if same_or_descendant_path(root, path) {
            return true;
        }

        let home = without_macos_private_prefix(machine_home).trim_end_matches('/');
        if home.is_empty() {
            return false;
        }
        let Some(relative_path) = path.strip_prefix(home) else {
            return false;
        };
        if !relative_path.is_empty() && !relative_path.starts_with('/') {
            return false;
        }
        let home_relative_path = format!("~{relative_path}");
        return same_or_descendant_path(root, &home_relative_path);
    }

    same_or_descendant_path(root, path)
}

fn same_or_descendant_path(root: &str, path: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn without_macos_private_prefix(path: &str) -> &str {
    path.strip_prefix("/private")
        .filter(|path| path.starts_with('/'))
        .unwrap_or(path)
}

pub fn home_view(state: &DomainState, context_id: Option<i64>, now: &str) -> HomeView {
    let mut view = HomeView {
        needs_attention: Vec::new(),
        attention_entries: attention_entries(state, context_id, now),
        running: Vec::new(),
        waiting: Vec::new(),
        due: Vec::new(),
        completed: Vec::new(),
    };

    for item in item_views_at(state, context_id, Some(now)) {
        let is_due = item_has_due_reminder(&item.item, now) && item.item.status != ItemStatus::Done;
        if is_due {
            view.due.push(item.clone());
        }
        if is_due
            || item.item.status == ItemStatus::Inbox
            || (item.item.status != ItemStatus::Done
                && view
                    .attention_entries
                    .iter()
                    .any(|entry| entry.item_id == item.item.id))
        {
            view.needs_attention.push(item.clone());
        }
        match item.item.status {
            ItemStatus::Inbox => {}
            ItemStatus::Active => view.running.push(item),
            ItemStatus::Waiting => view.waiting.push(item),
            ItemStatus::Done => view.completed.push(item),
        }
    }

    view
}

pub fn search_items(state: &DomainState, query: &str, context_id: Option<i64>) -> Vec<ItemView> {
    let query = query.trim().to_lowercase();
    item_views(state, context_id)
        .into_iter()
        .filter(|view| {
            query.is_empty()
                || [
                    view.item.human_identifier.as_str(),
                    view.item.title.as_str(),
                    view.item.notes.as_str(),
                    view.context_name.as_str(),
                    view.project_name.as_str(),
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
        })
        .collect()
}

pub fn external_link_view(state: &DomainState, link: &Link) -> Option<ExternalLinkView> {
    external_link_view_at(state, link, None)
}

fn external_link_view_at(
    state: &DomainState,
    link: &Link,
    now: Option<&str>,
) -> Option<ExternalLinkView> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == link.external_object_id)?;
    let snapshot = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .cloned();
    Some(ExternalLinkView {
        link: link.clone(),
        object: object.clone(),
        snapshot,
        attention_policy: effective_attention_policy(state, link, object),
        attention_entry: attention_entry_for_link_at(state, link, object, now),
        supports_implementation_spec: supports_implementation_spec(object),
        supports_implementation_ticket: supports_implementation_ticket(object),
    })
}

pub fn attention_entries(
    state: &DomainState,
    context_id: Option<i64>,
    now: &str,
) -> Vec<AttentionEntry> {
    attention_entries_at(state, context_id, Some(now))
}

fn attention_entries_at(
    state: &DomainState,
    context_id: Option<i64>,
    now: Option<&str>,
) -> Vec<AttentionEntry> {
    let mut entries = Vec::new();
    for link in state.links.iter().filter(|link| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, link.item_id)
                .map(|link_context_id| link_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id);
        let Some(object) = object else {
            continue;
        };
        if let Some(entry) = attention_entry_for_link_at(state, link, object, now) {
            entries.push(entry);
        }
        if link
            .review_at
            .as_deref()
            .is_some_and(|review_at| now.is_some_and(|now| review_at <= now))
        {
            entries.push(AttentionEntry {
                kind: AttentionEntryKind::Review,
                link_id: link.id,
                reminder_id: None,
                run_id: None,
                queue_id: None,
                item_id: link.item_id,
                external_object_id: object.id,
                source_title: state
                    .snapshots
                    .iter()
                    .find(|snapshot| snapshot.external_object_id == object.id)
                    .map(|snapshot| snapshot.title.clone())
                    .unwrap_or_else(|| object.canonical_url.clone()),
                source_url: object.canonical_url.clone(),
                activities: Vec::new(),
                summary: format!(
                    "Review scheduled for {}",
                    link.review_at.as_deref().unwrap_or_default()
                ),
            });
        }
    }

    for item in state.items.iter().filter(|item| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, item.id)
                .map(|item_context_id| item_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        entries.extend(
            item.reminders
                .iter()
                .filter(|reminder| reminder.remind_at.as_str() <= now.unwrap_or_default())
                .map(|reminder| AttentionEntry {
                    kind: AttentionEntryKind::Reminder,
                    link_id: 0,
                    reminder_id: Some(reminder.id),
                    run_id: None,
                    queue_id: None,
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Reminder due at {}", reminder.remind_at),
                }),
        );
    }

    entries.extend(
        state
            .runs
            .iter()
            .filter(|run| run.state == RunState::Blocked)
            .filter(|run| {
                context_id.is_none_or(|context_id| {
                    item_context_id(state, run.item_id)
                        .map(|run_context_id| run_context_id == context_id)
                        .unwrap_or(false)
                })
            })
            .filter_map(|run| {
                let item = state.items.iter().find(|item| item.id == run.item_id)?;
                Some(AttentionEntry {
                    kind: AttentionEntryKind::BlockedRun,
                    link_id: 0,
                    reminder_id: None,
                    run_id: Some(run.id),
                    queue_id: None,
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Run #{} is blocked and needs your input", run.id),
                })
            }),
    );

    for queue in state
        .implementation_queues
        .iter()
        .filter(|queue| queue.active && queue.paused_reason.is_some())
    {
        let Some(entry) = queue
            .entries
            .iter()
            .find(|entry| !entry.done && !entry.skipped)
        else {
            continue;
        };
        let Some(link) = state.links.iter().find(|link| {
            link.item_id == queue.item_id
                && link.external_object_id == queue.spec_external_object_id
        }) else {
            continue;
        };
        let Some(object) = state
            .external_objects
            .iter()
            .find(|object| object.id == queue.spec_external_object_id)
        else {
            continue;
        };
        let reason = match queue.paused_reason.as_ref().expect("filtered paused queue") {
            ImplementationQueuePauseReason::TicketStillOpen => "ticket is still open".to_owned(),
            ImplementationQueuePauseReason::CheckoutDirty => "checkout is dirty".to_owned(),
            ImplementationQueuePauseReason::RunStopped => "Run was stopped".to_owned(),
            ImplementationQueuePauseReason::PaneMissing => "Run Pane is missing".to_owned(),
            ImplementationQueuePauseReason::LaunchFailed(message) => {
                format!("next Run failed to launch: {message}")
            }
        };
        entries.push(AttentionEntry {
            kind: AttentionEntryKind::ImplementationQueue,
            link_id: link.id,
            reminder_id: None,
            run_id: entry.run_id,
            queue_id: Some(queue.id),
            item_id: queue.item_id,
            external_object_id: object.id,
            source_title: entry.ticket_title.clone(),
            source_url: entry.ticket_url.clone(),
            activities: Vec::new(),
            summary: format!(
                "Implementation Queue ticket #{} paused: {reason}",
                entry.ticket_number
            ),
        });
    }

    entries
}

fn item_has_due_reminder(item: &Item, now: &str) -> bool {
    item.reminders
        .iter()
        .any(|reminder| reminder.remind_at.as_str() <= now)
}

fn item_views(state: &DomainState, context_id: Option<i64>) -> Vec<ItemView> {
    item_views_at(state, context_id, None)
}

fn item_views_at(state: &DomainState, context_id: Option<i64>, now: Option<&str>) -> Vec<ItemView> {
    state
        .items
        .iter()
        .filter_map(|item| {
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            if context_id.is_some_and(|candidate| candidate != project.context_id) {
                return None;
            }
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;
            let relationships = state
                .relationships
                .iter()
                .filter(|relation| {
                    relation.from_item_id == item.id || relation.to_item_id == item.id
                })
                .cloned()
                .collect();
            let links = state
                .links
                .iter()
                .filter(|link| link.item_id == item.id)
                .filter_map(|link| external_link_view_at(state, link, now))
                .collect();
            let project_repositories = state
                .repositories
                .iter()
                .filter(|repository| repository.project_id == item.project_id)
                .map(|repository| WorkspaceRepository {
                    repository_id: repository.id,
                    branch: format!("mission-{}", item.human_identifier),
                    base_branch: repository.base_branch.clone(),
                })
                .collect::<Vec<_>>();
            let workspaces = state
                .workspaces
                .iter()
                .filter(|workspace| workspace.item_id == item.id)
                .map(|workspace| Workspace {
                    repositories: project_repositories.clone(),
                    ..workspace.clone()
                })
                .collect();
            let runs: Vec<Run> = state
                .runs
                .iter()
                .filter(|run| run.item_id == item.id)
                .cloned()
                .collect();
            let run_projections = runs.iter().map(run_projection).collect();
            let run_signals = item_run_signals(&runs);
            let worktrees = state
                .worktrees
                .iter()
                .filter(|worktree| {
                    state.workspaces.iter().any(|workspace| {
                        workspace.id == worktree.workspace_id && workspace.item_id == item.id
                    })
                })
                .cloned()
                .collect();
            Some(ItemView {
                item: item.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                project_name: project.name.clone(),
                relationships,
                workspaces,
                worktrees,
                runs,
                run_projections,
                run_signals,
                implementation_queues: state
                    .implementation_queues
                    .iter()
                    .filter(|queue| queue.item_id == item.id)
                    .cloned()
                    .collect(),
                links,
            })
        })
        .collect()
}

pub fn compose_run_prompt(
    state: &DomainState,
    item_id: i64,
    profile: ExecutionProfile,
    selection: &RunPromptSelection,
    language: Option<GrillLanguage>,
    initial_prompt: Option<&str>,
) -> Result<String, DomainError> {
    let initial_prompt = initial_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty());
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let mut sections = Vec::new();
    if selection.include_objective {
        sections.push(format!("Item objective:\n{}", item.title));
    }
    for external_object_id in &selection.external_object_ids {
        let link = state
            .links
            .iter()
            .find(|link| link.item_id == item_id && link.external_object_id == *external_object_id)
            .ok_or(DomainError::RunPromptSourceNotLinked {
                external_object_id: *external_object_id,
                item_id,
            })?;
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
            .ok_or(DomainError::ExternalObjectNotFound {
                external_object_id: *external_object_id,
            })?;
        let title = state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == object.id)
            .map(|snapshot| snapshot.title.as_str())
            .unwrap_or("Linked external object");
        sections.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
    }

    let instruction = match profile {
        ExecutionProfile::Investigate => {
            "Investigate this work, inspect the relevant code, and report findings before changing files."
                .to_owned()
        }
        ExecutionProfile::Implement => {
            format!(
                "{}\n\nImplement this work using the Repositories configured for its Project or a registered Worktree, run the relevant checks, and leave the changes ready for review.",
                implementation_skill_snapshot()
            )
        }
        ExecutionProfile::Review => {
            "Review the current changes for this Item for correctness, regressions, and missing test coverage."
                .to_owned()
        }
        ExecutionProfile::CustomPrompt => initial_prompt
            .ok_or(DomainError::EmptyRunPrompt)?
            .to_owned(),
        ExecutionProfile::Autonomous | ExecutionProfile::Plan | ExecutionProfile::PstackReview => {
            return Err(DomainError::ExecutionProfileNotInWorkflow {
                workflow: Workflow::MattPocock,
                execution_profile: profile,
            })
        }
        ExecutionProfile::Grill => {
            "Use the selected grilling skill to ask a structured frontier of questions before recommending the next decision."
                .to_owned()
        }
    };
    sections.insert(0, instruction);
    if profile != ExecutionProfile::CustomPrompt {
        if let Some(initial_prompt) = initial_prompt {
            sections.push(format!("User's initial prompt:\n{initial_prompt}"));
        }
    }
    if let Some(language) = language {
        sections.insert(0, language.run_response_instruction().to_owned());
    }
    let prompt = sections.join("\n\n");
    clean_name(prompt, DomainError::EmptyRunPrompt)
}

pub fn compose_pstack_prompt(
    state: &DomainState,
    item_id: i64,
    profile: ExecutionProfile,
    skill_root: &str,
    language: GrillLanguage,
    initial_prompt: Option<&str>,
) -> Result<String, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let initial_prompt = initial_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let spec = state
        .links
        .iter()
        .find(|link| link.item_id == item_id && link.purpose == LinkPurpose::ToSpec)
        .and_then(|link| {
            state
                .external_objects
                .iter()
                .find(|object| object.id == link.external_object_id)
        })
        .map(|object| object.canonical_url.as_str());
    let context_id = item_context_id(state, item_id)?;
    let context = state
        .contexts
        .iter()
        .find(|context| context.id == context_id)
        .ok_or(DomainError::ContextNotFound { context_id })?;
    let roles_path = pstack_role_file_path(skill_root, context);
    let roles_instruction = format!(
        "Read the generated role instructions at `{roles_path}` and follow them when delegating."
    );
    if matches!(
        profile,
        ExecutionProfile::PstackReview | ExecutionProfile::CustomPrompt
    ) && initial_prompt.is_none()
    {
        return Err(DomainError::EmptyRunPrompt);
    }
    let profile_instruction = match profile {
        ExecutionProfile::Autonomous => format!("You are starting an Autonomous pstack Run. Read `{skill_root}/skills/poteto-mode/SKILL.md` in full before acting. The Skill tool is unavailable because these skills disable model invocation; read any other needed skill by its absolute path under `{skill_root}/skills/` instead of relying on the Skill tool. {roles_instruction} Do not paste skill text into your response."),
        ExecutionProfile::Plan => format!("You are starting a Plan pstack Run. Read `{skill_root}/skills/poteto-mode/SKILL.md` in full and follow `{skill_root}/skills/poteto-mode/playbooks/multi-phase-plan.md` in full. The Skill tool is unavailable because these skills disable model invocation; read both files by their absolute paths. {roles_instruction} Complete the required planning phases, write the plan in the repository, then stop without implementing it. Report the plan's repository-relative path in your final response. Do not delegate implementation."),
        ExecutionProfile::PstackReview => format!("You are starting a pstack Review Run. Read `{skill_root}/skills/interrogate/SKILL.md` and follow it to review the Pull Request or branch named in the Initial Prompt. Read the generated role instructions at `{roles_path}` and use its `Review panel` entry to configure the read-only reviewers. Review only: do not edit files, commit, push, or apply suggested changes. Synthesize the reviewers' findings into a verdict, including actionable findings and disagreements. Do not use the matt-pocock `review` profile instructions."),
        ExecutionProfile::CustomPrompt => {
            initial_prompt.ok_or(DomainError::EmptyRunPrompt)?;
            format!("You are starting a Custom pstack Run. No skill is selected: do what the Initial Prompt asks. {roles_instruction}")
        }
        _ => return Err(DomainError::ExecutionProfileNotInWorkflow { workflow: Workflow::Pstack, execution_profile: profile }),
    };
    let mut prompt = format!("{profile_instruction}\n\nItem: {}", item.title,);
    if let Some(initial_prompt) = initial_prompt {
        prompt.push_str(&format!("\n\nInitial Prompt:\n{initial_prompt}"));
    }
    if let Some(spec) = spec {
        prompt.push_str(&format!("\n\nSpec: {spec}"));
    }
    prompt.push_str(&format!("\n\n{}\nWrite commits and pull requests in English.\n\nMission Manager event contract:\nWhen a Pull Request is opened, immediately print `AI_MISSION_MANAGER_EVENT {{\"event\":\"pull_request.opened\",\"url\":\"<canonical Pull Request URL>\"}}` on a line by itself. Report every Pull Request opened by this Run. At the end of the final Attention section in your final response, print `AI_MISSION_MANAGER_EVENT {{\"event\":\"attention.final\",\"summary\":\"<concise Attention summary>\"}}`. For a Plan Run, also print `AI_MISSION_MANAGER_EVENT {{\"event\":\"plan.ready\",\"path\":\"<repository-relative plan path>\"}}` after writing the plan. Escape JSON strings correctly.", language.run_response_instruction()));
    if profile == ExecutionProfile::PstackReview {
        prompt.push_str("\n\nThis Run is read-only. Do not edit files, change branches, create commits, push, open or modify pull requests, or apply reviewer suggestions. Return the synthesized verdict in the final response.");
    }
    clean_name(prompt, DomainError::EmptyRunPrompt)
}

pub fn pstack_role_file_path(skill_root: &str, context: &Context) -> String {
    let identity = format!(
        "{}|{:?}|{:?}|{:?}",
        serde_json::to_string(&context.pstack_roles).unwrap_or_default(),
        context.claude_profile_id,
        context.codex_profile_id,
        context.id,
    );
    let hash = identity.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("{skill_root}/roles/context-{}-{hash:016x}.md", context.id)
}

pub fn compose_pstack_role_file(
    parent_agent: AgentKind,
    roles: &PstackRoleTable,
    claude_profile: Option<&CliConfigurationProfile>,
    codex_profile: Option<&CliConfigurationProfile>,
) -> String {
    let mut contents = String::from(
        "# pstack role assignments\n\nDelegate each task according to its role row. Use the configured model and effort. When the role CLI matches the active parent CLI, spawn the role inside the current harness. When it differs, invoke that CLI as shown, preserving the selected Context profile.\n\n",
    );
    for entry in &roles.0 {
        let config = &entry.configuration;
        let profile = match config.agent {
            AgentKind::Claude => claude_profile,
            AgentKind::Codex => codex_profile,
        };
        let profile_line = profile
            .map(|profile| {
                format!(
                    "Context CLI profile #{} (`{}`) at `{}`",
                    profile.id, profile.name, profile.directory
                )
            })
            .unwrap_or_else(|| {
                "the standard CLI configuration (no Context profile selected)".to_owned()
            });
        let invocation = match config.agent {
            AgentKind::Claude => format!(
                "`{}claude -p --model {} --effort {}`",
                profile
                    .map(|profile| format!(
                        "CLAUDE_CONFIG_DIR={} ",
                        shell_single_quote(&profile.directory)
                    ))
                    .unwrap_or_default(),
                config.model,
                config.effort
            ),
            AgentKind::Codex => format!(
                "`{}codex exec --model {} -c model_reasoning_effort={}`",
                profile
                    .map(|profile| format!(
                        "CODEX_HOME={} ",
                        shell_single_quote(&profile.directory)
                    ))
                    .unwrap_or_default(),
                config.model,
                config.effort
            ),
        };
        let delegation = if config.agent == parent_agent {
            format!(
                "Spawn this role inside the active {} harness.",
                parent_agent.slug()
            )
        } else {
            format!(
                "This role uses the other CLI; invoke it using {invocation} with {profile_line}."
            )
        };
        contents.push_str(&format!(
            "## {}\n- CLI: {}\n- Model: `{}`\n- Effort: `{}`\n- {}\n\n",
            entry.role.label(),
            config.agent.slug(),
            config.model,
            config.effort,
            delegation
        ));
    }
    contents
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn compose_grill_prompt(
    state: &DomainState,
    item_id: i64,
    configuration: &GrillConfiguration,
    language: GrillLanguage,
    initial_prompt: &str,
) -> Result<String, DomainError> {
    validate_grill_configuration(configuration)?;
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let initial_prompt = clean_name(initial_prompt.to_owned(), DomainError::EmptyRunPrompt)?;
    // Item notes are not added here: the launch form prefills the initial prompt with them.
    let mut context = vec![format!("Item objective:\n{}", item.title)];
    for link in state.links.iter().filter(|link| link.item_id == item_id) {
        if let Some(object) = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
        {
            let title = state
                .snapshots
                .iter()
                .find(|snapshot| snapshot.external_object_id == object.id)
                .map(|snapshot| snapshot.title.as_str())
                .unwrap_or("Linked external object");
            context.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
        }
    }
    Ok(format!(
        "You are starting a Grill Run.\n\n{}\n\nGrill configuration: agent={}, model={}, effort={}.\n\nGrilling skill snapshot:\n{}\n\n{}\n\nRelevant Item context:\n{}\n\nUser's initial prompt:\n{}",
        language.response_instruction(),
        serde_json::to_string(&configuration.agent).unwrap_or_else(|_| "unknown".into()),
        configuration.model,
        configuration.effort,
        grill_skill_snapshot(),
        GRILL_OUTPUT_CONTRACT,
        context.join("\n\n"),
        initial_prompt,
    ))
}

pub fn compose_grill_continuation_prompt(
    state: &DomainState,
    run_id: i64,
    action: GrillContinuationAction,
) -> Result<String, DomainError> {
    let run = state
        .runs
        .iter()
        .find(|run| run.id == run_id)
        .ok_or(DomainError::RunNotFound { run_id })?;
    if run.execution_profile != ExecutionProfile::Grill {
        return Err(DomainError::NotGrillRun { run_id });
    }
    let item = state
        .items
        .iter()
        .find(|item| item.id == run.item_id)
        .ok_or(DomainError::ItemNotFound {
            item_id: run.item_id,
        })?;
    let mut context = vec![format!("Item objective:\n{}", item.title)];
    if !item.notes.trim().is_empty() {
        context.push(format!("Item notes:\n{}", item.notes.trim()));
    }
    for link in state.links.iter().filter(|link| link.item_id == item.id) {
        if let Some(object) = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
        {
            let title = state
                .snapshots
                .iter()
                .find(|snapshot| snapshot.external_object_id == object.id)
                .map(|snapshot| snapshot.title.as_str())
                .unwrap_or("Linked external object");
            context.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
        }
    }
    let decisions = if run.grill_decisions.is_empty() {
        run.grill_response
            .as_deref()
            .filter(|response| !response.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| format_grill_response(&run.grill_answers).ok())
            .unwrap_or_else(|| "No structured Grill decisions were recorded.".into())
    } else {
        format_recorded_grill_decisions(&run.grill_decisions)
    };

    let mut sections = vec![
        GrillLanguage::from_run_prompt(&run.prompt)
            .response_instruction()
            .to_owned(),
        "Continue the existing Grill Run in the same Run and Pane. The Grill conversation is already in your context; use it as the primary source.".to_owned(),
        format!("Selected downstream action: {}", action.as_str()),
        format!(
            "Downstream skill snapshot (inject this content explicitly; do not rely on the agent having the skill installed):\n{}",
            action.skill_snapshot()
        ),
        format!("Relevant Item context:\n{}", context.join("\n\n")),
        format!("Recorded Grill decisions:\n{decisions}"),
    ];
    if let Some(open_questions) = open_grill_questions(run) {
        sections.push(match run.grill_action {
            None => format!(
                "The user stopped the Grill early, before answering these questions. Do not answer them yourself and do not ask them again: record each one in the spec as an open question, with your recommendation.\n{open_questions}"
            ),
            Some(previous) => format!(
                "The user moved on from {} without answering its last questions. Do not ask them again: treat each one as settled by its recommendation.\n{open_questions}",
                previous.as_str()
            ),
        });
    }
    if action == GrillContinuationAction::ToTickets {
        let specs = downstream_issue_urls(state, run.id, GrillContinuationAction::ToSpec);
        if !specs.is_empty() {
            sections.push(format!(
                "Spec created earlier in this Run (the reference for to-tickets: fetch it and read its full body and comments, and use it as the tickets' parent):\n{}",
                specs.join("\n")
            ));
        }
    }
    sections.push(
        "Issue tracker configuration: read the repository's AGENTS.md or CLAUDE.md and the files they point to (such as docs/agents/issue-tracker.md and docs/agents/triage-labels.md). Only ask the user to run /setup-matt-pocock-skills when none of them configure a tracker.".to_owned(),
    );
    sections.push(format!(
        "Continuation instruction:\nApply the selected {} skill to the Item using the conversation and decisions above. Keep working in the same working directory. Ask any confirmation questions as one grouped frontier that follows the output contract below. Wait for an explicit user decision to finish or stop; never mark the Item Done automatically.",
        action.as_str()
    ));
    sections.push(GRILL_OUTPUT_CONTRACT.to_owned());
    sections.push(downstream_issue_event_instruction(run.id, action));
    Ok(sections.join("\n\n"))
}

pub fn compose_plan_go_prompt(run: &Run) -> Result<String, DomainError> {
    if run.workflow != Workflow::Pstack
        || run.execution_profile != ExecutionProfile::Plan
        || run.plan_phase != Some(PlanPhase::AwaitingGo)
    {
        return Err(DomainError::PlanGoNotAvailable { run_id: run.id });
    }
    let plan_path = run
        .plan_path
        .as_deref()
        .unwrap_or("the plan you just wrote");
    Ok(format!(
        "{}\n\nThe user approved continuing this Plan Run by selecting Go. Continue in this same Run, Pane, and working directory. Read and execute the plan at `{plan_path}`. Implement its phases, perform the required verification, and report the resulting changes and checks. Do not rewrite the plan unless implementation reveals a concrete blocker.",
        GrillLanguage::from_run_prompt(&run.prompt).response_instruction(),
    ))
}

fn downstream_issue_event_instruction(run_id: i64, action: GrillContinuationAction) -> String {
    format!(
        "Mission Manager links the work objects you create to the Item. For to-tickets, set each ticket's native parent to the Spec when the tracker supports it; treat that relation write as best-effort, so a rejection must not stop creation. Mission Manager records the parent locally and never reads tracker relations back. Immediately after creating each object, print one JSON event on a line by itself. Use its canonical URL, or its local Markdown path for files under a registered checkout. Keep objects in publication order; use a 1-based ordinal for each ticket and list any tickets it is blocked by as their URLs or paths in blocked_by. Example: AI_MISSION_MANAGER_EVENT {{\"event\":\"external.object.created\",\"url\":\"<canonical URL or local path>\",\"ordinal\":1,\"blocked_by\":[],\"run_id\":{},\"action\":\"{}\"}}",
        run_id,
        action.as_str(),
    )
}

/// The pending question group of a step the user moved on from.
fn open_grill_questions(run: &Run) -> Option<String> {
    if run.grill_phase != Some(GrillPhase::WaitingForAnswers) {
        return None;
    }
    let group = run.grill_question_group.as_ref()?;
    let questions = group
        .questions
        .iter()
        .map(|question| {
            let mut line = format!("Q{}: ", question.number);
            if let Some(title) = &question.title {
                line.push_str(title);
                line.push(' ');
            }
            line.push_str(&question.prompt.replace('\n', " "));
            if let Some(recommendation) = &question.recommendation {
                line.push_str(&format!(" (recommended: {recommendation})"));
            }
            line
        })
        .collect::<Vec<_>>();
    (!questions.is_empty()).then(|| questions.join("\n"))
}

/// Issues a downstream action of this Run already linked to its Item.
fn downstream_issue_urls(
    state: &DomainState,
    run_id: i64,
    action: GrillContinuationAction,
) -> Vec<String> {
    state
        .links
        .iter()
        .filter(|link| {
            link.provenance.as_ref().is_some_and(|provenance| {
                provenance.run_id == run_id && provenance.action == action
            })
        })
        .filter_map(|link| {
            state
                .external_objects
                .iter()
                .find(|object| object.id == link.external_object_id)
                .map(|object| object.canonical_url.clone())
        })
        .collect()
}

fn format_recorded_grill_decisions(decisions: &[GrillAnswer]) -> String {
    decisions
        .iter()
        .map(|decision| {
            let answer = decision.answer.trim().replace('\n', "\n   ");
            format!("Q{}: {answer}", decision.question_number)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn clean_name<E>(name: String, empty_error: E) -> Result<String, E> {
    let name = name.trim();
    if name.is_empty() {
        return Err(empty_error);
    }
    Ok(name.to_owned())
}

pub(crate) fn clean_machine_transport(
    transport: MachineTransport,
) -> Result<MachineTransport, DomainError> {
    match transport {
        MachineTransport::Local => Ok(MachineTransport::Local),
        MachineTransport::Ssh {
            host,
            user,
            port,
            identity_file,
            known_hosts_file,
            strict_host_key_checking,
        } => {
            let host = host.trim().to_owned();
            if host.is_empty() {
                return Err(DomainError::EmptyMachineHost);
            }
            if !host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".@:_-".contains(&byte))
            {
                return Err(DomainError::InvalidMachineHost);
            }
            let user = user.map(|value| value.trim().to_owned());
            if user.as_deref().is_some_and(str::is_empty) {
                return Err(DomainError::EmptyMachineUser);
            }
            if user.as_deref().is_some_and(|value| {
                !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            }) {
                return Err(DomainError::InvalidMachineUser);
            }
            if port == Some(0) {
                return Err(DomainError::InvalidMachinePort);
            }
            if strict_host_key_checking
                .as_deref()
                .is_some_and(|value| !matches!(value, "yes" | "accept-new" | "no"))
            {
                return Err(DomainError::InvalidMachineHostKeyChecking);
            }
            Ok(MachineTransport::Ssh {
                host,
                user,
                port,
                identity_file: identity_file.map(|value| value.trim().to_owned()),
                known_hosts_file: known_hosts_file.map(|value| value.trim().to_owned()),
                strict_host_key_checking,
            })
        }
    }
}

pub(crate) fn ensure_context(state: &DomainState, context_id: i64) -> Result<(), DomainError> {
    if state
        .contexts
        .iter()
        .any(|context| context.id == context_id)
    {
        Ok(())
    } else {
        Err(DomainError::ContextNotFound { context_id })
    }
}

pub(crate) fn item_project_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

pub(crate) fn normalize_workspace_repositories(
    state: &DomainState,
    project_id: i64,
    repositories: Vec<WorkspaceRepositoryInput>,
) -> Result<Vec<WorkspaceRepository>, DomainError> {
    let mut normalized = Vec::with_capacity(repositories.len());
    for input in repositories {
        let repository = state
            .repositories
            .iter()
            .find(|repository| repository.id == input.repository_id)
            .ok_or(DomainError::RepositoryNotFound {
                repository_id: input.repository_id,
            })?;
        if repository.project_id != project_id {
            return Err(DomainError::RepositoryProjectMismatch {
                repository_id: input.repository_id,
                project_id,
            });
        }
        if normalized
            .iter()
            .any(|selected: &WorkspaceRepository| selected.repository_id == input.repository_id)
        {
            return Err(DomainError::DuplicateRepositorySelection {
                repository_id: input.repository_id,
            });
        }
        normalized.push(WorkspaceRepository {
            repository_id: input.repository_id,
            branch: clean_name(input.branch, DomainError::EmptyBranch)?,
            base_branch: clean_name(input.base_branch, DomainError::EmptyBranch)?,
        });
    }
    Ok(normalized)
}

pub(crate) fn clean_repository_name(name: String) -> Result<String, DomainError> {
    let name = clean_name(name, DomainError::EmptyRepositoryName)?;
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(DomainError::InvalidRepositoryName);
    }
    Ok(name)
}

pub(crate) fn item_context_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.context_id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

pub(crate) fn ensure_context_execution_machine(
    state: &DomainState,
    context_id: i64,
    machine_id: i64,
) -> Result<(), DomainError> {
    let context = state
        .contexts
        .iter()
        .find(|context| context.id == context_id)
        .ok_or(DomainError::ContextNotFound { context_id })?;
    match context.execution_machine_id {
        Some(execution_machine_id) if execution_machine_id == machine_id => Ok(()),
        Some(_) => Err(DomainError::ContextExecutionMachineMismatch {
            context_id,
            machine_id,
        }),
        None => Err(DomainError::ContextHasNoExecutionMachine { context_id }),
    }
}

pub(crate) fn ensure_item(state: &DomainState, item_id: i64) -> Result<(), DomainError> {
    if state.items.iter().any(|item| item.id == item_id) {
        Ok(())
    } else {
        Err(DomainError::ItemNotFound { item_id })
    }
}

pub(crate) fn upsert_snapshot(state: &mut DomainState, snapshot: ExternalSnapshot) {
    if let Some(existing) = state
        .snapshots
        .iter_mut()
        .find(|existing| existing.external_object_id == snapshot.external_object_id)
    {
        *existing = snapshot;
    } else {
        state.snapshots.push(snapshot);
    }
}

pub(crate) fn link_external_object(
    state: &mut DomainState,
    item_id: i64,
    object: ExternalObjectInput,
    snapshot: Option<ExternalSnapshotData>,
    provenance: Option<LinkProvenance>,
    allow_existing_link: bool,
) -> Result<Vec<Effect>, DomainError> {
    ensure_item(state, item_id)?;
    if object.canonical_url.trim().is_empty() {
        return Err(DomainError::EmptyExternalUrl);
    }
    if object.external_key.trim().is_empty() {
        return Err(DomainError::EmptyExternalObjectKey);
    }

    let existing_object = state
        .external_objects
        .iter()
        .find(|candidate| {
            candidate.provider == object.provider && candidate.external_key == object.external_key
        })
        .cloned();
    let (external_object, is_new_object) = match existing_object {
        Some(object) => (object, false),
        None => {
            let id = state.next_external_object_id;
            let next_external_object_id =
                id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let object = ExternalObject {
                id,
                provider: object.provider,
                kind: object.kind,
                external_key: object.external_key,
                canonical_url: object.canonical_url.trim().to_owned(),
            };
            state.next_external_object_id = next_external_object_id;
            state.external_objects.push(object.clone());
            (object, true)
        }
    };

    let mut effects = Vec::new();
    if is_new_object {
        effects.push(Effect::PersistExternalObject {
            object: external_object.clone(),
            next_external_object_id: state.next_external_object_id,
        });
    }

    if let Some(link) = state
        .links
        .iter_mut()
        .find(|link| link.item_id == item_id && link.external_object_id == external_object.id)
    {
        if !allow_existing_link {
            return Err(DomainError::LinkAlreadyExists);
        }
        if link.provenance != provenance {
            link.provenance = provenance;
            effects.push(Effect::PersistLinkState { link: link.clone() });
        }
    } else {
        let link_id = state.next_link_id;
        let next_link_id = link_id
            .checked_add(1)
            .ok_or(DomainError::SequenceExhausted)?;
        let reviewed_activity_id = state
            .activities
            .iter()
            .filter(|activity| activity.external_object_id == external_object.id)
            .map(|activity| activity.id)
            .max()
            .unwrap_or_default();
        let link = Link {
            id: link_id,
            item_id,
            external_object_id: external_object.id,
            reviewed_activity_id,
            attention_policy: None,
            watch_until: None,
            review_at: None,
            purpose: provenance
                .as_ref()
                .map(|provenance| LinkPurpose::from(provenance.action))
                .unwrap_or_default(),
            spec_external_object_id: None,
            provenance,
        };
        state.next_link_id = next_link_id;
        state.links.push(link.clone());
        effects.push(Effect::PersistLink { link, next_link_id });
    }

    if let Some(snapshot_data) = snapshot {
        let snapshot = ExternalSnapshot {
            external_object_id: external_object.id,
            title: snapshot_data.title,
            state: snapshot_data.state,
            metadata: snapshot_data.metadata,
            fetched_at: snapshot_data.fetched_at,
        };
        let snapshot_changed = state
            .snapshots
            .iter()
            .find(|existing| existing.external_object_id == snapshot.external_object_id)
            != Some(&snapshot);
        if snapshot_changed {
            upsert_snapshot(state, snapshot.clone());
            effects.push(Effect::PersistExternalSnapshot { snapshot });
        }
    }

    Ok(effects)
}

fn effective_attention_policy(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
) -> ExternalChangePolicy {
    if let Some(policy) = link.attention_policy {
        return policy;
    }
    let context_id = item_context_id(state, link.item_id).ok();
    context_id
        .and_then(|context_id| {
            state.attention_defaults.iter().find(|attention_default| {
                attention_default.context_id == context_id
                    && attention_default.object_kind == object.kind
            })
        })
        .map(|attention_default| attention_default.policy)
        .unwrap_or_else(ExternalChangePolicy::all)
}

fn attention_entry_for_link_at(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
    now: Option<&str>,
) -> Option<AttentionEntry> {
    let policy = effective_attention_policy(state, link, object);
    let watch_active = now.is_none_or(|now| {
        link.watch_until
            .as_deref()
            .is_none_or(|watch_until| watch_until > now)
    });
    let activities = state
        .activities
        .iter()
        .filter(|activity| {
            watch_active
                && activity.external_object_id == object.id
                && activity.id > link.reviewed_activity_id
        })
        .filter_map(|activity| {
            let changes = activity
                .changes
                .iter()
                .filter(|change| policy.allows(change.kind))
                .cloned()
                .collect::<Vec<_>>();
            (!changes.is_empty()).then_some(Activity {
                id: activity.id,
                external_object_id: activity.external_object_id,
                observed_at: activity.observed_at,
                changes,
            })
        })
        .collect::<Vec<_>>();
    if activities.is_empty() {
        return None;
    }

    let source_title = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .map(|snapshot| snapshot.title.clone())
        .unwrap_or_else(|| object.canonical_url.clone());
    let summary = activities
        .iter()
        .flat_map(|activity| activity.changes.iter())
        .map(format_change)
        .collect::<Vec<_>>()
        .join("; ");

    Some(AttentionEntry {
        kind: AttentionEntryKind::ExternalChange,
        link_id: link.id,
        reminder_id: None,
        run_id: None,
        queue_id: None,
        item_id: link.item_id,
        external_object_id: object.id,
        source_title,
        source_url: object.canonical_url.clone(),
        activities,
        summary,
    })
}

pub(crate) fn snapshot_changes(
    previous: &ExternalSnapshot,
    current: &ExternalSnapshot,
) -> Vec<ExternalChange> {
    let mut changes = Vec::new();
    if previous.title != current.title {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::Title,
            key: None,
            previous: Some(previous.title.clone()),
            current: Some(current.title.clone()),
        });
    }
    if previous.state != current.state {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::State,
            key: None,
            previous: Some(previous.state.clone()),
            current: Some(current.state.clone()),
        });
    }

    let mut keys = previous
        .metadata
        .iter()
        .map(|metadata| metadata.key.clone())
        .chain(current.metadata.iter().map(|metadata| metadata.key.clone()))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    for key in keys {
        let previous_value = previous
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        let current_value = current
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        if previous_value != current_value {
            changes.push(ExternalChange {
                kind: ExternalChangeKind::Metadata,
                key: Some(key),
                previous: previous_value,
                current: current_value,
            });
        }
    }
    changes
}

fn format_change(change: &ExternalChange) -> String {
    let label = match change.kind {
        ExternalChangeKind::Title => "Title".to_owned(),
        ExternalChangeKind::State => "State".to_owned(),
        ExternalChangeKind::Metadata => {
            format!("Metadata {}", change.key.as_deref().unwrap_or("value"))
        }
    };
    match (&change.previous, &change.current) {
        (Some(previous), Some(current)) => format!("{label} changed from {previous} to {current}"),
        (None, Some(current)) => format!("{label} added as {current}"),
        (Some(previous), None) => format!("{label} removed (was {previous})"),
        (None, None) => format!("{label} changed"),
    }
}
