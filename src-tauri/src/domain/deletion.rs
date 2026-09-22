use super::*;

pub fn plan_reset_local_data(state: &DomainState) -> ResetLocalDataPlan {
    let mut affected_records = Vec::new();
    affected_records.extend(state.contexts.iter().map(|context| ResetLocalDataRecord {
        kind: "Context".into(),
        id: context.id,
        label: context.name.clone(),
    }));
    affected_records.extend(state.projects.iter().map(|project| ResetLocalDataRecord {
        kind: "Project".into(),
        id: project.id,
        label: project.name.clone(),
    }));
    affected_records.extend(
        state
            .repositories
            .iter()
            .map(|repository| ResetLocalDataRecord {
                kind: "Repository".into(),
                id: repository.id,
                label: repository.name.clone(),
            }),
    );
    affected_records.extend(state.items.iter().map(|item| ResetLocalDataRecord {
        kind: "Item".into(),
        id: item.id,
        label: format!("{} · {}", item.human_identifier, item.title),
    }));
    affected_records.extend(
        state
            .workspaces
            .iter()
            .map(|workspace| ResetLocalDataRecord {
                kind: "Workspace".into(),
                id: workspace.id,
                label: format!("Item {}", workspace.item_id),
            }),
    );
    affected_records.extend(state.machines.iter().map(|machine| ResetLocalDataRecord {
        kind: "Machine".into(),
        id: machine.id,
        label: machine.name.clone(),
    }));
    affected_records.extend(state.runs.iter().map(|run| ResetLocalDataRecord {
        kind: "Run".into(),
        id: run.id,
        label: format!(
            "{:?} · Item {} · Machine {}",
            run.state,
            state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .map(|item| item.human_identifier.as_str())
                .unwrap_or("unknown"),
            state
                .machines
                .iter()
                .find(|machine| machine.id == run.machine_id)
                .map(|machine| machine.name.as_str())
                .unwrap_or("unknown")
        ),
    }));
    affected_records.extend(state.links.iter().map(|link| ResetLocalDataRecord {
        kind: "Link".into(),
        id: link.id,
        label: format!(
            "Item {} → {}",
            state
                .items
                .iter()
                .find(|item| item.id == link.item_id)
                .map(|item| item.human_identifier.as_str())
                .unwrap_or("unknown"),
            state
                .external_objects
                .iter()
                .find(|object| object.id == link.external_object_id)
                .map(|object| object.external_key.as_str())
                .unwrap_or("unknown")
        ),
    }));
    affected_records.extend(state.external_objects.iter().map(|external_object| {
        ResetLocalDataRecord {
            kind: "External Object".into(),
            id: external_object.id,
            label: external_object.external_key.clone(),
        }
    }));

    ResetLocalDataPlan {
        summary: ResetLocalDataSummary {
            context_count: state.contexts.len(),
            project_count: state.projects.len(),
            repository_count: state.repositories.len(),
            item_count: state.items.len(),
            workspace_count: state.workspaces.len(),
            machine_count: state.machines.len(),
            run_count: state.runs.len(),
            reminder_count: state.items.iter().map(|item| item.reminders.len()).sum(),
            relationship_count: state.relationships.len(),
            link_count: state.links.len(),
            external_object_count: state.external_objects.len(),
            snapshot_count: state.snapshots.len(),
            activity_count: state.activities.len(),
            attention_default_count: state.attention_defaults.len(),
        },
        affected_records,
        workspaces: state
            .workspaces
            .iter()
            .map(|workspace| ItemDeletionWorkspace {
                id: workspace.id,
                item_id: workspace.item_id,
            })
            .collect(),
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    }
}

