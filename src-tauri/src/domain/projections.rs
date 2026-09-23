use super::*;

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
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Run #{} is blocked and needs your input", run.id),
                })
            }),
    );

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
            let runs = state
                .runs
                .iter()
                .filter(|run| run.item_id == item.id)
                .cloned()
                .collect();
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
    custom_prompt: Option<&str>,
) -> Result<String, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let mut sections = Vec::new();
    if selection.include_objective {
        sections.push(format!("Item objective:\n{}", item.title));
    }
    if selection.include_notes && !item.notes.trim().is_empty() {
        sections.push(format!("Item notes:\n{}", item.notes.trim()));
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
            "Implement this work using the Repositories configured for its Project or a registered Worktree, run the relevant checks, and leave the changes ready for review."
                .to_owned()
        }
        ExecutionProfile::Review => {
            "Review the current changes for this Item for correctness, regressions, and missing test coverage."
                .to_owned()
        }
        ExecutionProfile::CustomPrompt => clean_name(
            custom_prompt.unwrap_or_default().to_owned(),
            DomainError::EmptyRunPrompt,
        )?,
        ExecutionProfile::Grill => {
            "Use the selected grilling skill to ask a structured frontier of questions before recommending the next decision."
                .to_owned()
        }
    };
    sections.insert(0, instruction);
    let prompt = sections.join("\n\n");
    clean_name(prompt, DomainError::EmptyRunPrompt)
}

pub fn compose_grill_prompt(
    state: &DomainState,
    item_id: i64,
    configuration: &GrillConfiguration,
    initial_prompt: &str,
) -> Result<String, DomainError> {
    validate_grill_configuration(configuration)?;
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let initial_prompt = clean_name(initial_prompt.to_owned(), DomainError::EmptyRunPrompt)?;
    let mut context = vec![format!("Item objective:\n{}", item.title)];
    if !item.notes.trim().is_empty() {
        context.push(format!("Item notes:\n{}", item.notes.trim()));
    }
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
        "You are starting a Grill Run.\n\nGrill configuration: agent={}, model={}, effort={}.\n\nGrilling skill snapshot:\n{}\n\nRelevant Item context:\n{}\n\nUser's initial prompt:\n{}",
        serde_json::to_string(&configuration.agent).unwrap_or_else(|_| "unknown".into()),
        configuration.model,
        configuration.effort,
        GRILL_SKILL_SNAPSHOT,
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

    Ok(format!(
        "Continue the existing Grill Run in the same Run and Pane.\n\nSelected downstream action: {}\n\nDownstream skill snapshot (inject this content explicitly; do not rely on the agent having the skill installed):\n{}\n\nRelevant Item context:\n{}\n\nGrill transcript:\n{}\n\nRecorded Grill decisions:\n{}\n\nContinuation instruction:\nApply the selected {} skill to the Item using the transcript and decisions above. Keep working in the same working directory. Ask any confirmation questions using the same ❓ and ➡️ markers as one grouped frontier. Wait for an explicit user decision to finish or stop; never mark the Item Done automatically.\n\nWhen to-spec or to-tickets creates a GitHub Issue, emit one machine-readable line with the confirmed Issue URL: AI_MISSION_MANAGER_EVENT {{\"event\":\"github.issue.created\",\"url\":\"<canonical URL>\",\"run_id\":{},\"action\":\"{}\"}}",
        action.as_str(),
        action.skill_snapshot(),
        context.join("\n\n"),
        if run.transcript.trim().is_empty() {
            "No transcript was captured yet."
        } else {
            run.transcript.as_str()
        },
        decisions,
        action.as_str(),
        run.id,
        action.as_str(),
    ))
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
