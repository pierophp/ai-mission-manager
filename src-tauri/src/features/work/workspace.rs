use super::*;

impl Runtime {
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

    pub(crate) fn prepare_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        reuse_existing_branch: bool,
        confirm_dirty_attachment: bool,
    ) -> Result<Worktree, String> {
        let (workspace, selected, repository, machine, location) =
            self.worktree_inputs(workspace_id, repository_id, machine_id)?;
        self.ensure_worktree_not_recorded(workspace_id, repository_id)?;

        let machine_home = machine_home_directory(&machine);
        let canonical_checkout = resolve_machine_path(&location.checkout_path, &machine_home);
        let worktree_root = resolve_machine_path(&location.worktree_root, &machine_home);
        let destination = worktree_path(
            &worktree_root,
            workspace.id,
            &selected.branch,
            &repository.name,
        );
        let inspection = match GitCli::system().prepare_worktree_on_machine(
            &machine,
            &repository,
            &canonical_checkout,
            &destination,
            &selected.branch,
            &selected.base_branch,
            reuse_existing_branch,
            confirm_dirty_attachment,
        ) {
            Ok(inspection) => inspection,
            Err(error) => {
                let mark_error = self.mark_workspace_resumable(workspace_id).err();
                return Err(format_commit_error(error.to_string(), mark_error));
            }
        };
        let path = normalize_machine_path(&destination.to_string_lossy(), &machine_home)
            .map_err(|error| error.to_string())?;
        self.persist_prepared_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            selected.branch,
            selected.base_branch,
            inspection.is_dirty,
        )
    }

    fn mark_workspace_resumable(&mut self, workspace_id: i64) -> Result<(), String> {
        let decision = decide(
            self.state.clone(),
            Event::MarkWorkspaceResumable { workspace_id },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)
    }

    pub(crate) fn attach_worktree(
        &mut self,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        confirm_dirty_attachment: bool,
    ) -> Result<Worktree, String> {
        let (_workspace, selected, repository, machine, location) =
            self.worktree_inputs(workspace_id, repository_id, machine_id)?;
        self.ensure_worktree_not_recorded(workspace_id, repository_id)?;

        let machine_home = machine_home_directory(&machine);
        let canonical_checkout = resolve_machine_path(&location.checkout_path, &machine_home);
        let path =
            normalize_machine_path(&path, &machine_home).map_err(|error| error.to_string())?;
        let worktree_path = resolve_machine_path(&path, &machine_home);
        let inspection = GitCli::system()
            .validate_worktree_attachment_on_machine(
                &machine,
                &repository,
                &canonical_checkout,
                &worktree_path,
                &selected.branch,
                confirm_dirty_attachment,
            )
            .map_err(|error| error.to_string())?;
        self.persist_prepared_worktree(
            workspace_id,
            repository_id,
            machine_id,
            path,
            selected.branch,
            selected.base_branch,
            inspection.is_dirty,
        )
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
            .ok_or_else(|| format!("Workspace {workspace_id} does not exist"))?;
        let selected = workspace
            .repositories
            .iter()
            .find(|repository| repository.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                format!("Repository {repository_id} is not selected in Workspace {workspace_id}")
            })?;
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))?;
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

    fn ensure_worktree_not_recorded(
        &self,
        workspace_id: i64,
        repository_id: i64,
    ) -> Result<(), String> {
        if self.state.worktrees.iter().any(|worktree| {
            worktree.workspace_id == workspace_id && worktree.repository_id == repository_id
        }) {
            return Err(format!(
                "Workspace {workspace_id} already has a recorded Worktree for Repository {repository_id}"
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
