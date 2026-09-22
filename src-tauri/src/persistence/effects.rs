use super::audit::append_audit_actions;
use super::codecs::*;
use super::*;

impl SqliteStore {
    pub fn apply(&mut self, effects: &[Effect]) -> Result<(), StoreError> {
        self.apply_with_audit(effects, &[])
    }

    pub fn apply_with_audit(
        &mut self,
        effects: &[Effect],
        audit_actions: &[AuditAction],
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        for effect in effects {
            match effect {
                Effect::PersistContext {
                    context,
                    next_context_id,
                } => {
                    transaction.execute(
                        "INSERT INTO contexts
                            (id, name, grill_agent, grill_model, grill_effort)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            context.id,
                            context.name,
                            agent_kind_as_str(context.grill_defaults.agent),
                            context.grill_defaults.model,
                            context.grill_defaults.effort,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_context_id'",
                        params![next_context_id],
                    )?;
                }
                Effect::PersistContextGrillDefaults { context } => {
                    transaction.execute(
                        "UPDATE contexts
                         SET grill_agent = ?1, grill_model = ?2, grill_effort = ?3
                         WHERE id = ?4",
                        params![
                            agent_kind_as_str(context.grill_defaults.agent),
                            context.grill_defaults.model,
                            context.grill_defaults.effort,
                            context.id,
                        ],
                    )?;
                }
                Effect::PersistProject {
                    project,
                    next_project_id,
                } => {
                    transaction.execute(
                        "INSERT INTO projects
                            (id, context_id, name, default_item_status, default_execution_mode)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            project.id,
                            project.context_id,
                            project.name,
                            item_status_as_str(project.defaults.item_status),
                            execution_mode_as_str(project.defaults.execution_mode),
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_project_id'",
                        params![next_project_id],
                    )?;
                }
                Effect::PersistRepository {
                    repository,
                    next_repository_id,
                } => {
                    transaction.execute(
                        "INSERT INTO repositories (id, project_id, name, remote_url, base_branch)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            repository.id,
                            repository.project_id,
                            repository.name,
                            repository.remote_url,
                            repository.base_branch,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_repository_id'",
                        params![next_repository_id],
                    )?;
                }
                Effect::UpdateRepository { repository } => {
                    transaction.execute(
                        "UPDATE repositories
                         SET remote_url = ?1, base_branch = ?2
                         WHERE id = ?3",
                        params![repository.remote_url, repository.base_branch, repository.id],
                    )?;
                }
                Effect::PersistRepositoryLocation { location } => {
                    transaction.execute(
                        "INSERT INTO repository_locations
                            (repository_id, machine_id, checkout_path, worktree_root)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            location.repository_id,
                            location.machine_id,
                            location.checkout_path,
                            location.worktree_root,
                        ],
                    )?;
                }
                Effect::ResetLocalData {
                    context,
                    project,
                    next_context_id,
                    next_project_id,
                } => {
                    transaction.execute_batch(
                        "DELETE FROM audit_entries;
                         DELETE FROM link_attention_state;
                         DELETE FROM activities;
                         DELETE FROM external_snapshots;
                         DELETE FROM external_links;
                         DELETE FROM external_objects;
                         DELETE FROM context_attention_defaults;
                         DELETE FROM item_relationships;
                         DELETE FROM reminders;
                         DELETE FROM runs;
                         DELETE FROM worktrees;
                         DELETE FROM workspace_repositories;
                         DELETE FROM workspaces;
                         DELETE FROM repository_locations;
                         DELETE FROM items;
                         DELETE FROM repositories;
                         DELETE FROM machines;
                         DELETE FROM projects;
                         DELETE FROM contexts;",
                    )?;
                    transaction.execute(
                        "INSERT INTO contexts
                            (id, name, grill_agent, grill_model, grill_effort)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            context.id,
                            context.name,
                            agent_kind_as_str(context.grill_defaults.agent),
                            context.grill_defaults.model,
                            context.grill_defaults.effort,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO projects
                            (id, context_id, name, default_item_status)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            project.id,
                            project.context_id,
                            project.name,
                            item_status_as_str(project.defaults.item_status),
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_context_id'",
                        params![next_context_id],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_project_id'",
                        params![next_project_id],
                    )?;
                }
                Effect::PersistItem {
                    item,
                    next_item_number,
                    next_item_id,
                } => {
                    transaction.execute(
                        "INSERT INTO items
                            (id, human_identifier, title, project_id, status, notes)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            item.id,
                            item.human_identifier,
                            item.title,
                            item.project_id,
                            item_status_as_str(item.status),
                            item.notes,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_item_number'",
                        params![next_item_number],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_item_id'",
                        params![next_item_id],
                    )?;
                }
                Effect::PersistItemUpdate { item } => {
                    transaction.execute(
                        "UPDATE items
                         SET status = ?1, notes = ?2
                         WHERE id = ?3",
                        params![item_status_as_str(item.status), item.notes, item.id,],
                    )?;
                }
                Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                } => {
                    transaction.execute(
                        "UPDATE items
                         SET status = ?1, notes = ?2
                         WHERE id = ?3",
                        params![item_status_as_str(item.status), item.notes, item.id],
                    )?;
                    persist_item_reminders(&transaction, item)?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_reminder_id'",
                        params![next_reminder_id],
                    )?;
                }
                Effect::PersistWorkspace {
                    workspace,
                    next_workspace_id,
                } => {
                    transaction.execute(
                        "INSERT INTO workspaces (id, item_id, preparation_state)
                         VALUES (?1, ?2, ?3)",
                        params![
                            workspace.id,
                            workspace.item_id,
                            workspace_preparation_state_as_str(workspace.preparation_state),
                        ],
                    )?;
                    persist_workspace_repositories(&transaction, workspace)?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_workspace_id'",
                        params![next_workspace_id],
                    )?;
                }
                Effect::PersistWorkspaceUpdate { workspace } => {
                    transaction.execute(
                        "UPDATE workspaces SET preparation_state = ?1 WHERE id = ?2",
                        params![
                            workspace_preparation_state_as_str(workspace.preparation_state),
                            workspace.id,
                        ],
                    )?;
                }
                Effect::PersistWorktree {
                    worktree,
                    next_worktree_id,
                } => {
                    transaction.execute(
                        "INSERT INTO worktrees
                            (id, workspace_id, repository_id, machine_id, path, branch,
                             base_branch, is_dirty)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            worktree.id,
                            worktree.workspace_id,
                            worktree.repository_id,
                            worktree.machine_id,
                            worktree.path,
                            worktree.branch,
                            worktree.base_branch,
                            bool_as_i64(worktree.is_dirty),
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_worktree_id'",
                        params![next_worktree_id],
                    )?;
                }
                Effect::RemoveWorktree { worktree_id } => {
                    transaction
                        .execute("DELETE FROM worktrees WHERE id = ?1", params![worktree_id])?;
                }
                Effect::RemoveWorkspace { workspace_id } => {
                    transaction.execute(
                        "DELETE FROM worktrees WHERE workspace_id = ?1",
                        params![workspace_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspaces WHERE id = ?1",
                        params![workspace_id],
                    )?;
                }
                Effect::PersistMachine {
                    machine,
                    next_machine_id,
                } => {
                    let transport_json =
                        serde_json::to_string(&machine.transport).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })?;
                    transaction.execute(
                        "INSERT INTO machines
                            (id, context_id, name, socket_name, transport_json,
                             last_observed, last_observed_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            machine.id,
                            machine.context_id,
                            machine.name,
                            machine.socket_name,
                            transport_json,
                            machine_observation_as_str(machine.last_observed),
                            machine.last_observed_at,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_machine_id'",
                        params![next_machine_id],
                    )?;
                }
                Effect::PersistMachineObservation { machine } => {
                    transaction.execute(
                        "UPDATE machines
                         SET last_observed = ?1, last_observed_at = ?2
                         WHERE id = ?3",
                        params![
                            machine_observation_as_str(machine.last_observed),
                            machine.last_observed_at,
                            machine.id,
                        ],
                    )?;
                }
                Effect::PersistRun { run, next_run_id } => {
                    transaction.execute(
                        "INSERT INTO runs
                            (id, item_id, workspace_id, repository_id, worktree_id,
                            machine_id, agent,
                            execution_profile, model, effort, skill_snapshot,
                            prompt, working_directory, session_name, pane_id,
                            started_at, state, pane_status, direct_checkouts_json,
                            transcript, grill_question_group_json, grill_answers_json,
                            grill_decisions_json, grill_response, grill_phase, grill_action)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
                        params![
                            run.id,
                            run.item_id,
                            run.workspace_id,
                            run.repository_id,
                            run.worktree_id,
                            run.machine_id,
                            agent_kind_as_str(run.agent),
                            execution_profile_as_str(run.execution_profile),
                            run.model,
                            run.effort,
                            run.skill_snapshot,
                            run.prompt,
                            run.working_directory,
                            run.session_name,
                            run.pane_id,
                            run.started_at,
                            run_state_as_str(run.state),
                            run_pane_status_as_str(run.pane_status),
                            serde_json::to_string(&run.direct_checkouts).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            run.transcript,
                            run.grill_question_group
                                .as_ref()
                                .map(serde_json::to_string)
                                .transpose()
                                .map_err(|error| {
                                    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                                })?,
                            serde_json::to_string(&run.grill_answers).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            serde_json::to_string(&run.grill_decisions).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            run.grill_response,
                            run.grill_phase.map(grill_phase_as_str),
                            run.grill_action.map(grill_continuation_action_as_str),
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_run_id'",
                        params![next_run_id],
                    )?;
                }
                Effect::PersistRunState { run } => {
                    transaction.execute(
                        "UPDATE runs SET state = ?1, grill_phase = ?2, grill_action = ?3 WHERE id = ?4",
                        params![
                            run_state_as_str(run.state),
                            run.grill_phase.map(grill_phase_as_str),
                            run.grill_action.map(grill_continuation_action_as_str),
                            run.id
                        ],
                    )?;
                }
                Effect::PersistRunTranscript { run } => {
                    transaction.execute(
                        "UPDATE runs
                         SET transcript = ?1, grill_question_group_json = ?2,
                             grill_answers_json = ?3, grill_decisions_json = ?4,
                             grill_response = ?5
                         WHERE id = ?6",
                        params![
                            run.transcript,
                            run.grill_question_group
                                .as_ref()
                                .map(serde_json::to_string)
                                .transpose()
                                .map_err(|error| {
                                    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                                })?,
                            serde_json::to_string(&run.grill_answers).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            serde_json::to_string(&run.grill_decisions).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            run.grill_response,
                            run.id,
                        ],
                    )?;
                }
                Effect::PersistGrillAnswers { run } => {
                    transaction.execute(
                        "UPDATE runs
                         SET grill_answers_json = ?1, grill_decisions_json = ?2
                         WHERE id = ?3",
                        params![
                            serde_json::to_string(&run.grill_answers).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            serde_json::to_string(&run.grill_decisions).map_err(|error| {
                                rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                            })?,
                            run.id,
                        ],
                    )?;
                }
                Effect::PersistGrillResponse { run } => {
                    transaction.execute(
                        "UPDATE runs SET grill_response = ?1 WHERE id = ?2",
                        params![run.grill_response, run.id],
                    )?;
                }
                Effect::PersistRunPaneStatus { run } => {
                    transaction.execute(
                        "UPDATE runs SET pane_status = ?1, grill_phase = ?2 WHERE id = ?3",
                        params![
                            run_pane_status_as_str(run.pane_status),
                            run.grill_phase.map(grill_phase_as_str),
                            run.id
                        ],
                    )?;
                }
                Effect::RemoveRepository { repository_id } => {
                    transaction.execute(
                        "DELETE FROM repository_locations WHERE repository_id = ?1",
                        params![repository_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM worktrees
                         WHERE repository_id = ?1
                            OR workspace_id IN (
                                SELECT workspace_id FROM workspace_repositories
                                WHERE repository_id = ?1
                            )",
                        params![repository_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspaces
                         WHERE id IN (
                             SELECT workspace_id FROM workspace_repositories
                             WHERE repository_id = ?1
                         )",
                        params![repository_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspace_repositories
                         WHERE repository_id = ?1
                            OR workspace_id IN (
                                SELECT workspace_id FROM workspace_repositories
                                WHERE repository_id = ?1
                            )",
                        params![repository_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM repositories WHERE id = ?1",
                        params![repository_id],
                    )?;
                }
                Effect::RemoveMachine { machine_id } => {
                    transaction
                        .execute("DELETE FROM machines WHERE id = ?1", params![machine_id])?;
                }
                Effect::RemoveRun { run_id } => {
                    transaction.execute("DELETE FROM runs WHERE id = ?1", params![run_id])?;
                }
                Effect::RemoveLink { link_id, .. } => {
                    transaction
                        .execute("DELETE FROM external_links WHERE id = ?1", params![link_id])?;
                }
                Effect::RemoveExternalObject { external_object_id } => {
                    transaction.execute(
                        "DELETE FROM external_objects WHERE id = ?1",
                        params![external_object_id],
                    )?;
                }
                Effect::RemoveItemCascade {
                    item_id,
                    orphaned_external_object_ids,
                    ..
                } => {
                    transaction.execute("DELETE FROM runs WHERE item_id = ?1", params![item_id])?;
                    transaction.execute(
                        "DELETE FROM worktrees
                         WHERE workspace_id IN (SELECT id FROM workspaces WHERE item_id = ?1)",
                        params![item_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspace_repositories
                         WHERE workspace_id IN (SELECT id FROM workspaces WHERE item_id = ?1)",
                        params![item_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspaces WHERE item_id = ?1",
                        params![item_id],
                    )?;
                    transaction
                        .execute("DELETE FROM reminders WHERE item_id = ?1", params![item_id])?;
                    transaction.execute(
                        "DELETE FROM item_relationships
                         WHERE from_item_id = ?1 OR to_item_id = ?1",
                        params![item_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM external_links WHERE item_id = ?1",
                        params![item_id],
                    )?;
                    for external_object_id in orphaned_external_object_ids {
                        transaction.execute(
                            "DELETE FROM external_objects
                             WHERE id = ?1
                               AND NOT EXISTS (
                                   SELECT 1 FROM external_links
                                   WHERE external_object_id = external_objects.id
                               )",
                            params![external_object_id],
                        )?;
                    }
                    transaction.execute("DELETE FROM items WHERE id = ?1", params![item_id])?;
                }
                Effect::RemoveProjectCascade {
                    project_id,
                    orphaned_external_object_ids,
                    ..
                } => {
                    transaction.execute(
                        "DELETE FROM runs
                         WHERE item_id IN (
                             SELECT id FROM items WHERE project_id = ?1
                         )",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM worktrees
                         WHERE workspace_id IN (
                             SELECT workspaces.id FROM workspaces
                             JOIN items ON items.id = workspaces.item_id
                             WHERE items.project_id = ?1
                         )",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspace_repositories
                         WHERE workspace_id IN (
                             SELECT workspaces.id FROM workspaces
                             JOIN items ON items.id = workspaces.item_id
                             WHERE items.project_id = ?1
                         )",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspaces
                         WHERE item_id IN (SELECT id FROM items WHERE project_id = ?1)",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM reminders
                         WHERE item_id IN (
                             SELECT id FROM items WHERE project_id = ?1
                         )",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM item_relationships
                         WHERE from_item_id IN (
                             SELECT id FROM items WHERE project_id = ?1
                         )
                            OR to_item_id IN (
                             SELECT id FROM items WHERE project_id = ?1
                         )",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM external_links
                         WHERE item_id IN (
                             SELECT id FROM items WHERE project_id = ?1
                         )",
                        params![project_id],
                    )?;
                    for external_object_id in orphaned_external_object_ids {
                        transaction.execute(
                            "DELETE FROM external_objects
                             WHERE id = ?1
                               AND NOT EXISTS (
                                   SELECT 1 FROM external_links
                                   WHERE external_object_id = external_objects.id
                               )",
                            params![external_object_id],
                        )?;
                    }
                    transaction.execute(
                        "DELETE FROM items WHERE project_id = ?1",
                        params![project_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM repositories WHERE project_id = ?1",
                        params![project_id],
                    )?;
                    transaction
                        .execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
                }
                Effect::RemoveContextCascade {
                    context_id,
                    orphaned_external_object_ids,
                    ..
                } => {
                    transaction.execute(
                        "DELETE FROM runs
                         WHERE machine_id IN (
                             SELECT id FROM machines WHERE context_id = ?1
                         )
                            OR item_id IN (
                             SELECT items.id
                             FROM items
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM worktrees
                         WHERE workspace_id IN (
                             SELECT workspaces.id FROM workspaces
                             JOIN items ON items.id = workspaces.item_id
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspace_repositories
                         WHERE workspace_id IN (
                             SELECT workspaces.id FROM workspaces
                             JOIN items ON items.id = workspaces.item_id
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM workspaces
                         WHERE item_id IN (
                             SELECT items.id FROM items
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM reminders
                         WHERE item_id IN (
                             SELECT items.id
                             FROM items
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM item_relationships
                         WHERE from_item_id IN (
                             SELECT items.id
                             FROM items
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )
                            OR to_item_id IN (
                             SELECT items.id
                             FROM items
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM external_links
                         WHERE item_id IN (
                             SELECT items.id
                             FROM items
                             JOIN projects ON projects.id = items.project_id
                             WHERE projects.context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    for external_object_id in orphaned_external_object_ids {
                        transaction.execute(
                            "DELETE FROM external_objects
                             WHERE id = ?1
                               AND NOT EXISTS (
                                   SELECT 1 FROM external_links
                                   WHERE external_object_id = external_objects.id
                               )",
                            params![external_object_id],
                        )?;
                    }
                    transaction.execute(
                        "DELETE FROM items
                         WHERE project_id IN (
                             SELECT id FROM projects WHERE context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM repositories
                         WHERE project_id IN (
                             SELECT id FROM projects WHERE context_id = ?1
                         )",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM machines WHERE context_id = ?1",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM context_attention_defaults WHERE context_id = ?1",
                        params![context_id],
                    )?;
                    transaction.execute(
                        "DELETE FROM projects WHERE context_id = ?1",
                        params![context_id],
                    )?;
                    transaction
                        .execute("DELETE FROM contexts WHERE id = ?1", params![context_id])?;
                }
                Effect::PersistItemRelation { relation } => {
                    transaction.execute(
                        "INSERT INTO item_relationships (from_item_id, to_item_id, kind)
                         VALUES (?1, ?2, ?3)",
                        params![
                            relation.from_item_id,
                            relation.to_item_id,
                            item_relation_kind_as_str(relation.kind),
                        ],
                    )?;
                }
                Effect::PersistExternalObject {
                    object,
                    next_external_object_id,
                } => {
                    transaction.execute(
                        "INSERT INTO external_objects
                            (id, provider, kind, external_key, canonical_url)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            object.id,
                            external_provider_as_str(object.provider),
                            external_object_kind_as_str(object.kind),
                            object.external_key,
                            object.canonical_url,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_external_object_id'",
                        params![next_external_object_id],
                    )?;
                }
                Effect::PersistLink { link, next_link_id } => {
                    transaction.execute(
                        "INSERT INTO external_links (id, item_id, external_object_id)
                         VALUES (?1, ?2, ?3)",
                        params![link.id, link.item_id, link.external_object_id],
                    )?;
                    persist_link_state(&transaction, link)?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_link_id'",
                        params![next_link_id],
                    )?;
                }
                Effect::PersistLinkState { link } => {
                    persist_link_state(&transaction, link)?;
                }
                Effect::PersistExternalSnapshot { snapshot } => {
                    let metadata_json =
                        serde_json::to_string(&snapshot.metadata).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })?;
                    transaction.execute(
                        "INSERT INTO external_snapshots
                            (external_object_id, title, state, metadata_json, fetched_at)
                         VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(external_object_id) DO UPDATE SET
                            title = excluded.title,
                            state = excluded.state,
                            metadata_json = excluded.metadata_json,
                            fetched_at = excluded.fetched_at",
                        params![
                            snapshot.external_object_id,
                            snapshot.title,
                            snapshot.state,
                            metadata_json,
                            snapshot.fetched_at,
                        ],
                    )?;
                }
                Effect::PersistActivity {
                    activity,
                    next_activity_id,
                } => {
                    let changes_json =
                        serde_json::to_string(&activity.changes).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })?;
                    transaction.execute(
                        "INSERT INTO activities
                            (id, external_object_id, observed_at, changes_json)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            activity.id,
                            activity.external_object_id,
                            activity.observed_at,
                            changes_json,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_activity_id'",
                        params![next_activity_id],
                    )?;
                }
                Effect::PersistContextAttentionDefault { attention_default } => {
                    transaction.execute(
                        "INSERT INTO context_attention_defaults
                            (context_id, object_kind, title_attention, state_attention,
                             metadata_attention)
                         VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(context_id, object_kind) DO UPDATE SET
                            title_attention = excluded.title_attention,
                            state_attention = excluded.state_attention,
                            metadata_attention = excluded.metadata_attention",
                        params![
                            attention_default.context_id,
                            external_object_kind_as_str(attention_default.object_kind),
                            bool_as_i64(attention_default.policy.title),
                            bool_as_i64(attention_default.policy.state),
                            bool_as_i64(attention_default.policy.metadata),
                        ],
                    )?;
                }
            }
        }
        append_audit_actions(&transaction, audit_actions)?;
        transaction.commit()?;
        Ok(())
    }
}

fn persist_link_state(
    transaction: &rusqlite::Transaction<'_>,
    link: &Link,
) -> Result<(), rusqlite::Error> {
    let (title_attention, state_attention, metadata_attention) = link
        .attention_policy
        .map(|policy| {
            (
                Some(bool_as_i64(policy.title)),
                Some(bool_as_i64(policy.state)),
                Some(bool_as_i64(policy.metadata)),
            )
        })
        .unwrap_or((None, None, None));
    let provenance_json = link
        .provenance
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    transaction.execute(
        "INSERT INTO link_attention_state
            (link_id, reviewed_activity_id, title_attention, state_attention, metadata_attention,
             watch_until, review_at, provenance_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(link_id) DO UPDATE SET
            reviewed_activity_id = excluded.reviewed_activity_id,
            title_attention = excluded.title_attention,
            state_attention = excluded.state_attention,
            metadata_attention = excluded.metadata_attention,
            watch_until = excluded.watch_until,
            review_at = excluded.review_at,
            provenance_json = excluded.provenance_json",
        params![
            link.id,
            link.reviewed_activity_id,
            title_attention,
            state_attention,
            metadata_attention,
            link.watch_until,
            link.review_at,
            provenance_json,
        ],
    )?;
    Ok(())
}

fn persist_item_reminders(
    transaction: &rusqlite::Transaction<'_>,
    item: &Item,
) -> Result<(), rusqlite::Error> {
    transaction.execute("DELETE FROM reminders WHERE item_id = ?1", params![item.id])?;
    for reminder in &item.reminders {
        transaction.execute(
            "INSERT INTO reminders (id, item_id, remind_at) VALUES (?1, ?2, ?3)",
            params![reminder.id, item.id, reminder.remind_at],
        )?;
    }
    Ok(())
}

fn persist_workspace_repositories(
    transaction: &rusqlite::Transaction<'_>,
    workspace: &Workspace,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "DELETE FROM workspace_repositories WHERE workspace_id = ?1",
        params![workspace.id],
    )?;
    for repository in &workspace.repositories {
        transaction.execute(
            "INSERT INTO workspace_repositories
                (workspace_id, repository_id, branch, base_branch)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                workspace.id,
                repository.repository_id,
                repository.branch,
                repository.base_branch,
            ],
        )?;
    }
    Ok(())
}
