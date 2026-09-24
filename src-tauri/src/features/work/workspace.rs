use std::sync::Mutex;

use super::*;
use crate::git::CheckoutInspection;

#[derive(Clone)]
struct WorktreeSetupSnapshot {
    workspace: Workspace,
    item: Item,
    selected: WorkspaceRepository,
    repository: Repository,
    machine: Machine,
    location: RepositoryLocation,
}

struct WorktreeSetupObservation {
    snapshot: WorktreeSetupSnapshot,
    path: String,
    inspection: CheckoutInspection,
}

impl WorktreeSetupSnapshot {
    fn destination_path(&self) -> PathBuf {
        let machine_home = machine_home_directory(&self.machine);
        let worktree_root = resolve_machine_path(&self.location.worktree_root, &machine_home);
        worktree_path(
            &worktree_root,
            self.workspace.id,
            &self.selected.branch,
            &self.repository.name,
        )
    }

    fn normalized_destination_path(&self) -> Result<String, String> {
        normalize_machine_path(
            &self.destination_path().to_string_lossy(),
            &machine_home_directory(&self.machine),
        )
        .map_err(|error| error.to_string())
    }

    fn observe_prepare(
        self,
        reuse_existing_branch: bool,
        confirm_dirty_attachment: bool,
    ) -> Result<WorktreeSetupObservation, String> {
        let machine_home = machine_home_directory(&self.machine);
        let canonical_checkout = resolve_machine_path(&self.location.checkout_path, &machine_home);
        let destination = self.destination_path();
        let path = self.normalized_destination_path()?;
        let inspection = GitCli::system()
            .prepare_worktree_on_machine(
                &self.machine,
                &self.repository,
                &canonical_checkout,
                &destination,
                &self.selected.branch,
                &self.selected.base_branch,
                reuse_existing_branch,
                confirm_dirty_attachment,
            )
            .map_err(|error| {
                format!(
                    "{error}; if Git created the Worktree before failing, it may remain at {path}"
                )
            })?;
        Ok(WorktreeSetupObservation {
            snapshot: self,
            path,
            inspection,
        })
    }

    fn observe_attach(
        self,
        path: String,
        confirm_dirty_attachment: bool,
    ) -> Result<WorktreeSetupObservation, String> {
        let machine_home = machine_home_directory(&self.machine);
        let canonical_checkout = resolve_machine_path(&self.location.checkout_path, &machine_home);
        let path =
            normalize_machine_path(&path, &machine_home).map_err(|error| error.to_string())?;
        let worktree_path = resolve_machine_path(&path, &machine_home);
        let inspection = GitCli::system()
            .validate_worktree_attachment_on_machine(
                &self.machine,
                &self.repository,
                &canonical_checkout,
                &worktree_path,
                &self.selected.branch,
                confirm_dirty_attachment,
            )
            .map_err(|error| error.to_string())?;
        Ok(WorktreeSetupObservation {
            snapshot: self,
            path,
            inspection,
        })
    }

    fn cleanup_prepared_worktree(&self, path: &str) -> Result<(), String> {
        let machine_home = machine_home_directory(&self.machine);
        let canonical_checkout = resolve_machine_path(&self.location.checkout_path, &machine_home);
        let worktree_path = resolve_machine_path(path, &machine_home);
        GitCli::system()
            .remove_worktree_on_machine(&self.machine, &canonical_checkout, &worktree_path, false)
            .map_err(|error| error.to_string())
    }
}