pub fn plan_item_deletion(
    state: &DomainState,
    item_id: i64,
) -> Result<ItemDeletionPlan, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| workspace.item_id == item_id)
        .map(|workspace| ItemDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();
    let run_ids = state
        .runs
        .iter()
        .filter(|run| run.item_id == item_id)
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| run.item_id == item_id && run_is_active(run))
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let link_ids = state
        .links
        .iter()
        .filter(|link| link.item_id == item_id)
        .map(|link| link.id)
        .collect::<Vec<_>>();
    let linked_external_object_ids = state
        .links
        .iter()
        .filter(|link| link.item_id == item_id)
        .map(|link| link.external_object_id)
        .collect::<Vec<_>>();
    let orphaned_external_object_ids = linked_external_object_ids
        .iter()
        .copied()
        .filter(|external_object_id| {
            !state.links.iter().any(|link| {
                link.external_object_id == *external_object_id && link.item_id != item_id
            })
        })
        .collect::<Vec<_>>();

    Ok(ItemDeletionPlan {
        item_id,
        human_identifier: item.human_identifier.clone(),
        title: item.title.clone(),
        reminder_count: item.reminders.len(),
        relationship_count: state
            .relationships
            .iter()
            .filter(|relation| relation.from_item_id == item_id || relation.to_item_id == item_id)
            .count(),
        workspaces,
        run_ids,
        active_run_ids,
        link_ids,
        orphaned_snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| orphaned_external_object_ids.contains(&snapshot.external_object_id))
            .count(),
        orphaned_activity_count: state
            .activities
            .iter()
            .filter(|activity| orphaned_external_object_ids.contains(&activity.external_object_id))
            .count(),
        orphaned_external_object_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_external_object_deletion(
    state: &DomainState,
    external_object_id: i64,
) -> Result<ExternalObjectDeletionPlan, DomainError> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == external_object_id)
        .ok_or(DomainError::ExternalObjectNotFound { external_object_id })?;
    let link_ids = state
        .links
        .iter()
        .filter(|link| link.external_object_id == external_object_id)
        .map(|link| link.id)
        .collect::<Vec<_>>();

    Ok(ExternalObjectDeletionPlan {
        external_object_id,
        provider: object.provider,
        kind: object.kind,
        external_key: object.external_key.clone(),
        canonical_url: object.canonical_url.clone(),
        link_ids,
        snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.external_object_id == external_object_id)
            .count(),
        activity_count: state
            .activities
            .iter()
            .filter(|activity| activity.external_object_id == external_object_id)
            .count(),
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_repository_deletion(
    state: &DomainState,
    repository_id: i64,
) -> Result<RepositoryDeletionPlan, DomainError> {
    let repository = state
        .repositories
        .iter()
        .find(|repository| repository.id == repository_id)
        .ok_or(DomainError::RepositoryNotFound { repository_id })?;
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| {
            workspace
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
        })
        .map(|workspace| RepositoryDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();

    Ok(RepositoryDeletionPlan {
        repository_id,
        name: repository.name.clone(),
        remote_url: repository.remote_url.clone(),
        workspaces,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_machine_deletion(
    state: &DomainState,
    machine_id: i64,
) -> Result<MachineDeletionPlan, DomainError> {
    let machine = state
        .machines
        .iter()
        .find(|machine| machine.id == machine_id)
        .ok_or(DomainError::MachineNotFound { machine_id })?;
    let runs = state
        .runs
        .iter()
        .filter(|run| run.machine_id == machine_id)
        .map(|run| {
            let item = state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .ok_or(DomainError::ItemNotFound {
                    item_id: run.item_id,
                })?;
            Ok(MachineDeletionRun {
                id: run.id,
                item_id: run.item_id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                workspace_id: run.workspace_id,
                worktree_id: run.worktree_id,
                state: run.state,
                pane_status: run.pane_status,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| run.machine_id == machine_id && run_is_active(run))
        .map(|run| run.id)
        .collect();

    Ok(MachineDeletionPlan {
        machine_id,
        name: machine.name.clone(),
        runs,
        active_run_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_project_deletion(
    state: &DomainState,
    project_id: i64,
) -> Result<ParentDeletionPlan, DomainError> {
    let project = state
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or(DomainError::ProjectNotFound { project_id })?;
    plan_parent_deletion(
        state,
        None,
        Some(project_id),
        vec![project_id],
        project.name.clone(),
    )
}

pub fn plan_context_deletion(
    state: &DomainState,
    context_id: i64,
) -> Result<ParentDeletionPlan, DomainError> {
    let context = state
        .contexts
        .iter()
        .find(|context| context.id == context_id)
        .ok_or(DomainError::ContextNotFound { context_id })?;
    let project_ids = state
        .projects
        .iter()
        .filter(|project| project.context_id == context_id)
        .map(|project| project.id)
        .collect();
    plan_parent_deletion(
        state,
        Some(context_id),
        None,
        project_ids,
        context.name.clone(),
    )
}

fn plan_parent_deletion(
    state: &DomainState,
    context_id: Option<i64>,
    project_id: Option<i64>,
    project_ids: Vec<i64>,
    name: String,
) -> Result<ParentDeletionPlan, DomainError> {
    let projects = state
        .projects
        .iter()
        .filter(|project| project_ids.contains(&project.id))
        .map(|project| ParentDeletionProject {
            id: project.id,
            name: project.name.clone(),
        })
        .collect::<Vec<_>>();
    let items = state
        .items
        .iter()
        .filter(|item| project_ids.contains(&item.project_id))
        .map(|item| ParentDeletionItem {
            id: item.id,
            human_identifier: item.human_identifier.clone(),
            title: item.title.clone(),
            project_id: item.project_id,
        })
        .collect::<Vec<_>>();
    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let repositories = state
        .repositories
        .iter()
        .filter(|repository| project_ids.contains(&repository.project_id))
        .map(|repository| ParentDeletionRepository {
            id: repository.id,
            name: repository.name.clone(),
            remote_url: repository.remote_url.clone(),
            project_id: repository.project_id,
        })
        .collect::<Vec<_>>();
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| item_ids.contains(&workspace.item_id))
        .map(|workspace| ItemDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();
    let machines = context_id
        .map(|context_id| {
            state
                .machines
                .iter()
                .filter(|machine| machine.context_id == context_id)
                .map(|machine| ParentDeletionMachine {
                    id: machine.id,
                    name: machine.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let machine_ids = machines
        .iter()
        .map(|machine| machine.id)
        .collect::<Vec<_>>();
    let runs = state
        .runs
        .iter()
        .filter(|run| item_ids.contains(&run.item_id) || machine_ids.contains(&run.machine_id))
        .map(|run| {
            let item = state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .ok_or(DomainError::ItemNotFound {
                    item_id: run.item_id,
                })?;
            Ok(ParentDeletionRun {
                id: run.id,
                item_id: run.item_id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                workspace_id: run.workspace_id,
                worktree_id: run.worktree_id,
                machine_id: run.machine_id,
                state: run.state,
                pane_status: run.pane_status,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| {
            (item_ids.contains(&run.item_id) || machine_ids.contains(&run.machine_id))
                && run_is_active(run)
        })
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let link_ids = state
        .links
        .iter()
        .filter(|link| item_ids.contains(&link.item_id))
        .map(|link| link.id)
        .collect::<Vec<_>>();
    let linked_external_object_ids = state
        .links
        .iter()
        .filter(|link| item_ids.contains(&link.item_id))
        .map(|link| link.external_object_id)
        .collect::<Vec<_>>();
    let orphaned_external_object_ids = linked_external_object_ids
        .iter()
        .copied()
        .filter(|external_object_id| {
            !state.links.iter().any(|link| {
                link.external_object_id == *external_object_id && !item_ids.contains(&link.item_id)
            })
        })
        .fold(Vec::new(), |mut ids, external_object_id| {
            if !ids.contains(&external_object_id) {
                ids.push(external_object_id);
            }
            ids
        });
    let attention_defaults = context_id
        .map(|context_id| {
            state
                .attention_defaults
                .iter()
                .filter(|attention_default| attention_default.context_id == context_id)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(ParentDeletionPlan {
        context_id,
        project_id,
        name,
        projects,
        items: items.clone(),
        repositories,
        machines,
        workspaces,
        runs,
        active_run_ids,
        reminder_count: items
            .iter()
            .filter_map(|parent_item| state.items.iter().find(|item| item.id == parent_item.id))
            .map(|item| item.reminders.len())
            .sum(),
        relationship_count: state
            .relationships
            .iter()
            .filter(|relation| {
                item_ids.contains(&relation.from_item_id) || item_ids.contains(&relation.to_item_id)
            })
            .count(),
        link_ids,
        attention_defaults,
        orphaned_snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| orphaned_external_object_ids.contains(&snapshot.external_object_id))
            .count(),
        orphaned_activity_count: state
            .activities
            .iter()
            .filter(|activity| orphaned_external_object_ids.contains(&activity.external_object_id))
            .count(),
        orphaned_external_object_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub(crate) fn sorted_ids(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

pub(crate) fn parent_selection_matches(expected: &[i64], provided: Vec<i64>) -> bool {
    sorted_ids(expected.to_vec()) == sorted_ids(provided)
}

pub(crate) fn workspace_preparation_state(
    state: &DomainState,
    workspace_id: i64,
    previous: WorkspacePreparationState,
) -> WorkspacePreparationState {
    let Some(workspace) = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
    else {
        return previous;
    };
    let item_project_id = state
        .items
        .iter()
        .find(|item| item.id == workspace.item_id)
        .map(|item| item.project_id);
    let repositories = state
        .repositories
        .iter()
        .filter(|repository| Some(repository.project_id) == item_project_id)
        .collect::<Vec<_>>();
    let complete = !repositories.is_empty()
        && repositories.iter().all(|repository| {
            state.worktrees.iter().any(|worktree| {
                worktree.workspace_id == workspace_id && worktree.repository_id == repository.id
            })
        });
    if complete {
        WorkspacePreparationState::Ready
    } else if previous == WorkspacePreparationState::Resumable {
        WorkspacePreparationState::Resumable
    } else {
        WorkspacePreparationState::Pending
    }
}

pub(crate) fn run_uses_repository(state: &DomainState, run: &Run, repository_id: i64) -> bool {
    run.repository_id == Some(repository_id)
        || run
            .direct_checkouts
            .iter()
            .any(|checkout| checkout.repository_id == repository_id)
        || run.worktree_id.is_some_and(|worktree_id| {
            state.worktrees.iter().any(|worktree| {
                worktree.id == worktree_id && worktree.repository_id == repository_id
            })
        })
}
