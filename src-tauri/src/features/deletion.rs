//! Deletion planning and cleanup workflows.
//!
//! This module is the cohesive implementation seam for destructive local
//! operations. Feature modules keep their Tauri facades, while this module
//! owns the preview/confirmation protocol, active-Run blockers, physical
//! Worktree checks, and local-only cleanup before committing the reducer's
//! decision through the shared `Runtime`.

use std::time::Duration;

use crate::{
    app::{RunDeletionResult, Runtime},
    domain::{
        decide, plan_context_deletion, plan_external_object_deletion, plan_item_deletion,
        plan_machine_deletion, plan_project_deletion, plan_repository_deletion,
        plan_reset_local_data, run_is_active, run_uses_repository, Event,
        ExternalObjectDeletionPlan, ExternalObjectDeletionSummary, ItemDeletionPlan,
        ItemDeletionSummary, MachineDeletionPlan, ParentDeletionPlan, ParentDeletionSummary,
        RepositoryDeletionPlan, ResetLocalDataPlan, ResetLocalDataSummary,
    },
    git::GitCli,
    terminal::kill_pane_with_timeout,
};

use crate::features::structure::{machine_home_directory, resolve_machine_path};

pub const RESET_CONFIRMATION_PHRASE: &str = "RESET ALL LOCAL DATA";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionPreview {
    pub plan: ItemDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionResult {
    pub summary: ItemDeletionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeRemovalReport {
    pub worktree_id: i64,
    pub workspace_id: i64,
    pub repository_id: i64,
    pub repository_name: String,
    pub machine_id: i64,
    pub path: String,
    pub branch: String,
    pub is_dirty: bool,
    pub requires_destructive_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeRemovalResult {
    pub worktree_id: i64,
    pub branch_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectLinkDeletionPreview {
    pub link_id: i64,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionPreview {
    pub plan: ExternalObjectDeletionPlan,
    pub links: Vec<ExternalObjectLinkDeletionPreview>,
    pub provider_warning: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionResult {
    pub summary: ExternalObjectDeletionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLinkDeletionResult {
    pub link_id: i64,
    pub external_object_id: i64,
    pub external_object_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionPreview {
    pub plan: RepositoryDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionPreview {
    pub plan: MachineDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionPreview {
    pub plan: ParentDeletionPlan,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataPreview {
    pub plan: ResetLocalDataPlan,
    pub audit_entry_count: usize,
    pub blockers: Vec<String>,
    pub confirmation_phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataResult {
    pub summary: ResetLocalDataSummary,
    pub audit_entry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionResult {
    pub summary: ParentDeletionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionResult {
    pub machine_id: i64,
    pub run_count: usize,
    pub worktree_count: usize,
    pub repository_location_count: usize,
    pub stop_attempt_count: usize,
    pub stop_failure_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionResult {
    pub repository_id: i64,
    pub workspace_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentDeletionTarget {
    Project(i64),
    Context(i64),
}

impl ParentDeletionTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Project(_) => "Project",
            Self::Context(_) => "Context",
        }
    }

    fn matches(self, plan: &ParentDeletionPlan) -> bool {
        match self {
            Self::Project(project_id) => plan.project_id == Some(project_id),
            Self::Context(context_id) => plan.context_id == Some(context_id),
        }
    }
}

impl Runtime {
    fn build_external_object_deletion_preview(
        &self,
        external_object_id: i64,
    ) -> Result<ExternalObjectDeletionPreview, String> {
        let plan = plan_external_object_deletion(&self.state, external_object_id)
            .map_err(|error| error.to_string())?;
        let links = plan
            .link_ids
            .iter()
            .map(|link_id| {
                let link = self
                    .state
                    .links
                    .iter()
                    .find(|link| link.id == *link_id)
                    .ok_or_else(|| format!("Link {link_id} disappeared while building preview"))?;
                let item = self
                    .state
                    .items
                    .iter()
                    .find(|item| item.id == link.item_id)
                    .ok_or_else(|| {
                        format!("Item {} disappeared while building preview", link.item_id)
                    })?;
                Ok(ExternalObjectLinkDeletionPreview {
                    link_id: link.id,
                    item_id: item.id,
                    item_identifier: item.human_identifier.clone(),
                    item_title: item.title.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(ExternalObjectDeletionPreview {
            plan,
            links,
            provider_warning: "This only removes local Mission Manager data. GitHub Issues, pull requests, and other provider-owned objects are never deleted.".into(),
        })
    }

    pub(crate) fn prepare_external_object_deletion(
        &mut self,
        external_object_id: i64,
    ) -> Result<ExternalObjectDeletionPreview, String> {
        let preview = self.build_external_object_deletion_preview(external_object_id)?;
        self.pending_external_object_deletion = Some(preview.clone());
        Ok(preview)
    }

    pub(crate) fn delete_external_object(
        &mut self,
        external_object_id: i64,
        confirmed: bool,
    ) -> Result<ExternalObjectDeletionResult, String> {
        if !confirmed {
            return Err(
                "External Object deletion requires explicit confirmation after reviewing its local deletion preview".into(),
            );
        }
        let pending = self
            .pending_external_object_deletion
            .as_ref()
            .filter(|preview| preview.plan.external_object_id == external_object_id)
            .cloned()
            .ok_or_else(|| {
                "Review the External Object deletion preview before deleting it".to_owned()
            })?;
        let current = self.build_external_object_deletion_preview(external_object_id)?;
        if current != pending {
            return Err(
                "The External Object or one of its Links changed after the preview; review the updated local deletion preview before deleting it".into(),
            );
        }

        let summary = current.plan.summary();
        let decision = decide(
            self.state.clone(),
            Event::DeleteExternalObject { external_object_id },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_external_object_deletion = None;
        Ok(ExternalObjectDeletionResult { summary })
    }

    pub(crate) fn unlink_external_link(
        &mut self,
        link_id: i64,
        confirmed: bool,
    ) -> Result<ExternalLinkDeletionResult, String> {
        if !confirmed {
            return Err("Unlinking an Item requires explicit confirmation".into());
        }
        let external_object_id = self
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .map(|link| link.external_object_id)
            .ok_or_else(|| format!("Link {link_id} does not exist"))?;
        let decision = decide(self.state.clone(), Event::DeleteLink { link_id })
            .map_err(|error| error.to_string())?;
        let external_object_deleted = !decision
            .state
            .external_objects
            .iter()
            .any(|object| object.id == external_object_id);
        self.commit(decision)?;
        Ok(ExternalLinkDeletionResult {
            link_id,
            external_object_id,
            external_object_deleted,
        })
    }

    fn repository(&self, repository_id: i64) -> Result<crate::domain::Repository, String> {
        self.state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))
    }

    fn build_worktree_removal_report(
        &self,
        worktree_id: i64,
    ) -> Result<WorktreeRemovalReport, String> {
        let worktree = self
            .state
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .cloned()
            .ok_or_else(|| format!("Worktree {worktree_id} does not exist"))?;
        let repository = self.repository(worktree.repository_id)?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == worktree.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", worktree.machine_id))?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == worktree.repository_id
                    && location.machine_id == worktree.machine_id
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Repository {} has no checkout registered on Machine {}",
                    repository.name, machine.name
                )
            })?;
        let machine_home = machine_home_directory(&machine);
        let canonical_checkout = resolve_machine_path(&location.checkout_path, &machine_home);
        let path = resolve_machine_path(&worktree.path, &machine_home);
        let inspection = GitCli::system()
            .validate_worktree_attachment_on_machine(
                &machine,
                &repository,
                &canonical_checkout,
                &path,
                &worktree.branch,
                true,
            )
            .map_err(|error| error.to_string())?;
        Ok(WorktreeRemovalReport {
            worktree_id,
            workspace_id: worktree.workspace_id,
            repository_id: worktree.repository_id,
            repository_name: repository.name,
            machine_id: worktree.machine_id,
            path: worktree.path,
            branch: worktree.branch,
            is_dirty: inspection.is_dirty,
            requires_destructive_confirmation: inspection.is_dirty,
        })
    }

    pub(crate) fn prepare_worktree_removal(
        &mut self,
        worktree_id: i64,
    ) -> Result<WorktreeRemovalReport, String> {
        let report = self.build_worktree_removal_report(worktree_id)?;
        self.pending_worktree_removals
            .insert(worktree_id, report.clone());
        Ok(report)
    }

    pub(crate) fn remove_worktree(
        &mut self,
        worktree_id: i64,
        confirmed: bool,
        destructive_confirmed: bool,
    ) -> Result<WorktreeRemovalResult, String> {
        if !confirmed {
            return Err(
                "Worktree removal requires explicit confirmation after reviewing its safety report"
                    .into(),
            );
        }
        let pending = self
            .pending_worktree_removals
            .get(&worktree_id)
            .cloned()
            .ok_or_else(|| {
                "Review the Worktree removal safety report before removing it".to_owned()
            })?;
        let current = self.build_worktree_removal_report(worktree_id)?;
        if current != pending {
            return Err("The Worktree changed after the safety report; review the updated report before removing it".into());
        }
        if current.requires_destructive_confirmation && !destructive_confirmed {
            return Err("Removing a dirty Worktree requires destructive confirmation".into());
        }
        self.remove_worktree_physical(&current)?;
        let decision = decide(self.state.clone(), Event::RemoveWorktree { worktree_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_worktree_removals.remove(&worktree_id);
        Ok(WorktreeRemovalResult {
            worktree_id,
            branch_preserved: true,
        })
    }

    fn remove_worktree_physical(&self, report: &WorktreeRemovalReport) -> Result<(), String> {
        let worktree = self
            .state
            .worktrees
            .iter()
            .find(|worktree| worktree.id == report.worktree_id)
            .ok_or_else(|| format!("Worktree {} does not exist", report.worktree_id))?;
        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == worktree.machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {} does not exist", worktree.machine_id))?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == worktree.repository_id
                    && location.machine_id == worktree.machine_id
            })
            .cloned()
            .ok_or_else(|| "The Repository checkout location no longer exists".to_owned())?;
        let machine_home = machine_home_directory(&machine);
        GitCli::system()
            .remove_worktree_on_machine(
                &machine,
                &resolve_machine_path(&location.checkout_path, &machine_home),
                &resolve_machine_path(&worktree.path, &machine_home),
                report.requires_destructive_confirmation,
            )
            .map_err(|error| error.to_string())
    }

    fn build_item_deletion_preview(&self, item_id: i64) -> Result<ItemDeletionPreview, String> {
        let plan = plan_item_deletion(&self.state, item_id).map_err(|error| error.to_string())?;
        let blockers = plan
            .active_run_ids
            .iter()
            .map(|run_id| format!("Run #{run_id} is active; stop it before deleting this Item."))
            .collect::<Vec<_>>();
        Ok(ItemDeletionPreview { plan, blockers })
    }

    pub(crate) fn prepare_item_deletion(
        &mut self,
        item_id: i64,
    ) -> Result<ItemDeletionPreview, String> {
        let preview = self.build_item_deletion_preview(item_id)?;
        self.pending_item_deletion = Some(preview.clone());
        Ok(preview)
    }

    pub(crate) fn delete_item(
        &mut self,
        item_id: i64,
        confirmed: bool,
    ) -> Result<ItemDeletionResult, String> {
        if !confirmed {
            return Err(
                "Item deletion requires explicit confirmation after reviewing its deletion preview"
                    .into(),
            );
        }
        let pending = self
            .pending_item_deletion
            .as_ref()
            .filter(|preview| preview.plan.item_id == item_id)
            .cloned()
            .ok_or_else(|| "Review the Item deletion preview before deleting it".to_owned())?;
        let current = self.build_item_deletion_preview(item_id)?;
        if current != pending {
            return Err(
                "The Item changed after the preview; review the updated deletion preview".into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Item deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }
        let summary = current.plan.summary();
        let decision = decide(self.state.clone(), Event::DeleteItem { item_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_item_deletion = None;
        Ok(ItemDeletionResult { summary })
    }

    fn build_parent_deletion_preview(
        &self,
        target: ParentDeletionTarget,
    ) -> Result<ParentDeletionPreview, String> {
        let plan = match target {
            ParentDeletionTarget::Project(project_id) => {
                plan_project_deletion(&self.state, project_id)
            }
            ParentDeletionTarget::Context(context_id) => {
                plan_context_deletion(&self.state, context_id)
            }
        }
        .map_err(|error| error.to_string())?;
        let mut blockers = plan
            .active_run_ids
            .iter()
            .map(|run_id| {
                format!(
                    "Run #{run_id} is active; stop it before deleting this {}.",
                    target.label()
                )
            })
            .collect::<Vec<_>>();
        if matches!(target, ParentDeletionTarget::Context(_)) && self.state.contexts.len() == 1 {
            blockers.push(
                "This is the last Context; create another Context before deleting it.".into(),
            );
        }
        Ok(ParentDeletionPreview { plan, blockers })
    }

    pub(crate) fn prepare_project_deletion(
        &mut self,
        project_id: i64,
    ) -> Result<ParentDeletionPreview, String> {
        let preview =
            self.build_parent_deletion_preview(ParentDeletionTarget::Project(project_id))?;
        self.pending_parent_deletion = Some(preview.clone());
        Ok(preview)
    }

    pub(crate) fn prepare_context_deletion(
        &mut self,
        context_id: i64,
    ) -> Result<ParentDeletionPreview, String> {
        let preview =
            self.build_parent_deletion_preview(ParentDeletionTarget::Context(context_id))?;
        self.pending_parent_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn build_reset_local_data_preview(&self) -> Result<ResetLocalDataPreview, String> {
        let plan = plan_reset_local_data(&self.state);
        let blockers = self
            .state
            .runs
            .iter()
            .filter(|run| run_is_active(run))
            .map(|run| {
                format!(
                    "Run #{} is active; stop it before resetting local data.",
                    run.id
                )
            })
            .collect::<Vec<_>>();
        Ok(ResetLocalDataPreview {
            plan,
            audit_entry_count: self
                .store
                .audit_entry_count()
                .map_err(|error| error.to_string())?,
            blockers,
            confirmation_phrase: RESET_CONFIRMATION_PHRASE.into(),
        })
    }

    pub(crate) fn prepare_reset_local_data(&mut self) -> Result<ResetLocalDataPreview, String> {
        let preview = self.build_reset_local_data_preview()?;
        self.pending_reset_local_data = Some(preview.clone());
        Ok(preview)
    }

    pub(crate) fn reset_all_local_data(
        &mut self,
        confirmation: String,
    ) -> Result<ResetLocalDataResult, String> {
        if confirmation != RESET_CONFIRMATION_PHRASE {
            return Err(format!(
                "Reset requires the exact confirmation phrase: {RESET_CONFIRMATION_PHRASE}"
            ));
        }
        let pending = self
            .pending_reset_local_data
            .as_ref()
            .cloned()
            .ok_or_else(|| "Review the reset preview before resetting local data".to_owned())?;
        let current = self.build_reset_local_data_preview()?;
        if current != pending {
            return Err(
                "The local model changed after the reset preview; review the updated preview before resetting local data".into(),
            );
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Reset is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }

        let decision =
            decide(self.state.clone(), Event::ResetLocalData).map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_reset_local_data = None;
        self.pending_item_deletion = None;
        self.pending_external_object_deletion = None;
        self.pending_repository_deletion = None;
        self.pending_machine_deletion = None;
        self.pending_parent_deletion = None;
        self.terminal_connections.clear();

        Ok(ResetLocalDataResult {
            summary: current.plan.summary,
            audit_entry_count: current.audit_entry_count,
        })
    }

    fn delete_parent(
        &mut self,
        target: ParentDeletionTarget,
        event: Event,
        confirmed: bool,
    ) -> Result<ParentDeletionResult, String> {
        if !confirmed {
            return Err(format!(
                "{} deletion requires explicit confirmation after reviewing its deletion preview",
                target.label()
            ));
        }
        let pending = self
            .pending_parent_deletion
            .as_ref()
            .filter(|preview| target.matches(&preview.plan))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Review the {} deletion preview before deleting it",
                    target.label()
                )
            })?;
        let current = self.build_parent_deletion_preview(target)?;
        if current != pending {
            return Err(format!(
                "The {} changed after the preview; review the updated deletion preview",
                target.label()
            ));
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "{} deletion is blocked:\n{}",
                target.label(),
                current.blockers.join("\n")
            ));
        }
        let summary = current.plan.summary();
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_parent_deletion = None;
        Ok(ParentDeletionResult { summary })
    }

    pub(crate) fn delete_project(
        &mut self,
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<ParentDeletionResult, String> {
        self.delete_parent(
            ParentDeletionTarget::Project(project_id),
            Event::DeleteProject {
                project_id,
                item_ids,
                repository_ids,
                workspace_ids,
            },
            confirmed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn delete_context(
        &mut self,
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        machine_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<ParentDeletionResult, String> {
        self.delete_parent(
            ParentDeletionTarget::Context(context_id),
            Event::DeleteContext {
                context_id,
                project_ids,
                item_ids,
                repository_ids,
                workspace_ids,
                machine_ids,
            },
            confirmed,
        )
    }

    pub(crate) fn prepare_repository_deletion(
        &mut self,
        repository_id: i64,
    ) -> Result<RepositoryDeletionPreview, String> {
        let preview = self.build_repository_deletion_preview(repository_id)?;
        self.pending_repository_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn build_machine_deletion_preview(
        &self,
        machine_id: i64,
    ) -> Result<MachineDeletionPreview, String> {
        let plan =
            plan_machine_deletion(&self.state, machine_id).map_err(|error| error.to_string())?;
        Ok(MachineDeletionPreview {
            plan,
            blockers: Vec::new(),
        })
    }

    pub(crate) fn prepare_machine_deletion(
        &mut self,
        machine_id: i64,
    ) -> Result<MachineDeletionPreview, String> {
        let preview = self.build_machine_deletion_preview(machine_id)?;
        self.pending_machine_deletion = Some(preview.clone());
        Ok(preview)
    }

    fn build_repository_deletion_preview(
        &self,
        repository_id: i64,
    ) -> Result<RepositoryDeletionPreview, String> {
        let plan = plan_repository_deletion(&self.state, repository_id)
            .map_err(|error| error.to_string())?;
        let blockers = self
            .state
            .runs
            .iter()
            .filter(|run| run_uses_repository(&self.state, run, repository_id))
            .map(|run| {
                format!(
                    "Run #{} on Item #{} uses this Repository and must be deleted first.",
                    run.id, run.item_id
                )
            })
            .collect();
        Ok(RepositoryDeletionPreview { plan, blockers })
    }

    pub(crate) fn delete_repository(
        &mut self,
        repository_id: i64,
        _workspace_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<RepositoryDeletionResult, String> {
        if !confirmed {
            return Err("Repository deletion requires explicit confirmation after reviewing its deletion preview".into());
        }
        let pending = self
            .pending_repository_deletion
            .as_ref()
            .filter(|preview| preview.plan.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                "Review the Repository deletion preview before deleting it".to_owned()
            })?;
        let current = self.build_repository_deletion_preview(repository_id)?;
        if current != pending {
            return Err("The Repository or its Item execution references changed after the preview; review the updated deletion preview".into());
        }
        if !current.blockers.is_empty() {
            return Err(format!(
                "Repository deletion is blocked:\n{}",
                current.blockers.join("\n")
            ));
        }
        let workspace_count = current.plan.workspaces.len();
        let decision = decide(
            self.state.clone(),
            Event::DeleteRepository {
                repository_id,
                workspace_ids: current
                    .plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect(),
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_repository_deletion = None;
        Ok(RepositoryDeletionResult {
            repository_id,
            workspace_count,
        })
    }

    pub(crate) fn delete_machine(
        &mut self,
        machine_id: i64,
        run_ids: Vec<i64>,
        worktree_ids: Vec<i64>,
        repository_location_repository_ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<MachineDeletionResult, String> {
        if !confirmed {
            return Err(
                "Machine deletion requires explicit confirmation after reviewing its deletion preview".into(),
            );
        }
        let pending = self
            .pending_machine_deletion
            .as_ref()
            .filter(|preview| preview.plan.machine_id == machine_id)
            .cloned()
            .ok_or_else(|| "Review the Machine deletion preview before deleting it".to_owned())?;
        let current = self.build_machine_deletion_preview(machine_id)?;
        if current != pending {
            return Err(
                "The Machine or its associated records changed after the preview; review the updated deletion preview before deleting it".into(),
            );
        }
        let mut expected_run_ids = current
            .plan
            .runs
            .iter()
            .map(|run| run.id)
            .collect::<Vec<_>>();
        let mut provided_run_ids = run_ids.clone();
        let mut expected_worktree_ids = current.plan.worktree_ids.clone();
        let mut provided_worktree_ids = worktree_ids.clone();
        let mut expected_repository_location_ids =
            current.plan.repository_location_repository_ids.clone();
        let mut provided_repository_location_ids = repository_location_repository_ids.clone();
        expected_run_ids.sort_unstable();
        provided_run_ids.sort_unstable();
        expected_worktree_ids.sort_unstable();
        provided_worktree_ids.sort_unstable();
        expected_repository_location_ids.sort_unstable();
        provided_repository_location_ids.sort_unstable();
        if expected_run_ids != provided_run_ids
            || expected_worktree_ids != provided_worktree_ids
            || expected_repository_location_ids != provided_repository_location_ids
        {
            return Err(
                "The reviewed Machine deletion contents do not match the confirmation; review the preview again".into(),
            );
        }

        let machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or_else(|| format!("Machine {machine_id} does not exist"))?;
        let mut stop_attempt_count = 0;
        let mut stop_failure_count = 0;
        for run in self.state.runs.iter().filter(|run| {
            run.machine_id == machine_id && run.pane_status != crate::domain::RunPaneStatus::Missing
        }) {
            stop_attempt_count += 1;
            if kill_pane_with_timeout(&machine, &run.pane_id, Duration::from_secs(5)).is_err() {
                stop_failure_count += 1;
            }
        }

        let run_count = current.plan.runs.len();
        let worktree_count = current.plan.worktree_ids.len();
        let repository_location_count = current.plan.repository_location_repository_ids.len();
        let decision = decide(
            self.state.clone(),
            Event::DeleteMachine {
                machine_id,
                run_ids,
                worktree_ids,
                repository_location_repository_ids,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.pending_machine_deletion = None;

        Ok(MachineDeletionResult {
            machine_id,
            run_count,
            worktree_count,
            repository_location_count,
            stop_attempt_count,
            stop_failure_count,
        })
    }

    pub(crate) fn delete_run(
        &mut self,
        run_id: i64,
        confirmed: bool,
    ) -> Result<RunDeletionResult, String> {
        if !confirmed {
            return Err("Run deletion requires explicit confirmation".into());
        }
        let decision = decide(self.state.clone(), Event::DeleteRun { run_id })
            .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        Ok(RunDeletionResult { run_id })
    }
}
