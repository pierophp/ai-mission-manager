use super::deletion::{parent_selection_matches, workspace_preparation_state};
use super::projections::{
    clean_machine_transport, clean_name, clean_repository_name, ensure_context, item_context_id,
    item_project_id, link_external_object, normalize_workspace_repositories, path_is_within,
    snapshot_changes, upsert_snapshot,
};
use super::*;

pub fn decide(mut state: DomainState, event: Event) -> Result<Decision, DomainError> {
    match event {
        Event::CreateContext { name } => {
            let name = clean_name(name, DomainError::EmptyContextName)?;
            if state.contexts.iter().any(|context| context.name == name) {
                return Err(DomainError::ContextNameTaken { name });
            }

            let id = state.next_context_id;
            let next_context_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context {
                id,
                name,
                grill_defaults: GrillConfiguration::default(),
            };
            let project = Project {
                id: project_id,
                context_id: id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts.push(context.clone());
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistContext {
                        context,
                        next_context_id,
                    },
                    Effect::PersistProject {
                        project,
                        next_project_id,
                    },
                ],
            })
        }
        Event::UpdateContext { context_id, name } => {
            let name = clean_name(name, DomainError::EmptyContextName)?;
            if state
                .contexts
                .iter()
                .any(|context| context.id != context_id && context.name == name)
            {
                return Err(DomainError::ContextNameTaken { name });
            }
            let context = state
                .contexts
                .iter_mut()
                .find(|context| context.id == context_id)
                .ok_or(DomainError::ContextNotFound { context_id })?;
            context.name = name;
            let context = context.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::UpdateContext { context }],
            })
        }
        Event::SetContextGrillDefaults {
            context_id,
            defaults,
        } => {
            validate_grill_configuration(&defaults)?;
            let context = state
                .contexts
                .iter_mut()
                .find(|context| context.id == context_id)
                .ok_or(DomainError::ContextNotFound { context_id })?;
            context.grill_defaults = defaults;
            let context = context.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::PersistContextGrillDefaults { context }],
            })
        }
        Event::CreateProject {
            context_id,
            name,
            defaults,
        } => {
            let name = clean_name(name, DomainError::EmptyProjectName)?;
            ensure_context(&state, context_id)?;
            if state
                .projects
                .iter()
                .any(|project| project.context_id == context_id && project.name == name)
            {
                return Err(DomainError::ProjectNameTaken { context_id, name });
            }

            let id = state.next_project_id;
            let next_project_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project = Project {
                id,
                context_id,
                name,
                defaults,
            };

            state.next_project_id = next_project_id;
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistProject {
                    project,
                    next_project_id,
                }],
            })
        }
        Event::UpdateProject {
            project_id,
            name,
            defaults,
        } => {
            let name = clean_name(name, DomainError::EmptyProjectName)?;
            let project_context_id = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?
                .context_id;
            if state.projects.iter().any(|project| {
                project.id != project_id
                    && project.context_id == project_context_id
                    && project.name == name
            }) {
                return Err(DomainError::ProjectNameTaken {
                    context_id: project_context_id,
                    name,
                });
            }
            let project = state
                .projects
                .iter_mut()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            project.name = name;
            project.defaults = defaults;
            let project = project.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::UpdateProject { project }],
            })
        }
        Event::RegisterRepository {
            project_id,
            name,
            remote_url,
        } => {
            let name = clean_name(name, DomainError::EmptyRepositoryName)?;
            if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
                return Err(DomainError::InvalidRepositoryName);
            }
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if state
                .repositories
                .iter()
                .any(|repository| repository.project_id == project.id && repository.name == name)
            {
                return Err(DomainError::RepositoryNameTaken { project_id, name });
            }

            let id = state.next_repository_id;
            let next_repository_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let repository = Repository {
                id,
                project_id,
                name,
                remote_url,
                base_branch: "main".into(),
            };
            state.next_repository_id = next_repository_id;
            state.repositories.push(repository.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRepository {
                    repository,
                    next_repository_id,
                }],
            })
        }
        Event::UpdateRepository {
            repository_id,
            name,
            remote_url,
            base_branch,
        } => {
            let name = clean_repository_name(name)?;
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyRepositoryBaseBranch)?;
            let project_id = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?
                .project_id;
            if state.repositories.iter().any(|repository| {
                repository.id != repository_id
                    && repository.project_id == project_id
                    && repository.name == name
            }) {
                return Err(DomainError::RepositoryNameTaken { project_id, name });
            }
            let repository = state
                .repositories
                .iter_mut()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            repository.name = name;
            repository.remote_url = remote_url;
            repository.base_branch = base_branch;
            let repository = repository.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::UpdateRepository { repository }],
            })
        }
        Event::RegisterRepositoryAtLocation {
            project_id,
            name,
            remote_url,
            base_branch,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let name = clean_repository_name(name)?;
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyRepositoryBaseBranch)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            let checkout_path =
                clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?;
            let worktree_root =
                clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?;
            let location = RepositoryLocation {
                repository_id: 0,
                machine_id,
                checkout_path,
                worktree_root,
            };

            if let Some(repository) = state
                .repositories
                .iter_mut()
                .find(|repository| repository.project_id == project_id && repository.name == name)
            {
                if repository.remote_url != remote_url {
                    return Err(DomainError::RepositoryRemoteMismatch { project_id, name });
                }
                if state.repository_locations.iter().any(|candidate| {
                    candidate.repository_id == repository.id && candidate.machine_id == machine_id
                }) {
                    return Err(DomainError::RepositoryLocationAlreadyExists {
                        repository_id: repository.id,
                        machine_id,
                    });
                }
                repository.base_branch = base_branch;
                let repository = repository.clone();
                let location = RepositoryLocation {
                    repository_id: repository.id,
                    ..location
                };
                state.repository_locations.push(location.clone());
                return Ok(Decision {
                    state,
                    effects: vec![
                        Effect::UpdateRepository { repository },
                        Effect::PersistRepositoryLocation { location },
                    ],
                });
            }

            let id = state.next_repository_id;
            let next_repository_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let repository = Repository {
                id,
                project_id,
                name,
                remote_url,
                base_branch,
            };
            let location = RepositoryLocation {
                repository_id: id,
                ..location
            };
            state.next_repository_id = next_repository_id;
            state.repositories.push(repository.clone());
            state.repository_locations.push(location.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistRepository {
                        repository,
                        next_repository_id,
                    },
                    Effect::PersistRepositoryLocation { location },
                ],
            })
        }
        Event::UpdateRepositoryLocation {
            repository_id,
            previous_machine_id,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == repository.project_id)
                .ok_or(DomainError::ProjectNotFound {
                    project_id: repository.project_id,
                })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            if let Some(previous_machine_id) = previous_machine_id {
                if !state.repository_locations.iter().any(|location| {
                    location.repository_id == repository_id
                        && location.machine_id == previous_machine_id
                }) {
                    return Err(DomainError::RepositoryLocationNotFound {
                        repository_id,
                        machine_id: previous_machine_id,
                    });
                }
            }
            if previous_machine_id != Some(machine_id)
                && state.repository_locations.iter().any(|location| {
                    location.repository_id == repository_id && location.machine_id == machine_id
                })
            {
                return Err(DomainError::RepositoryLocationAlreadyExists {
                    repository_id,
                    machine_id,
                });
            }
            let location = RepositoryLocation {
                repository_id,
                machine_id,
                checkout_path: clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?,
                worktree_root: clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?,
            };
            if let Some(previous_machine_id) = previous_machine_id {
                state.repository_locations.retain(|candidate| {
                    !(candidate.repository_id == repository_id
                        && candidate.machine_id == previous_machine_id)
                });
            }
            state.repository_locations.push(location.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::UpdateRepositoryLocation {
                    previous_machine_id,
                    location,
                }],
            })
        }
        Event::ConfigureRepositoryLocation {
            repository_id,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == repository.project_id)
                .ok_or(DomainError::ProjectNotFound {
                    project_id: repository.project_id,
                })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            if state.repository_locations.iter().any(|candidate| {
                candidate.repository_id == repository_id && candidate.machine_id == machine_id
            }) {
                return Err(DomainError::RepositoryLocationAlreadyExists {
                    repository_id,
                    machine_id,
                });
            }
            let location = RepositoryLocation {
                repository_id,
                machine_id,
                checkout_path: clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?,
                worktree_root: clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?,
            };
            state.repository_locations.push(location.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRepositoryLocation { location }],
            })
        }
        Event::ResetLocalData => {
            let active_run_ids = state
                .runs
                .iter()
                .filter(|run| run_is_active(run))
                .map(|run| run.id)
                .collect::<Vec<_>>();
            if !active_run_ids.is_empty() {
                return Err(DomainError::ResetHasActiveRuns {
                    run_ids: active_run_ids,
                });
            }
            let context_id = state.next_context_id;
            let next_context_id = context_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context {
                id: context_id,
                name: "Personal".into(),
                grill_defaults: GrillConfiguration::default(),
            };
            let project = Project {
                id: project_id,
                context_id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts = vec![context.clone()];
            state.projects = vec![project.clone()];
            state.repositories.clear();
            state.repository_locations.clear();
            state.items.clear();
            state.workspaces.clear();
            state.worktrees.clear();
            state.machines.clear();
            state.runs.clear();
            state.relationships.clear();
            state.external_objects.clear();
            state.links.clear();
            state.snapshots.clear();
            state.activities.clear();
            state.attention_defaults.clear();

            Ok(Decision {
                state,
                effects: vec![Effect::ResetLocalData {
                    context,
                    project,
                    next_context_id,
                    next_project_id,
                }],
            })
        }
        Event::DeleteRepository {
            repository_id,
            workspace_ids,
        } => {
            let plan = plan_repository_deletion(&state, repository_id)?;
            let mut expected_workspace_ids = plan
                .workspaces
                .iter()
                .map(|workspace| workspace.id)
                .collect::<Vec<_>>();
            let mut provided_workspace_ids = workspace_ids;
            expected_workspace_ids.sort_unstable();
            provided_workspace_ids.sort_unstable();
            if expected_workspace_ids != provided_workspace_ids {
                return Err(DomainError::RepositoryWorkspacesMismatch {
                    repository_id,
                    expected_workspace_ids,
                    provided_workspace_ids,
                });
            }
            if let Some(workspace_id) = plan.workspaces.iter().find_map(|workspace| {
                state
                    .runs
                    .iter()
                    .any(|run| run.workspace_id == Some(workspace.id))
                    .then_some(workspace.id)
            }) {
                return Err(DomainError::WorkspaceHasRuns { workspace_id });
            }

            for workspace in &plan.workspaces {
                state
                    .worktrees
                    .retain(|worktree| worktree.workspace_id != workspace.id);
                state
                    .workspaces
                    .retain(|candidate| candidate.id != workspace.id);
            }
            state
                .repositories
                .retain(|repository| repository.id != repository_id);
            state
                .repository_locations
                .retain(|location| location.repository_id != repository_id);

            let mut effects = plan
                .workspaces
                .iter()
                .map(|workspace| Effect::RemoveWorkspace {
                    workspace_id: workspace.id,
                })
                .collect::<Vec<_>>();
            effects.push(Effect::RemoveRepository { repository_id });

            Ok(Decision { state, effects })
        }
        Event::DeleteProject {
            project_id,
            item_ids,
            repository_ids,
            workspace_ids,
        } => {
            let plan = plan_project_deletion(&state, project_id)?;
            if !parent_selection_matches(
                &plan.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                item_ids,
            ) || !parent_selection_matches(
                &plan
                    .repositories
                    .iter()
                    .map(|repository| repository.id)
                    .collect::<Vec<_>>(),
                repository_ids,
            ) || !parent_selection_matches(
                &plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<_>>(),
                workspace_ids,
            ) {
                return Err(DomainError::ProjectDeletionPlanMismatch { project_id });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ProjectHasActiveRuns {
                    project_id,
                    run_ids: plan.active_run_ids.clone(),
                });
            }

            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            let item_ids = plan.items.iter().map(|item| item.id).collect::<Vec<_>>();
            state.items.retain(|item| !item_ids.contains(&item.id));
            state.worktrees.retain(|worktree| {
                !plan
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == worktree.workspace_id)
            });
            state.workspaces.retain(|workspace| {
                !plan
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.id == workspace.id)
            });
            state
                .runs
                .retain(|run| !plan.runs.iter().any(|candidate| candidate.id == run.id));
            state.relationships.retain(|relation| {
                !item_ids.contains(&relation.from_item_id)
                    && !item_ids.contains(&relation.to_item_id)
            });
            state.links.retain(|link| !item_ids.contains(&link.item_id));
            state
                .external_objects
                .retain(|object| !plan.orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&activity.external_object_id)
            });
            state.repositories.retain(|repository| {
                !plan
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository.id)
            });
            let repository_ids = plan
                .repositories
                .iter()
                .map(|repository| repository.id)
                .collect::<Vec<_>>();
            state
                .repository_locations
                .retain(|location| !repository_ids.contains(&location.repository_id));
            state.projects.retain(|project| project.id != project_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveProjectCascade {
                    project_id,
                    item_ids,
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workspace_ids: plan
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.id)
                        .collect(),
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteContext {
            context_id,
            project_ids,
            item_ids,
            repository_ids,
            workspace_ids,
            machine_ids,
        } => {
            let plan = plan_context_deletion(&state, context_id)?;
            if state.contexts.len() == 1 {
                return Err(DomainError::CannotDeleteLastContext);
            }
            if !parent_selection_matches(
                &plan
                    .projects
                    .iter()
                    .map(|project| project.id)
                    .collect::<Vec<_>>(),
                project_ids,
            ) || !parent_selection_matches(
                &plan.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                item_ids,
            ) || !parent_selection_matches(
                &plan
                    .repositories
                    .iter()
                    .map(|repository| repository.id)
                    .collect::<Vec<_>>(),
                repository_ids,
            ) || !parent_selection_matches(
                &plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<_>>(),
                workspace_ids,
            ) || !parent_selection_matches(
                &plan
                    .machines
                    .iter()
                    .map(|machine| machine.id)
                    .collect::<Vec<_>>(),
                machine_ids,
            ) {
                return Err(DomainError::ContextDeletionPlanMismatch { context_id });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ContextHasActiveRuns {
                    context_id,
                    run_ids: plan.active_run_ids.clone(),
                });
            }

            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            let item_ids = plan.items.iter().map(|item| item.id).collect::<Vec<_>>();
            let project_ids = plan
                .projects
                .iter()
                .map(|project| project.id)
                .collect::<Vec<_>>();
            let machine_ids = plan
                .machines
                .iter()
                .map(|machine| machine.id)
                .collect::<Vec<_>>();
            state.contexts.retain(|context| context.id != context_id);
            state
                .projects
                .retain(|project| !project_ids.contains(&project.id));
            state.items.retain(|item| !item_ids.contains(&item.id));
            state.repositories.retain(|repository| {
                !plan
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository.id)
            });
            let repository_ids = plan
                .repositories
                .iter()
                .map(|repository| repository.id)
                .collect::<Vec<_>>();
            state
                .repository_locations
                .retain(|location| !repository_ids.contains(&location.repository_id));
            state.worktrees.retain(|worktree| {
                !plan
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == worktree.workspace_id)
            });
            state.workspaces.retain(|workspace| {
                !plan
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.id == workspace.id)
            });
            state
                .runs
                .retain(|run| !plan.runs.iter().any(|candidate| candidate.id == run.id));
            state
                .machines
                .retain(|machine| !machine_ids.contains(&machine.id));
            state.relationships.retain(|relation| {
                !item_ids.contains(&relation.from_item_id)
                    && !item_ids.contains(&relation.to_item_id)
            });
            state.links.retain(|link| !item_ids.contains(&link.item_id));
            state
                .external_objects
                .retain(|object| !plan.orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&activity.external_object_id)
            });
            state
                .attention_defaults
                .retain(|attention_default| attention_default.context_id != context_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveContextCascade {
                    context_id,
                    project_ids,
                    item_ids,
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workspace_ids: plan
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.id)
                        .collect(),
                    machine_ids,
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteMachine {
            machine_id,
            run_ids,
        } => {
            let plan = plan_machine_deletion(&state, machine_id)?;
            let mut expected_run_ids = plan.runs.iter().map(|run| run.id).collect::<Vec<_>>();
            let mut provided_run_ids = run_ids;
            expected_run_ids.sort_unstable();
            provided_run_ids.sort_unstable();
            if expected_run_ids != provided_run_ids {
                return Err(DomainError::MachineRunsMismatch {
                    machine_id,
                    expected_run_ids,
                    provided_run_ids,
                });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::MachineHasActiveRuns {
                    machine_id,
                    run_ids: plan.active_run_ids,
                });
            }

            state.runs.retain(|run| run.machine_id != machine_id);
            state.machines.retain(|machine| machine.id != machine_id);
            state
                .repository_locations
                .retain(|location| location.machine_id != machine_id);
            let mut effects = plan
                .runs
                .iter()
                .map(|run| Effect::RemoveRun { run_id: run.id })
                .collect::<Vec<_>>();
            effects.push(Effect::RemoveMachine { machine_id });

            Ok(Decision { state, effects })
        }
        Event::CreateItem {
            title,
            context_id,
            project_id,
        } => {
            if title.trim().is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            ensure_context(&state, context_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if project.context_id != context_id {
                return Err(DomainError::ProjectContextMismatch {
                    project_id,
                    context_id,
                });
            }

            let id = state.next_item_id;
            let number = state.next_item_number;
            let next_item_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let next_item_number = number
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let item = Item {
                id,
                human_identifier: format!("MC-{number}"),
                title: title.trim().to_owned(),
                project_id,
                status: project.defaults.item_status,
                notes: String::new(),
                reminders: Vec::new(),
            };

            state.next_item_id = next_item_id;
            state.next_item_number = next_item_number;
            state.items.push(item.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItem {
                    item,
                    next_item_number,
                    next_item_id,
                }],
            })
        }
        Event::CreateWorkspace {
            item_id,
            repositories,
        } => {
            let project_id = item_project_id(&state, item_id)?;
            if repositories.is_empty() {
                return Err(DomainError::EmptyWorkspaceRepositories);
            }
            let repositories = normalize_workspace_repositories(&state, project_id, repositories)?;
            let id = state.next_workspace_id;
            let next_workspace_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let workspace = Workspace {
                id,
                item_id,
                repositories,
                preparation_state: WorkspacePreparationState::Pending,
            };
            state.next_workspace_id = next_workspace_id;
            state.workspaces.push(workspace.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkspace {
                    workspace,
                    next_workspace_id,
                }],
            })
        }
        Event::CreateWorktree {
            workspace_id,
            repository_id,
            machine_id,
            path,
            branch,
            base_branch,
            is_dirty,
        } => {
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::WorktreeRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            if state.worktrees.iter().any(|worktree| {
                worktree.workspace_id == workspace_id && worktree.repository_id == repository_id
            }) {
                return Err(DomainError::WorktreeAlreadyExists {
                    repository_id,
                    workspace_id,
                });
            }
            let item_context_id = item_context_id(&state, workspace.item_id)?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != item_context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: item_context_id,
                });
            }
            let path = clean_name(path, DomainError::EmptyWorktreePath)?;
            let branch = clean_name(branch, DomainError::EmptyBranch)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyBranch)?;
            let id = state.next_worktree_id;
            let next_worktree_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let worktree = Worktree {
                id,
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty,
            };
            state.next_worktree_id = next_worktree_id;
            state.worktrees.push(worktree.clone());

            let preparation_state =
                workspace_preparation_state(&state, workspace_id, workspace.preparation_state);
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .expect("the Workspace was checked above");
            let workspace_changed = workspace.preparation_state != preparation_state;
            workspace.preparation_state = preparation_state;
            let workspace = workspace.clone();
            let mut effects = vec![Effect::PersistWorktree {
                worktree,
                next_worktree_id,
            }];
            if workspace_changed {
                effects.push(Effect::PersistWorkspaceUpdate { workspace });
            }

            Ok(Decision { state, effects })
        }
        Event::MarkWorkspaceResumable { workspace_id } => {
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            workspace.preparation_state = WorkspacePreparationState::Resumable;
            let workspace = workspace.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkspaceUpdate { workspace }],
            })
        }
        Event::RemoveWorktree { worktree_id } => {
            let position = state
                .worktrees
                .iter()
                .position(|worktree| worktree.id == worktree_id)
                .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
            let workspace_id = state.worktrees[position].workspace_id;
            state.worktrees.remove(position);
            let previous_workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .cloned()
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            let preparation_state = workspace_preparation_state(
                &state,
                workspace_id,
                previous_workspace.preparation_state,
            );
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .expect("the Workspace was checked above");
            workspace.preparation_state = preparation_state;
            let workspace = workspace.clone();

            Ok(Decision {
                state,
                effects: vec![
                    Effect::RemoveWorktree { worktree_id },
                    Effect::PersistWorkspaceUpdate { workspace },
                ],
            })
        }
        Event::RemoveWorkspace { workspace_id } => {
            if !state
                .workspaces
                .iter()
                .any(|workspace| workspace.id == workspace_id)
            {
                return Err(DomainError::WorkspaceNotFound { workspace_id });
            }
            if state
                .runs
                .iter()
                .any(|run| run.workspace_id == Some(workspace_id))
            {
                return Err(DomainError::WorkspaceHasRuns { workspace_id });
            }
            state
                .worktrees
                .retain(|worktree| worktree.workspace_id != workspace_id);
            state
                .workspaces
                .retain(|workspace| workspace.id != workspace_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveWorkspace { workspace_id }],
            })
        }
        Event::RegisterMachine {
            context_id,
            name,
            socket_name,
            transport,
        } => {
            ensure_context(&state, context_id)?;
            let name = clean_name(name, DomainError::EmptyMachineName)?;
            let socket_name = clean_name(socket_name, DomainError::EmptyMachineSocketName)?;
            let transport = clean_machine_transport(transport)?;
            if state
                .machines
                .iter()
                .any(|machine| machine.context_id == context_id && machine.name == name)
            {
                return Err(DomainError::MachineNameTaken { context_id, name });
            }
            let id = state.next_machine_id;
            let next_machine_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let machine = Machine {
                id,
                context_id,
                name,
                socket_name,
                transport,
                last_observed: MachineObservation::Unknown,
                last_observed_at: None,
            };
            state.next_machine_id = next_machine_id;
            state.machines.push(machine.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistMachine {
                    machine,
                    next_machine_id,
                }],
            })
        }
        Event::UpdateMachine {
            machine_id,
            name,
            socket_name,
            transport,
        } => {
            let machine_context_id = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?
                .context_id;
            let name = clean_name(name, DomainError::EmptyMachineName)?;
            let socket_name = clean_name(socket_name, DomainError::EmptyMachineSocketName)?;
            let transport = clean_machine_transport(transport)?;
            if state.machines.iter().any(|machine| {
                machine.id != machine_id
                    && machine.context_id == machine_context_id
                    && machine.name == name
            }) {
                return Err(DomainError::MachineNameTaken {
                    context_id: machine_context_id,
                    name,
                });
            }
            let machine = state
                .machines
                .iter_mut()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            machine.name = name;
            machine.socket_name = socket_name;
            machine.transport = transport;
            let machine = machine.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::UpdateMachine { machine }],
            })
        }
        Event::ObserveMachine {
            machine_id,
            observation,
            observed_at,
        } => {
            let machine = state
                .machines
                .iter_mut()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            machine.last_observed = observation;
            machine.last_observed_at = Some(observed_at);
            let machine = machine.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::PersistMachineObservation { machine }],
            })
        }
        Event::DeleteItem { item_id } => {
            let plan = plan_item_deletion(&state, item_id)?;
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ItemHasActiveRuns {
                    item_id,
                    run_ids: plan.active_run_ids,
                });
            }
            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            state.items.retain(|item| item.id != item_id);
            state.worktrees.retain(|worktree| {
                !state.workspaces.iter().any(|workspace| {
                    workspace.id == worktree.workspace_id && workspace.item_id == item_id
                })
            });
            state
                .workspaces
                .retain(|workspace| workspace.item_id != item_id);
            state.runs.retain(|run| run.item_id != item_id);
            state.relationships.retain(|relation| {
                relation.from_item_id != item_id && relation.to_item_id != item_id
            });
            state.links.retain(|link| link.item_id != item_id);
            state
                .external_objects
                .retain(|object| !orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !orphaned_external_object_ids.contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !orphaned_external_object_ids.contains(&activity.external_object_id)
            });

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveItemCascade {
                    item_id,
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteLink { link_id } => {
            let link = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .cloned()
                .ok_or(DomainError::LinkNotFound { link_id })?;
            let external_object_id = link.external_object_id;
            state.links.retain(|candidate| candidate.id != link_id);
            let orphaned = !state
                .links
                .iter()
                .any(|candidate| candidate.external_object_id == external_object_id);
            if orphaned {
                state
                    .external_objects
                    .retain(|object| object.id != external_object_id);
                state
                    .snapshots
                    .retain(|snapshot| snapshot.external_object_id != external_object_id);
                state
                    .activities
                    .retain(|activity| activity.external_object_id != external_object_id);
            }

            let mut effects = vec![Effect::RemoveLink {
                link_id,
                external_object_id,
            }];
            if orphaned {
                effects.push(Effect::RemoveExternalObject { external_object_id });
            }
            Ok(Decision { state, effects })
        }
        Event::DeleteExternalObject { external_object_id } => {
            plan_external_object_deletion(&state, external_object_id)?;
            state
                .links
                .retain(|link| link.external_object_id != external_object_id);
            state
                .external_objects
                .retain(|object| object.id != external_object_id);
            state
                .snapshots
                .retain(|snapshot| snapshot.external_object_id != external_object_id);
            state
                .activities
                .retain(|activity| activity.external_object_id != external_object_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveExternalObject { external_object_id }],
            })
        }
        Event::DeleteRun { run_id } => {
            let position = state
                .runs
                .iter()
                .position(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            let run_state = state.runs[position].state;
            if !run_is_finished(&state.runs[position]) {
                return Err(DomainError::RunNotFinished {
                    run_id,
                    state: run_state,
                });
            }
            state.runs.remove(position);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveRun { run_id }],
            })
        }
        Event::StartDirectRun {
            item_id,
            workspace_id,
            machine_id,
            agent,
            execution_profile,
            prompt,
            working_directory,
            session_name,
            pane_id,
            started_at,
            prompt_selection,
            checkouts,
            repository_id,
            allow_dirty,
            allow_shared_checkouts,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if checkouts.is_empty() {
                return Err(DomainError::EmptyDirectRunCheckouts);
            }
            if !workspace
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            let selected_repository_ids = workspace
                .repositories
                .iter()
                .map(|repository| repository.repository_id)
                .collect::<Vec<_>>();
            let mut checkout_repository_ids = Vec::new();
            for checkout in &checkouts {
                if checkout.path.trim().is_empty()
                    || checkout.branch.trim().is_empty()
                    || !selected_repository_ids.contains(&checkout.repository_id)
                    || checkout_repository_ids.contains(&checkout.repository_id)
                {
                    return Err(DomainError::InvalidDirectRunCheckout {
                        repository_id: checkout.repository_id,
                    });
                }
                checkout_repository_ids.push(checkout.repository_id);
            }
            if checkout_repository_ids.len() != selected_repository_ids.len() {
                return Err(DomainError::InvalidDirectRunCheckout {
                    repository_id: selected_repository_ids
                        .into_iter()
                        .find(|repository_id| !checkout_repository_ids.contains(repository_id))
                        .unwrap_or_default(),
                });
            }
            for external_object_id in prompt_selection.external_object_ids {
                if !state.links.iter().any(|link| {
                    link.item_id == item_id && link.external_object_id == external_object_id
                }) {
                    return Err(DomainError::RunPromptSourceNotLinked {
                        external_object_id,
                        item_id,
                    });
                }
            }
            let dirty_repository_ids = checkouts
                .iter()
                .filter(|checkout| checkout.is_dirty)
                .map(|checkout| checkout.repository_id)
                .collect::<Vec<_>>();
            if !allow_dirty && !dirty_repository_ids.is_empty() {
                return Err(DomainError::DirectRunDirtyCheckouts {
                    repository_ids: dirty_repository_ids,
                });
            }
            let mut shared_run_ids = Vec::new();
            let mut shared_paths = Vec::new();
            for active_run in state.runs.iter().filter(|run| {
                run.machine_id == machine_id
                    && run_is_active(run)
                    && run.pane_status != RunPaneStatus::Missing
            }) {
                for checkout in &checkouts {
                    if active_run
                        .direct_checkouts
                        .iter()
                        .any(|active_checkout| active_checkout.path == checkout.path)
                    {
                        shared_run_ids.push(active_run.id);
                        shared_paths.push(checkout.path.clone());
                    }
                }
            }
            shared_run_ids.sort_unstable();
            shared_run_ids.dedup();
            shared_paths.sort();
            shared_paths.dedup();
            if !allow_shared_checkouts && !shared_run_ids.is_empty() {
                return Err(DomainError::DirectRunSharedCheckouts {
                    run_ids: shared_run_ids,
                    paths: shared_paths,
                });
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if !checkouts
                .iter()
                .any(|checkout| checkout.path == working_directory)
            {
                return Err(DomainError::InvalidDirectRunCheckout {
                    repository_id: checkouts[0].repository_id,
                });
            }
            if checkouts
                .iter()
                .find(|checkout| checkout.repository_id == repository_id)
                .map(|checkout| checkout.path.as_str())
                != Some(working_directory.as_str())
            {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch { repository_id });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id: None,
                machine_id,
                agent,
                execution_profile,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: checkouts,
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
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
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            let worktree = state
                .worktrees
                .iter()
                .find(|worktree| worktree.id == worktree_id)
                .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
            if worktree.workspace_id != workspace_id {
                return Err(DomainError::RunWorktreeWorkspaceMismatch {
                    worktree_id,
                    workspace_id,
                });
            }
            if worktree.machine_id != machine_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == worktree.repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id: worktree.repository_id,
                    workspace_id,
                });
            }
            for external_object_id in prompt_selection.external_object_ids {
                if !state.links.iter().any(|link| {
                    link.item_id == item_id && link.external_object_id == external_object_id
                }) {
                    return Err(DomainError::RunPromptSourceNotLinked {
                        external_object_id,
                        item_id,
                    });
                }
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if working_directory != worktree.path {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                    repository_id: worktree.repository_id,
                });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(worktree.repository_id),
                worktree_id: Some(worktree_id),
                machine_id,
                agent,
                execution_profile,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::StartGrillRun {
            item_id,
            workspace_id,
            repository_id,
            machine_id,
            configuration,
            prompt,
            skill_snapshot,
            working_directory,
            session_name,
            pane_id,
            started_at,
            checkouts,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            validate_grill_configuration(&configuration)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == repository.project_id)
                .ok_or(DomainError::ProjectNotFound {
                    project_id: repository.project_id,
                })?;
            if project.context_id != context_id {
                return Err(DomainError::RepositoryProjectMismatch {
                    repository_id,
                    project_id: project.id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if let Some(run) = state.runs.iter().find(|run| {
                run.item_id == item_id
                    && run.execution_profile == ExecutionProfile::Grill
                    && run_is_active(run)
            }) {
                return Err(DomainError::ActiveGrillRun {
                    item_id,
                    run_id: run.id,
                });
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            if skill_snapshot.trim().is_empty() {
                return Err(DomainError::EmptyGrillSkillSnapshot);
            }
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            let selected_repository_ids = workspace
                .repositories
                .iter()
                .map(|repository| repository.repository_id)
                .collect::<Vec<_>>();
            let mut checkout_repository_ids = Vec::new();
            for checkout in &checkouts {
                if checkout.path.trim().is_empty()
                    || checkout.branch.trim().is_empty()
                    || !selected_repository_ids.contains(&checkout.repository_id)
                    || checkout_repository_ids.contains(&checkout.repository_id)
                {
                    return Err(DomainError::InvalidDirectRunCheckout {
                        repository_id: checkout.repository_id,
                    });
                }
                checkout_repository_ids.push(checkout.repository_id);
            }
            if checkout_repository_ids.len() != selected_repository_ids.len()
                || !checkouts
                    .iter()
                    .any(|checkout| checkout.path == working_directory)
            {
                return Err(DomainError::InvalidDirectRunCheckout { repository_id });
            }
            if checkouts
                .iter()
                .find(|checkout| checkout.repository_id == repository_id)
                .map(|checkout| checkout.path.as_str())
                != Some(working_directory.as_str())
            {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch { repository_id });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id: None,
                machine_id,
                agent: configuration.agent,
                execution_profile: ExecutionProfile::Grill,
                model: Some(configuration.model),
                effort: Some(configuration.effort),
                skill_snapshot: Some(skill_snapshot),
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: checkouts,
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: Some(GrillPhase::Starting),
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::AttachRun {
            item_id,
            workspace_id,
            worktree_id,
            repository_id,
            machine_id,
            agent,
            working_directory,
            machine_home,
            session_name,
            pane_id,
            attached_at,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            if let Some(worktree_id) = worktree_id {
                let worktree = state
                    .worktrees
                    .iter()
                    .find(|worktree| worktree.id == worktree_id)
                    .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
                if worktree.workspace_id != workspace_id
                    || worktree.repository_id != repository_id
                    || worktree.path != working_directory
                {
                    return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                        repository_id,
                    });
                }
            } else {
                let location_matches = state.repository_locations.iter().any(|location| {
                    location.repository_id == repository_id
                        && location.machine_id == machine_id
                        && path_is_within(
                            &location.checkout_path,
                            &working_directory,
                            &machine_home,
                        )
                });
                if !location_matches {
                    return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                        repository_id,
                    });
                }
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id,
                machine_id,
                agent,
                execution_profile: ExecutionProfile::CustomPrompt,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt: "Attached existing agent".into(),
                working_directory,
                session_name,
                pane_id,
                started_at: attached_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::UpdateRunState {
            run_id,
            state: run_state,
        } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.state = run_state;
            if run.execution_profile == ExecutionProfile::Grill {
                run.grill_phase = match run_state {
                    RunState::Unknown => run.grill_phase.or(Some(GrillPhase::Starting)),
                    RunState::Working => Some(GrillPhase::Working),
                    RunState::Blocked => Some(GrillPhase::WaitingForAnswers),
                    RunState::Finished => Some(GrillPhase::AwaitingNextAction),
                };
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::FinishRun { run_id } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.state = RunState::Finished;
            if run.execution_profile == ExecutionProfile::Grill {
                run.grill_phase = Some(GrillPhase::Finished);
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::ContinueGrill { run_id, action } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill
                || run.grill_phase != Some(GrillPhase::AwaitingNextAction)
                || run.pane_status != RunPaneStatus::Available
            {
                return Err(DomainError::GrillContinuationNotAvailable {
                    run_id,
                    phase: run.grill_phase,
                });
            }
            run.state = RunState::Working;
            run.grill_phase = Some(GrillPhase::Working);
            run.grill_question_group = None;
            run.grill_answers.clear();
            run.grill_response = None;
            run.grill_action = Some(action);
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::CaptureDownstreamIssues {
            run_id,
            action,
            issues,
        } => {
            let run = state
                .runs
                .iter()
                .find(|run| run.id == run_id)
                .cloned()
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill
                || run.grill_action != Some(action)
                || !matches!(
                    action,
                    GrillContinuationAction::ToSpec | GrillContinuationAction::ToTickets
                )
            {
                return Err(DomainError::DownstreamCaptureNotAvailable { run_id, action });
            }

            let mut effects = Vec::new();
            for issue in issues {
                if issue.object.provider != ExternalProvider::GitHub
                    || issue.object.kind != ExternalObjectKind::Issue
                    || issue.object.external_key.trim().is_empty()
                    || issue.object.canonical_url.trim().is_empty()
                {
                    continue;
                }
                effects.extend(link_external_object(
                    &mut state,
                    run.item_id,
                    issue.object,
                    Some(issue.snapshot),
                    Some(LinkProvenance {
                        run_id,
                        action,
                        discovery: issue.discovery,
                    }),
                    true,
                )?);
            }

            Ok(Decision { state, effects })
        }
        Event::RecordRunTranscript {
            run_id,
            transcript,
            question_group,
        } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill {
                return Err(DomainError::NotGrillRun { run_id });
            }
            run.transcript = transcript;
            if run.grill_question_group != question_group {
                run.grill_answers.clear();
                run.grill_response = None;
                run.grill_question_group = question_group;
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunTranscript { run }],
            })
        }
        Event::RecordGrillAnswers { run_id, answers } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill {
                return Err(DomainError::NotGrillRun { run_id });
            }
            if run.grill_response.is_some() {
                return Err(DomainError::GrillResponseAlreadySubmitted { run_id });
            }
            let question_group = run
                .grill_question_group
                .as_ref()
                .ok_or(DomainError::GrillQuestionGroupNotFound { run_id })?;
            let mut normalized_answers = answers;
            normalized_answers.sort_by_key(|answer| answer.question_number);
            if normalized_answers.len() != question_group.questions.len() {
                return Err(DomainError::GrillQuestionGroupNotFound { run_id });
            }
            for answer in &normalized_answers {
                if !question_group
                    .questions
                    .iter()
                    .any(|question| question.number == answer.question_number)
                {
                    return Err(DomainError::UnknownGrillQuestion {
                        question_number: answer.question_number,
                    });
                }
            }
            format_grill_response(&normalized_answers)?;
            run.grill_decisions
                .extend(normalized_answers.iter().cloned());
            run.grill_answers = normalized_answers;
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistGrillAnswers { run }],
            })
        }
        Event::RecordGrillResponse { run_id, response } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill {
                return Err(DomainError::NotGrillRun { run_id });
            }
            if run.grill_answers.is_empty() {
                return Err(DomainError::EmptyGrillAnswer);
            }
            let response = clean_name(response, DomainError::EmptyGrillResponse)?;
            run.grill_response = Some(response);
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistGrillResponse { run }],
            })
        }
        Event::SetRunPaneStatus { run_id, status } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.pane_status = status;
            if run.execution_profile == ExecutionProfile::Grill {
                if status != RunPaneStatus::Available && run_is_active(run) {
                    run.grill_phase = Some(GrillPhase::RecoverablePaneLoss);
                } else if status == RunPaneStatus::Available
                    && (run.grill_phase.is_none()
                        || run.grill_phase == Some(GrillPhase::RecoverablePaneLoss))
                {
                    run.grill_phase = Some(match run.state {
                        RunState::Unknown => GrillPhase::Starting,
                        RunState::Working => GrillPhase::Working,
                        RunState::Blocked => GrillPhase::WaitingForAnswers,
                        RunState::Finished => GrillPhase::AwaitingNextAction,
                    });
                }
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunPaneStatus { run }],
            })
        }
        Event::SetItemStatus { item_id, status } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.status = status;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemTitle { item_id, title } => {
            let title = title.trim();
            if title.is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.title = title.to_owned();
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemNotes { item_id, notes } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.notes = notes;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemRelation {
            from_item_id,
            to_item_id,
            kind,
        } => {
            if from_item_id == to_item_id {
                return Err(DomainError::SelfRelation {
                    item_id: from_item_id,
                });
            }
            let from_context_id = item_context_id(&state, from_item_id)?;
            let to_context_id = item_context_id(&state, to_item_id)?;
            if from_context_id != to_context_id {
                return Err(DomainError::ItemContextMismatch {
                    from_item_id,
                    to_item_id,
                });
            }

            let relation = ItemRelation {
                from_item_id,
                to_item_id,
                kind,
            };
            if state.relationships.contains(&relation) {
                return Err(DomainError::RelationAlreadyExists);
            }
            state.relationships.push(relation.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemRelation { relation }],
            })
        }
        Event::AddItemReminder { item_id, remind_at } => {
            let remind_at = clean_name(remind_at, DomainError::EmptyReminderAt)?;
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let id = state.next_reminder_id;
            let next_reminder_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            item.reminders.push(Reminder { id, remind_at });
            state.next_reminder_id = next_reminder_id;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::RemoveItemReminder {
            item_id,
            reminder_id,
        } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let position = item
                .reminders
                .iter()
                .position(|reminder| reminder.id == reminder_id)
                .ok_or(DomainError::ReminderNotFound {
                    item_id,
                    reminder_id,
                })?;
            item.reminders.remove(position);
            let item = item.clone();
            let next_reminder_id = state.next_reminder_id;

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::SetLinkWatchUntil {
            link_id,
            watch_until,
        } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.watch_until = watch_until;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetLinkReviewAt { link_id, review_at } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = review_at;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::ClearLinkReviewAt { link_id } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = None;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::LinkExternalObject {
            item_id,
            object,
            snapshot,
        } => {
            let effects = link_external_object(&mut state, item_id, object, snapshot, None, false)?;
            Ok(Decision { state, effects })
        }
        Event::RefreshExternalObject {
            external_object_id,
            snapshot: snapshot_data,
        } => {
            if !state
                .external_objects
                .iter()
                .any(|object| object.id == external_object_id)
            {
                return Err(DomainError::ExternalObjectNotFound { external_object_id });
            }
            let snapshot = ExternalSnapshot {
                external_object_id,
                title: snapshot_data.title,
                state: snapshot_data.state,
                metadata: snapshot_data.metadata,
                fetched_at: snapshot_data.fetched_at,
            };
            let changes = state
                .snapshots
                .iter()
                .find(|existing| existing.external_object_id == external_object_id)
                .map(|previous| snapshot_changes(previous, &snapshot))
                .unwrap_or_default();
            upsert_snapshot(&mut state, snapshot.clone());

            let mut effects = Vec::new();
            if !changes.is_empty() {
                let id = state.next_activity_id;
                let next_activity_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                let activity = Activity {
                    id,
                    external_object_id,
                    observed_at: snapshot.fetched_at,
                    changes,
                };
                state.next_activity_id = next_activity_id;
                state.activities.push(activity.clone());
                effects.push(Effect::PersistActivity {
                    activity,
                    next_activity_id,
                });
            }
            effects.push(Effect::PersistExternalSnapshot { snapshot });

            Ok(Decision { state, effects })
        }
        Event::SetLinkAttentionPolicy { link_id, policy } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.attention_policy = policy;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetContextAttentionDefault {
            context_id,
            object_kind,
            policy,
        } => {
            ensure_context(&state, context_id)?;
            let attention_default = ContextAttentionDefault {
                context_id,
                object_kind,
                policy,
            };
            if let Some(existing) = state.attention_defaults.iter_mut().find(|existing| {
                existing.context_id == context_id && existing.object_kind == object_kind
            }) {
                *existing = attention_default.clone();
            } else {
                state.attention_defaults.push(attention_default.clone());
            }

            Ok(Decision {
                state,
                effects: vec![Effect::PersistContextAttentionDefault { attention_default }],
            })
        }
        Event::MarkLinkReviewed { link_id } => {
            let external_object_id = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?
                .external_object_id;
            let reviewed_activity_id = state
                .activities
                .iter()
                .filter(|activity| activity.external_object_id == external_object_id)
                .map(|activity| activity.id)
                .max()
                .unwrap_or_default();
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .expect("the Link was checked above");
            link.reviewed_activity_id = reviewed_activity_id;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
    }
}
