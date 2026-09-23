//! Activity feature implementation.
//!
//! Activity owns the durable audit history and the user-facing projection of
//! observed external changes. Needs Attention remains a separate projection in
//! the domain: Activity records what was observed, while attention decides
//! what currently requires a user's involvement.

use std::sync::Mutex;

use tauri::State;

use crate::{
    app::Runtime,
    domain::{ActivityTabView, AuditAction, AuditEntry, DomainState, Effect, ObservedActivity},
};

pub(crate) fn list_audit_history(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<AuditEntry>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .list_audit_history()
}

pub(crate) fn get_activity_tab(
    state: State<'_, Mutex<Runtime>>,
) -> Result<ActivityTabView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .activity_tab()
}

impl Runtime {
    pub(crate) fn list_audit_history(&self) -> Result<Vec<AuditEntry>, String> {
        self.store
            .list_audit_history()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn activity_tab(&self) -> Result<ActivityTabView, String> {
        let audit_entries = self
            .store
            .list_audit_history()
            .map_err(|error| error.to_string())?;
        Ok(activity_tab_view(&self.state, audit_entries))
    }

    pub(crate) fn commit(&mut self, decision: crate::domain::Decision) -> Result<(), String> {
        self.commit_with_audit(decision, &[])
    }

    pub(crate) fn commit_with_audit(
        &mut self,
        decision: crate::domain::Decision,
        additional_actions: &[AuditAction],
    ) -> Result<(), String> {
        let ensure_project_workspaces = decision.effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::PersistItem { .. }
                    | Effect::PersistRepository { .. }
                    | Effect::UpdateRepository { .. }
                    | Effect::RemoveRepository { .. }
            )
        });
        let mut audit_actions = audit_actions(&self.state, &decision.effects);
        audit_actions.extend_from_slice(additional_actions);
        self.store
            .apply_with_audit(&decision.effects, &audit_actions)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        if ensure_project_workspaces {
            self.ensure_project_workspaces()?;
        }
        Ok(())
    }
}

fn activity_tab_view(state: &DomainState, audit_entries: Vec<AuditEntry>) -> ActivityTabView {
    let activities = state
        .activities
        .iter()
        .rev()
        .filter_map(|activity| {
            let object = state
                .external_objects
                .iter()
                .find(|object| object.id == activity.external_object_id)?;
            Some(ObservedActivity {
                activity: activity.clone(),
                object: object.clone(),
            })
        })
        .collect();

    ActivityTabView {
        audit_entries,
        activities,
    }
}