pub(crate) async fn prepare_worktree_with_state(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    reuse_existing_branch: bool,
    confirm_dirty_attachment: bool,
    state: &Mutex<Runtime>,
) -> Result<Worktree, String> {
    let snapshot = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.worktree_setup_snapshot(workspace_id, repository_id, machine_id)?
    };
    let worker_snapshot = snapshot.clone();
    let observation = tauri::async_runtime::spawn_blocking(move || {
        worker_snapshot.observe_prepare(reuse_existing_branch, confirm_dirty_attachment)
    })
    .await
    .map_err(|error| format!("Worktree preparation worker failed: {error}"))?;
    let partial_path = snapshot.normalized_destination_path()?;
    let (result, cleanup) = {
        let mut runtime = state
            .lock()
            .map_err(|_| {
                format!(
                    "Mission Manager state is unavailable after Worktree preparation; Git may have created a Worktree at {partial_path}"
                )
            })?;
        if !runtime.worktree_setup_snapshot_is_current(&snapshot)
            || runtime
                .ensure_worktree_not_recorded(workspace_id, repository_id)
                .is_err()
        {
            let cleanup = observation
                .ok()
                .map(|observation| (observation.snapshot, observation.path));
            (
                Err("The Workspace or Repository changed while the Worktree was prepared; review it again".into()),
                cleanup,
            )
        } else {
            match observation {
                Err(error) => {
                    let mark_error = runtime.mark_workspace_resumable(workspace_id).err();
                    (Err(format_commit_error(error, mark_error)), None)
                }
                Ok(observation) => {
                    let cleanup = (observation.snapshot.clone(), observation.path.clone());
                    let result = runtime.persist_prepared_worktree(
                        workspace_id,
                        repository_id,
                        machine_id,
                        observation.path,
                        observation.snapshot.selected.branch,
                        observation.snapshot.selected.base_branch,
                        observation.inspection.is_dirty,
                    );
                    if result.is_ok() {
                        (result, None)
                    } else {
                        (result, Some(cleanup))
                    }
                }
            }
        }
    };
    if let Some((cleanup_snapshot, cleanup_path)) = cleanup {
        let cleanup_result = tauri::async_runtime::spawn_blocking(move || {
            cleanup_snapshot.cleanup_prepared_worktree(&cleanup_path)
        })
        .await
        .map_err(|error| format!("Worktree cleanup worker failed: {error}"))?;
        return match (result, cleanup_result) {
            (Err(error), Ok(())) if error.contains("changed while") => {
                Err(format!("{error}; the newly prepared Worktree was removed"))
            }
            (Err(error), Ok(())) => {
                Err(format!("{error}; the newly prepared Worktree was removed"))
            }
            (Err(error), Err(cleanup_error)) => Err(format!(
                "{error}; the newly prepared Worktree could not be removed: {cleanup_error}"
            )),
            (Ok(worktree), _) => Ok(worktree),
        };
    }
    result
}

pub(crate) async fn attach_worktree_with_state(
    workspace_id: i64,
    repository_id: i64,
    machine_id: i64,
    path: String,
    confirm_dirty_attachment: bool,
    state: &Mutex<Runtime>,
) -> Result<Worktree, String> {
    let snapshot = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.worktree_setup_snapshot(workspace_id, repository_id, machine_id)?
    };
    let worker_snapshot = snapshot.clone();
    let observation = tauri::async_runtime::spawn_blocking(move || {
        worker_snapshot.observe_attach(path, confirm_dirty_attachment)
    })
    .await
    .map_err(|error| format!("Worktree attachment worker failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if !runtime.worktree_setup_snapshot_is_current(&snapshot)
        || runtime
            .ensure_worktree_not_recorded(workspace_id, repository_id)
            .is_err()
    {
        return Err(
            "The Workspace or Repository changed while the Worktree was inspected; review it again"
                .into(),
        );
    }
    runtime.persist_prepared_worktree(
        workspace_id,
        repository_id,
        machine_id,
        observation.path,
        observation.snapshot.selected.branch,
        observation.snapshot.selected.base_branch,
        observation.inspection.is_dirty,
    )
}

impl Runtime {
    pub(crate) fn ensure_project_workspaces(&mut self) -> Result<(), String> {
        let items = self.state.items.clone();
        for item in items {
            let workspaces = self
                .state
                .workspaces
                .iter()
                .filter(|workspace| workspace.item_id == item.id)
                .cloned()
                .collect::<Vec<_>>();

            let repositories = self
                .state
                .repositories
                .iter()
                .filter(|repository| repository.project_id == item.project_id)
                .map(|repository| WorkspaceRepositoryInput {
                    repository_id: repository.id,
                    branch: format!("mission-{}", item.human_identifier),
                    base_branch: repository.base_branch.clone(),
                })
                .collect::<Vec<_>>();
            if workspaces.is_empty() {
                if !repositories.is_empty() {
                    self.create_workspace(item.id, repositories)?;
                }
                continue;
            }
            for workspace in workspaces {
                let reconciled_repositories = repositories
                    .iter()
                    .map(|repository| {
                        workspace
                            .repositories
                            .iter()
                            .find(|selected| selected.repository_id == repository.repository_id)
                            .map(|selected| WorkspaceRepositoryInput {
                                repository_id: selected.repository_id,
                                branch: selected.branch.clone(),
                                base_branch: selected.base_branch.clone(),
                            })
                            .unwrap_or_else(|| repository.clone())
                    })
                    .collect::<Vec<_>>();
                if workspace.repositories
                    != reconciled_repositories
                        .iter()
                        .map(|repository| WorkspaceRepository {
                            repository_id: repository.repository_id,
                            branch: repository.branch.clone(),
                            base_branch: repository.base_branch.clone(),
                        })
                        .collect::<Vec<_>>()
                {
                    let decision = decide(
                        self.state.clone(),
                        Event::SetWorkspaceRepositories {
                            workspace_id: workspace.id,
                            repositories: reconciled_repositories,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    self.commit(decision)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn create_workspace(
        &mut self,
        item_id: i64,
        repositories: Vec<WorkspaceRepositoryInput>,
    ) -> Result<Workspace, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorkspace {
                item_id,
                repositories,
            },
        )
        .map_err(|error| error.to_string())?;
        let workspace = decision
            .state
            .workspaces
            .last()
            .cloned()
            .ok_or_else(|| "Workspace creation produced no Workspace".to_owned())?;
        self.commit(decision)?;
        Ok(workspace)
    }

    pub(crate) fn create_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        branch: String,
        base_branch: String,
    ) -> Result<Worktree, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorktree {
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty: false,
            },
        )
        .map_err(|error| error.to_string())?;
        let worktree = decision
            .state
            .worktrees
            .last()
            .cloned()
            .ok_or_else(|| "Worktree creation produced no Worktree".to_owned())?;
        self.commit(decision)?;
        Ok(worktree)
    }

    fn mark_workspace_resumable(&mut self, workspace_id: i64) -> Result<(), String> {
        let decision = decide(
            self.state.clone(),
            Event::MarkWorkspaceResumable { workspace_id },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)
    }

    fn worktree_inputs(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
    ) -> Result<
        (
            Workspace,
            WorkspaceRepository,
            Repository,
            Machine,
            RepositoryLocation,
        ),
        String,
    > {
        let workspace = self
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .cloned()
            .ok_or_else(|| format!("Project execution setup {workspace_id} does not exist"))?;
        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == workspace.item_id)
            .cloned()
            .ok_or_else(|| format!("Item {} does not exist", workspace.item_id))?;
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))?;
        if repository.project_id != item.project_id {
            return Err(format!(
                "Repository {repository_id} does not belong to the Item's Project"
            ));
        }
        let selected = WorkspaceRepository {
            repository_id,
            branch: format!("mission-{}", item.human_identifier),
            base_branch: repository.base_branch.clone(),
        };
        let machine = self.machine_for_item(workspace.item_id, Some(machine_id))?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == repository_id && location.machine_id == machine_id
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Repository {} has no checkout registered on Machine {}",
                    repository.name, machine.name
                )
            })?;
        Ok((workspace, selected, repository, machine, location))
    }

    fn worktree_setup_snapshot(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
    ) -> Result<WorktreeSetupSnapshot, String> {
        let (workspace, selected, repository, machine, location) =
            self.worktree_inputs(workspace_id, repository_id, machine_id)?;
        self.ensure_worktree_not_recorded(workspace_id, repository_id)?;
        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == workspace.item_id)
            .cloned()
            .ok_or_else(|| format!("Item {} does not exist", workspace.item_id))?;
        Ok(WorktreeSetupSnapshot {
            workspace,
            item,
            selected,
            repository,
            machine,
            location,
        })
    }

    fn worktree_setup_snapshot_is_current(&self, snapshot: &WorktreeSetupSnapshot) -> bool {
        self.state
            .workspaces
            .iter()
            .any(|workspace| workspace == &snapshot.workspace)
            && self.state.items.iter().any(|item| item == &snapshot.item)
            && self
                .state
                .repositories
                .iter()
                .any(|repository| repository == &snapshot.repository)
            && self
                .state
                .machines
                .iter()
                .any(|machine| machine == &snapshot.machine)
            && self
                .state
                .repository_locations
                .iter()
                .any(|location| location == &snapshot.location)
    }

    fn ensure_worktree_not_recorded(
        &self,
        workspace_id: i64,
        repository_id: i64,
    ) -> Result<(), String> {
        if self.state.worktrees.iter().any(|worktree| {
            worktree.workspace_id == workspace_id && worktree.repository_id == repository_id
        }) {
            return Err(format!(
                "This Item already has a registered Worktree for Repository {repository_id}"
            ));
        }
        Ok(())
    }

    // These values are the complete persisted Worktree record; bundling them would obscure the
    // one-to-one mapping with Event::CreateWorktree without reducing call-site complexity.
    #[allow(clippy::too_many_arguments)]
    fn persist_prepared_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        branch: String,
        base_branch: String,
        is_dirty: bool,
    ) -> Result<Worktree, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorktree {
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty,
            },
        )
        .map_err(|error| error.to_string())?;
        let worktree = decision
            .state
            .worktrees
            .last()
            .cloned()
            .ok_or_else(|| "Worktree creation produced no Worktree".to_owned())?;
        self.commit(decision)?;
        Ok(worktree)
    }
}