fn audit_actions(before: &DomainState, effects: &[Effect]) -> Vec<AuditAction> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::PersistContext { context, .. } => Some(AuditAction::ContextCreated {
                context_id: context.id,
            }),
            Effect::PersistContextGrillDefaults { .. }
            | Effect::UpdateContext { .. }
            | Effect::UpdateProject { .. } => None,
            Effect::PersistProject { project, .. } => Some(AuditAction::ProjectCreated {
                project_id: project.id,
            }),
            Effect::PersistRepository { repository, .. } => {
                Some(AuditAction::RepositoryRegistered {
                    repository_id: repository.id,
                })
            }
            Effect::ResetLocalData {
                context, project, ..
            } => Some(AuditAction::ResetBoundary {
                context_id: context.id,
                project_id: project.id,
            }),
            Effect::PersistItem { item, .. } => Some(AuditAction::ItemCreated { item_id: item.id }),
            Effect::PersistItemUpdate { item } => {
                let previous = before
                    .items
                    .iter()
                    .find(|candidate| candidate.id == item.id);
                match previous {
                    Some(previous) if previous.title != item.title => {
                        Some(AuditAction::ItemTitleChanged { item_id: item.id })
                    }
                    Some(previous) if previous.status != item.status => {
                        Some(AuditAction::ItemStatusChanged {
                            item_id: item.id,
                            from: previous.status,
                            to: item.status,
                        })
                    }
                    Some(previous) if previous.notes != item.notes => {
                        Some(AuditAction::ItemNotesChanged { item_id: item.id })
                    }
                    Some(_) => None,
                    None => Some(AuditAction::ItemNotesChanged { item_id: item.id }),
                }
            }
            Effect::PersistItemReminders { item, .. } => {
                Some(AuditAction::ItemRemindersChanged { item_id: item.id })
            }
            Effect::PersistWorkspace { .. }
            | Effect::PersistWorkspaceUpdate { .. }
            | Effect::RemoveWorkspace { .. } => None,
            Effect::RemoveRepository { repository_id } => Some(AuditAction::RepositoryDeleted {
                repository_id: *repository_id,
                workspace_count: before
                    .workspaces
                    .iter()
                    .filter(|workspace| {
                        workspace
                            .repositories
                            .iter()
                            .any(|repository| repository.repository_id == *repository_id)
                    })
                    .count()
                    .into(),
            }),
            Effect::RemoveMachine { machine_id } => Some(AuditAction::MachineDeleted {
                machine_id: *machine_id,
                run_count: before
                    .runs
                    .iter()
                    .filter(|run| run.machine_id == *machine_id)
                    .count()
                    .into(),
                worktree_count: before
                    .worktrees
                    .iter()
                    .filter(|worktree| worktree.machine_id == *machine_id)
                    .count()
                    .into(),
            }),
            Effect::RemoveRun { run_id } => Some(AuditAction::RunDeleted { run_id: *run_id }),
            Effect::RemoveLink {
                link_id,
                external_object_id,
            } => Some(AuditAction::LinkDeleted {
                link_id: *link_id,
                external_object_id: Some(*external_object_id),
                external_object_deleted: Some(
                    before
                        .links
                        .iter()
                        .filter(|link| link.external_object_id == *external_object_id)
                        .count()
                        == 1,
                ),
            }),
            Effect::RemoveExternalObject { external_object_id } => {
                Some(AuditAction::ExternalObjectDeleted {
                    external_object_id: *external_object_id,
                    link_count: Some(
                        before
                            .links
                            .iter()
                            .filter(|link| link.external_object_id == *external_object_id)
                            .count(),
                    ),
                    snapshot_count: Some(
                        before
                            .snapshots
                            .iter()
                            .filter(|snapshot| snapshot.external_object_id == *external_object_id)
                            .count(),
                    ),
                    activity_count: Some(
                        before
                            .activities
                            .iter()
                            .filter(|activity| activity.external_object_id == *external_object_id)
                            .count(),
                    ),
                })
            }
            Effect::RemoveItemCascade { summary, .. } => Some(AuditAction::ItemDeleted {
                summary: summary.clone(),
            }),
            Effect::RemoveProjectCascade { summary, .. } => Some(AuditAction::ProjectDeleted {
                summary: summary.clone(),
            }),
            Effect::RemoveContextCascade { summary, .. } => Some(AuditAction::ContextDeleted {
                summary: summary.clone(),
            }),
            Effect::PersistMachine { machine, .. } => Some(AuditAction::MachineRegistered {
                machine_id: machine.id,
            }),
            Effect::PersistMachineObservation { machine } => {
                let previous = before
                    .machines
                    .iter()
                    .find(|candidate| candidate.id == machine.id);
                (previous.is_none()
                    || previous.map(|candidate| candidate.last_observed)
                        != Some(machine.last_observed)
                    || previous.and_then(|candidate| candidate.last_observed_at)
                        != machine.last_observed_at)
                    .then_some(AuditAction::MachineObserved {
                        machine_id: machine.id,
                        observation: machine.last_observed,
                    })
            }
            Effect::PersistRun { run, .. } => Some(AuditAction::RunCreated { run_id: run.id }),
            Effect::PersistRunState { run } => {
                let previous = before.runs.iter().find(|candidate| candidate.id == run.id);
                previous
                    .filter(|previous| previous.state != run.state)
                    .map(|previous| AuditAction::RunStateChanged {
                        run_id: run.id,
                        from: previous.state,
                        to: run.state,
                    })
            }
            Effect::PersistRunTranscript { .. }
            | Effect::PersistGrillAnswers { .. }
            | Effect::PersistGrillResponse { .. } => None,
            Effect::PersistRunPaneStatus { run } => {
                let previous = before.runs.iter().find(|candidate| candidate.id == run.id);
                previous
                    .filter(|previous| previous.pane_status != run.pane_status)
                    .map(|previous| AuditAction::RunPaneStatusChanged {
                        run_id: run.id,
                        from: previous.pane_status,
                        to: run.pane_status,
                    })
            }
            Effect::PersistWorktree { .. }
            | Effect::RemoveWorktree { .. }
            | Effect::UpdateRepository { .. }
            | Effect::UpdateMachine { .. }
            | Effect::PersistRepositoryLocation { .. }
            | Effect::UpdateRepositoryLocation { .. } => None,
            Effect::PersistItemRelation { relation } => Some(AuditAction::ItemRelationChanged {
                from_item_id: relation.from_item_id,
                to_item_id: relation.to_item_id,
                kind: relation.kind,
            }),
            Effect::PersistExternalObject { object, .. } => {
                Some(AuditAction::ExternalObjectCreated {
                    external_object_id: object.id,
                })
            }
            Effect::PersistLink { link, .. } => Some(AuditAction::LinkCreated { link_id: link.id }),
            Effect::PersistLinkState { link } => {
                let previous = before
                    .links
                    .iter()
                    .find(|candidate| candidate.id == link.id);
                previous
                    .filter(|previous| previous != &link)
                    .map(|_| AuditAction::LinkUpdated { link_id: link.id })
            }
            Effect::PersistActivity { activity, .. } => {
                Some(AuditAction::ExternalObjectRefreshed {
                    external_object_id: activity.external_object_id,
                })
            }
            Effect::PersistExternalSnapshot { .. } => None,
            Effect::PersistContextAttentionDefault { attention_default } => {
                Some(AuditAction::ContextAttentionDefaultChanged {
                    context_id: attention_default.context_id,
                    object_kind: attention_default.object_kind,
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Activity, ExternalChange, ExternalChangeKind, ExternalObject, ExternalObjectKind,
        ExternalProvider,
    };

    fn state_with_activity() -> DomainState {
        DomainState {
            next_context_id: 1,
            next_project_id: 1,
            next_item_id: 1,
            next_item_number: 1,
            next_repository_id: 1,
            next_workspace_id: 1,
            next_worktree_id: 1,
            next_machine_id: 1,
            next_run_id: 1,
            next_external_object_id: 3,
            next_link_id: 1,
            next_activity_id: 4,
            next_reminder_id: 1,
            contexts: Vec::new(),
            projects: Vec::new(),
            repositories: Vec::new(),
            repository_locations: Vec::new(),
            items: Vec::new(),
            workspaces: Vec::new(),
            worktrees: Vec::new(),
            machines: Vec::new(),
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: vec![
                ExternalObject {
                    id: 1,
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "org/repo#1".into(),
                    canonical_url: "https://github.com/org/repo/issues/1".into(),
                },
                ExternalObject {
                    id: 2,
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::PullRequest,
                    external_key: "org/repo#2".into(),
                    canonical_url: "https://github.com/org/repo/pull/2".into(),
                },
            ],
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: vec![
                Activity {
                    id: 1,
                    external_object_id: 1,
                    observed_at: 10,
                    changes: vec![ExternalChange {
                        kind: ExternalChangeKind::State,
                        key: None,
                        previous: Some("open".into()),
                        current: Some("closed".into()),
                    }],
                },
                Activity {
                    id: 2,
                    external_object_id: 2,
                    observed_at: 20,
                    changes: Vec::new(),
                },
                Activity {
                    id: 3,
                    external_object_id: 1,
                    observed_at: 30,
                    changes: Vec::new(),
                },
            ],
            attention_defaults: Vec::new(),
        }
    }

    #[test]
    fn activity_projection_orders_observations_newest_first_and_keeps_audit_history() {
        let state = state_with_activity();
        let audit_entries = vec![AuditEntry {
            id: 7,
            recorded_at: 40,
            action: AuditAction::ExternalObjectRefreshed {
                external_object_id: 1,
            },
        }];

        let view = activity_tab_view(&state, audit_entries.clone());

        assert_eq!(view.audit_entries, audit_entries);
        assert_eq!(
            view.activities
                .iter()
                .map(|entry| entry.activity.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(
            view.activities
                .iter()
                .map(|entry| entry.object.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
    }
}
