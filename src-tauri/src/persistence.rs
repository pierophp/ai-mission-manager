use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use crate::domain::{
    Activity, AgentKind, AuditAction, AuditEntry, Context, ContextAttentionDefault, DomainState,
    Effect, ExecutionMode, ExecutionProfile, ExternalChangePolicy, ExternalMetadata,
    ExternalObject, ExternalObjectKind, ExternalProvider, ExternalSnapshot, GrillConfiguration,
    Item, ItemRelation, ItemRelationKind, ItemStatus, Link, Machine, MachineObservation, Project,
    ProjectDefaults, Reminder, Repository, RepositoryLocation, Run, RunPaneStatus, RunState,
    Workspace, WorkspacePreparationState, WorkspaceRepository, Worktree,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid Item status in database: {0}")]
    InvalidItemStatus(String),
    #[error("invalid Project default status in database: {0}")]
    InvalidProjectDefaultStatus(String),
    #[error("invalid Item relationship kind in database: {0}")]
    InvalidItemRelationKind(String),
    #[error("invalid External Object provider in database: {0}")]
    InvalidExternalProvider(String),
    #[error("invalid External Object kind in database: {0}")]
    InvalidExternalObjectKind(String),
    #[error("invalid External Object metadata in database: {0}")]
    InvalidExternalMetadata(String),
    #[error("invalid Activity changes in database: {0}")]
    InvalidActivityChanges(String),
    #[error("invalid Agent kind in database: {0}")]
    InvalidAgentKind(String),
    #[error("invalid Grill configuration in database: {0}")]
    InvalidGrillConfiguration(String),
    #[error("invalid Execution Profile in database: {0}")]
    InvalidExecutionProfile(String),
    #[error("invalid Execution Mode in database: {0}")]
    InvalidExecutionMode(String),
    #[error("invalid Run state in database: {0}")]
    InvalidRunState(String),
    #[error("invalid Run Pane status in database: {0}")]
    InvalidRunPaneStatus(String),
    #[error("invalid Workspace preparation state in database: {0}")]
    InvalidWorkspacePreparationState(String),
    #[error("invalid Machine transport in database: {0}")]
    InvalidMachineTransport(String),
    #[error("invalid Machine observation in database: {0}")]
    InvalidMachineObservation(String),
    #[error("invalid audit action in database: {0}")]
    InvalidAuditAction(String),
    #[error("invalid {key} value in database: {value}")]
    InvalidSequence { key: String, value: String },
    #[error("a database sequence is exhausted")]
    SequenceExhausted,
    #[error("database schema is incompatible; remove the database and start again")]
    IncompatibleSchema,
}

pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let mut connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        let item_columns = table_columns(&connection, "items")?;
        if !item_columns.is_empty()
            && (!item_columns.iter().any(|column| column == "project_id")
                || !item_columns.iter().any(|column| column == "notes"))
        {
            return Err(StoreError::IncompatibleSchema);
        }
        let link_attention_columns = table_columns(&connection, "link_attention_state")?;
        if !link_attention_columns.is_empty()
            && (!link_attention_columns
                .iter()
                .any(|column| column == "watch_until")
                || !link_attention_columns
                    .iter()
                    .any(|column| column == "review_at"))
        {
            return Err(StoreError::IncompatibleSchema);
        }
        let machine_columns = table_columns(&connection, "machines")?;
        if !machine_columns.is_empty()
            && !machine_columns
                .iter()
                .any(|column| column == "transport_json")
        {
            connection.execute(
                "ALTER TABLE machines ADD COLUMN transport_json TEXT NOT NULL DEFAULT '{\"kind\":\"local\"}'",
                [],
            )?;
        }
        if !machine_columns.is_empty()
            && !machine_columns
                .iter()
                .any(|column| column == "last_observed")
        {
            connection.execute(
                "ALTER TABLE machines ADD COLUMN last_observed TEXT NOT NULL DEFAULT 'unknown'",
                [],
            )?;
        }
        if !machine_columns.is_empty()
            && !machine_columns
                .iter()
                .any(|column| column == "last_observed_at")
        {
            connection.execute(
                "ALTER TABLE machines ADD COLUMN last_observed_at INTEGER",
                [],
            )?;
        }
        migrate_legacy_workset_data(&mut connection)?;
        migrate_runs_for_grill(&mut connection)?;
        initialize_schema(&mut connection)?;

        Ok(Self { connection })
    }

    pub fn load_state(&self) -> Result<DomainState, StoreError> {
        let next_context_id = self.sequence("next_context_id")?;
        let next_project_id = self.sequence("next_project_id")?;
        let next_item_id = self.sequence("next_item_id")?;
        let next_item_number = self.sequence("next_item_number")?;
        let next_repository_id = self.sequence("next_repository_id")?;
        let next_workspace_id = self.sequence("next_workspace_id")?;
        let next_worktree_id = self.sequence("next_worktree_id")?;
        let next_machine_id = self.sequence("next_machine_id")?;
        let next_run_id = self.sequence("next_run_id")?;
        let next_external_object_id = self.sequence("next_external_object_id")?;
        let next_link_id = self.sequence("next_link_id")?;
        let next_activity_id = self.sequence("next_activity_id")?;
        let next_reminder_id = self.sequence("next_reminder_id")?;
        let contexts = {
            let mut statement = self.connection.prepare(
                "SELECT id, name, grill_agent, grill_model, grill_effort
                     FROM contexts ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let agent: String = row.get(2)?;
                let model: String = row.get(3)?;
                let effort: String = row.get(4)?;
                Ok(Context {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    grill_defaults: GrillConfiguration {
                        agent: parse_agent_kind(&agent).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        model,
                        effort,
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let projects = {
            let mut statement = self.connection.prepare(
                "SELECT id, context_id, name, default_item_status, default_execution_mode
                 FROM projects
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let status: String = row.get(3)?;
                let execution_mode: String = row.get(4)?;
                Ok(Project {
                    id: row.get(0)?,
                    context_id: row.get(1)?,
                    name: row.get(2)?,
                    defaults: ProjectDefaults {
                        item_status: parse_project_default_status(&status).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        execution_mode: parse_execution_mode(&execution_mode).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let repositories = {
            let mut statement = self.connection.prepare(
                "SELECT id, project_id, name, remote_url, base_branch
                 FROM repositories
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(Repository {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    remote_url: row.get(3)?,
                    base_branch: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let repository_locations = {
            let mut statement = self.connection.prepare(
                "SELECT repository_id, machine_id, checkout_path, worktree_root
                 FROM repository_locations
                 ORDER BY repository_id, machine_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(RepositoryLocation {
                    repository_id: row.get(0)?,
                    machine_id: row.get(1)?,
                    checkout_path: row.get(2)?,
                    worktree_root: row.get(3)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let machines = {
            let mut statement = self.connection.prepare(
                "SELECT id, context_id, name, socket_name, transport_json, last_observed,
                        last_observed_at
                 FROM machines
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let transport: String = row.get(4)?;
                let observation: String = row.get(5)?;
                Ok(Machine {
                    id: row.get(0)?,
                    context_id: row.get(1)?,
                    name: row.get(2)?,
                    socket_name: row.get(3)?,
                    transport: serde_json::from_str(&transport).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(StoreError::InvalidMachineTransport(error.to_string())),
                        )
                    })?,
                    last_observed: parse_machine_observation(&observation).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    last_observed_at: row.get(6)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut items = {
            let mut statement = self.connection.prepare(
                "SELECT id, human_identifier, title, project_id, status, notes
                 FROM items
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let status: String = row.get(4)?;
                Ok(Item {
                    id: row.get(0)?,
                    human_identifier: row.get(1)?,
                    title: row.get(2)?,
                    project_id: row.get(3)?,
                    status: parse_item_status(&status).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    notes: row.get(5)?,
                    reminders: Vec::new(),
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let reminders = {
            let mut statement = self.connection.prepare(
                "SELECT id, item_id, remind_at
                 FROM reminders
                 ORDER BY item_id, id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(1)?,
                    Reminder {
                        id: row.get(0)?,
                        remind_at: row.get(2)?,
                    },
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (item_id, reminder) in reminders {
            if let Some(item) = items.iter_mut().find(|item| item.id == item_id) {
                item.reminders.push(reminder);
            }
        }
        let mut workspaces = {
            let mut statement = self.connection.prepare(
                "SELECT id, item_id, preparation_state
                 FROM workspaces
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let preparation_state: String = row.get(2)?;
                Ok(Workspace {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    repositories: Vec::new(),
                    preparation_state: parse_workspace_preparation_state(&preparation_state)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let workspace_repositories = {
            let mut statement = self.connection.prepare(
                "SELECT workspace_id, repository_id, branch, base_branch
                 FROM workspace_repositories
                 ORDER BY workspace_id, repository_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    WorkspaceRepository {
                        repository_id: row.get(1)?,
                        branch: row.get(2)?,
                        base_branch: row.get(3)?,
                    },
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (workspace_id, repository) in workspace_repositories {
            if let Some(workspace) = workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
            {
                workspace.repositories.push(repository);
            }
        }
        let worktrees = {
            let mut statement = self.connection.prepare(
                "SELECT id, workspace_id, repository_id, machine_id, path, branch,
                        base_branch, is_dirty
                 FROM worktrees
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(Worktree {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    repository_id: row.get(2)?,
                    machine_id: row.get(3)?,
                    path: row.get(4)?,
                    branch: row.get(5)?,
                    base_branch: row.get(6)?,
                    is_dirty: row.get::<_, i64>(7)? != 0,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let runs = {
            let mut statement = self.connection.prepare(
                "SELECT id, item_id, workspace_id, repository_id, worktree_id,
                        machine_id, agent, execution_profile, model, effort, skill_snapshot,
                        prompt, working_directory, session_name, pane_id, started_at, state,
                        pane_status, direct_checkouts_json
                 FROM runs
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let agent: String = row.get(6)?;
                let execution_profile: String = row.get(7)?;
                let direct_checkouts_json: String = row.get(18)?;
                Ok(Run {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    workspace_id: row.get(2)?,
                    repository_id: row.get(3)?,
                    worktree_id: row.get(4)?,
                    machine_id: row.get(5)?,
                    agent: parse_agent_kind(&agent).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    execution_profile: parse_execution_profile(&execution_profile).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    model: row.get(8)?,
                    effort: row.get(9)?,
                    skill_snapshot: row.get(10)?,
                    prompt: row.get(11)?,
                    working_directory: row.get(12)?,
                    session_name: row.get(13)?,
                    pane_id: row.get(14)?,
                    started_at: row.get(15)?,
                    state: parse_run_state(&row.get::<_, String>(16)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            16,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    pane_status: parse_run_pane_status(&row.get::<_, String>(17)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                17,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    direct_checkouts: serde_json::from_str(&direct_checkouts_json).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                18,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let relationships = {
            let mut statement = self.connection.prepare(
                "SELECT from_item_id, to_item_id, kind
                 FROM item_relationships
                 ORDER BY from_item_id, to_item_id, kind",
            )?;
            let rows = statement.query_map([], |row| {
                let kind: String = row.get(2)?;
                Ok(ItemRelation {
                    from_item_id: row.get(0)?,
                    to_item_id: row.get(1)?,
                    kind: parse_item_relation_kind(&kind).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let external_objects = {
            let mut statement = self.connection.prepare(
                "SELECT id, provider, kind, external_key, canonical_url
                 FROM external_objects
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let provider: String = row.get(1)?;
                let kind: String = row.get(2)?;
                Ok(ExternalObject {
                    id: row.get(0)?,
                    provider: parse_external_provider(&provider).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    kind: parse_external_object_kind(&kind).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    external_key: row.get(3)?,
                    canonical_url: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let links = {
            let mut statement = self.connection.prepare(
                "SELECT external_links.id, external_links.item_id, external_links.external_object_id,
                        COALESCE(link_attention_state.reviewed_activity_id, 0),
                        link_attention_state.title_attention,
                        link_attention_state.state_attention,
                        link_attention_state.metadata_attention,
                        link_attention_state.watch_until,
                        link_attention_state.review_at
                 FROM external_links
                 LEFT JOIN link_attention_state
                   ON link_attention_state.link_id = external_links.id
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let title_attention: Option<i64> = row.get(4)?;
                let state_attention: Option<i64> = row.get(5)?;
                let metadata_attention: Option<i64> = row.get(6)?;
                let attention_policy = match (title_attention, state_attention, metadata_attention)
                {
                    (Some(title), Some(state), Some(metadata)) => Some(ExternalChangePolicy {
                        title: title != 0,
                        state: state != 0,
                        metadata: metadata != 0,
                    }),
                    _ => None,
                };
                Ok(Link {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    external_object_id: row.get(2)?,
                    reviewed_activity_id: row.get(3)?,
                    attention_policy,
                    watch_until: row.get(7)?,
                    review_at: row.get(8)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let snapshots = {
            let mut statement = self.connection.prepare(
                "SELECT external_object_id, title, state, metadata_json, fetched_at
                 FROM external_snapshots
                 ORDER BY external_object_id",
            )?;
            let rows = statement.query_map([], |row| {
                let metadata_json: String = row.get(3)?;
                Ok(ExternalSnapshot {
                    external_object_id: row.get(0)?,
                    title: row.get(1)?,
                    state: row.get(2)?,
                    metadata: serde_json::from_str::<Vec<ExternalMetadata>>(&metadata_json)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    fetched_at: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let activities = {
            let mut statement = self.connection.prepare(
                "SELECT id, external_object_id, observed_at, changes_json
                 FROM activities
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let changes_json: String = row.get(3)?;
                Ok(Activity {
                    id: row.get(0)?,
                    external_object_id: row.get(1)?,
                    observed_at: row.get(2)?,
                    changes: serde_json::from_str(&changes_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let attention_defaults = {
            let mut statement = self.connection.prepare(
                "SELECT context_id, object_kind, title_attention, state_attention,
                        metadata_attention
                 FROM context_attention_defaults
                 ORDER BY context_id, object_kind",
            )?;
            let rows = statement.query_map([], |row| {
                let object_kind: String = row.get(1)?;
                Ok(ContextAttentionDefault {
                    context_id: row.get(0)?,
                    object_kind: parse_external_object_kind(&object_kind).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    policy: ExternalChangePolicy {
                        title: row.get::<_, i64>(2)? != 0,
                        state: row.get::<_, i64>(3)? != 0,
                        metadata: row.get::<_, i64>(4)? != 0,
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        Ok(DomainState {
            next_context_id,
            next_project_id,
            next_item_id,
            next_item_number,
            next_repository_id,
            next_workspace_id,
            next_worktree_id,
            next_machine_id,
            next_run_id,
            next_external_object_id,
            next_link_id,
            next_activity_id,
            next_reminder_id,
            contexts,
            projects,
            repositories,
            repository_locations,
            items,
            workspaces,
            worktrees,
            machines,
            runs,
            relationships,
            external_objects,
            links,
            snapshots,
            activities,
            attention_defaults,
        })
    }

    pub fn list_audit_history(&self) -> Result<Vec<AuditEntry>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, action_json
             FROM audit_entries
             ORDER BY id DESC
             LIMIT 200",
        )?;
        let rows = statement.query_map([], |row| {
            let action_json: String = row.get(2)?;
            Ok(AuditEntry {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                action: serde_json::from_str(&action_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(StoreError::InvalidAuditAction(error.to_string())),
                    )
                })?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn audit_entry_count(&self) -> Result<usize, StoreError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM audit_entries", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as usize)
            .map_err(StoreError::from)
    }

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
                            started_at, state, pane_status, direct_checkouts_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_run_id'",
                        params![next_run_id],
                    )?;
                }
                Effect::PersistRunState { run } => {
                    transaction.execute(
                        "UPDATE runs SET state = ?1 WHERE id = ?2",
                        params![run_state_as_str(run.state), run.id],
                    )?;
                }
                Effect::PersistRunPaneStatus { run } => {
                    transaction.execute(
                        "UPDATE runs SET pane_status = ?1 WHERE id = ?2",
                        params![run_pane_status_as_str(run.pane_status), run.id],
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
        if !audit_actions.is_empty() {
            let mut next_audit_id: i64 = transaction.query_row(
                "SELECT value FROM metadata WHERE key = 'next_audit_id'",
                [],
                |row| row.get(0),
            )?;
            for action in audit_actions {
                let action_json = serde_json::to_string(action)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                transaction.execute(
                    "INSERT INTO audit_entries (id, recorded_at, action_json)
                     VALUES (?1, strftime('%s', 'now'), ?2)",
                    params![next_audit_id, action_json],
                )?;
                next_audit_id = next_audit_id
                    .checked_add(1)
                    .ok_or(StoreError::SequenceExhausted)?;
            }
            transaction.execute(
                "UPDATE metadata SET value = ?1 WHERE key = 'next_audit_id'",
                params![next_audit_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn sequence(&self, key: &str) -> Result<i64, StoreError> {
        let value: i64 = self.connection.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )?;
        if value < 1 {
            return Err(StoreError::InvalidSequence {
                key: key.into(),
                value: value.to_string(),
            });
        }
        Ok(value)
    }

    pub fn gh_executable_path(&self) -> Result<Option<PathBuf>, StoreError> {
        self.executable_path("gh_executable_path")
    }

    pub fn set_gh_executable_path(&mut self, path: &Path) -> Result<(), StoreError> {
        self.set_executable_path("gh_executable_path", path)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn executable_path(&self, key: &str) -> Result<Option<PathBuf>, StoreError> {
        self.setting(key).map(|path| path.map(PathBuf::from))
    }

    pub fn set_executable_path(&mut self, key: &str, path: &Path) -> Result<(), StoreError> {
        self.set_setting(key, &path.to_string_lossy())
    }
}

fn initialize_schema(connection: &mut Connection) -> Result<(), StoreError> {
    let projects_table_existed = !table_columns(connection, "projects")?.is_empty();
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS metadata (
             key TEXT PRIMARY KEY NOT NULL,
             value INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS settings (
             key TEXT PRIMARY KEY NOT NULL,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS contexts (
             id INTEGER PRIMARY KEY NOT NULL,
             name TEXT NOT NULL UNIQUE,
             grill_agent TEXT NOT NULL DEFAULT 'claude',
             grill_model TEXT NOT NULL DEFAULT 'claude-sonnet-4-5',
             grill_effort TEXT NOT NULL DEFAULT 'high'
         );
         CREATE TABLE IF NOT EXISTS projects (
             id INTEGER PRIMARY KEY NOT NULL,
             context_id INTEGER NOT NULL REFERENCES contexts(id),
             name TEXT NOT NULL,
             default_item_status TEXT NOT NULL
                 CHECK (default_item_status IN ('Inbox', 'Active', 'Waiting', 'Done')),
             default_execution_mode TEXT NOT NULL DEFAULT 'worktree'
                 CHECK (default_execution_mode IN ('direct', 'worktree')),
             UNIQUE (context_id, name)
         );
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_context_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_project_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_number', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_repository_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_workspace_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_worktree_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_machine_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_run_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_external_object_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_link_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_activity_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_reminder_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_audit_id', 1);
         INSERT OR IGNORE INTO contexts (id, name) VALUES (1, 'Personal');",
    )?;

    if !projects_table_existed {
        ensure_default_projects(connection)?;
    }

    let context_columns = table_columns(connection, "contexts")?;
    if !context_columns.is_empty() && !context_columns.iter().any(|column| column == "grill_agent")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN grill_agent TEXT NOT NULL DEFAULT 'claude'",
            [],
        )?;
    }
    if !context_columns.is_empty() && !context_columns.iter().any(|column| column == "grill_model")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN grill_model TEXT NOT NULL DEFAULT 'claude-sonnet-4-5'",
            [],
        )?;
    }
    if !context_columns.is_empty()
        && !context_columns
            .iter()
            .any(|column| column == "grill_effort")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN grill_effort TEXT NOT NULL DEFAULT 'high'",
            [],
        )?;
    }

    if table_columns(connection, "items")?.is_empty() {
        create_items_table(connection)?;
    }

    let project_columns = table_columns(connection, "projects")?;
    if !project_columns.is_empty()
        && !project_columns
            .iter()
            .any(|column| column == "default_execution_mode")
    {
        connection.execute(
            "ALTER TABLE projects ADD COLUMN default_execution_mode TEXT NOT NULL DEFAULT 'worktree'",
            [],
        )?;
    }

    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS projects_by_context
             ON projects (context_id);
         CREATE INDEX IF NOT EXISTS items_by_project_and_status
             ON items (project_id, status);
         CREATE TABLE IF NOT EXISTS repositories (
             id INTEGER PRIMARY KEY NOT NULL,
             project_id INTEGER NOT NULL REFERENCES projects(id),
             name TEXT NOT NULL,
             remote_url TEXT NOT NULL,
             base_branch TEXT NOT NULL DEFAULT 'main',
             UNIQUE (project_id, name)
         );
         CREATE INDEX IF NOT EXISTS repositories_by_project
             ON repositories (project_id, id);
         CREATE TABLE IF NOT EXISTS machines (
             id INTEGER PRIMARY KEY NOT NULL,
             context_id INTEGER NOT NULL REFERENCES contexts(id),
             name TEXT NOT NULL,
             socket_name TEXT NOT NULL,
             transport_json TEXT NOT NULL DEFAULT '{\"kind\":\"local\"}',
             last_observed TEXT NOT NULL DEFAULT 'unknown',
             last_observed_at INTEGER,
             UNIQUE (context_id, name)
         );
         CREATE INDEX IF NOT EXISTS machines_by_context
             ON machines (context_id, id);
         CREATE TABLE IF NOT EXISTS repository_locations (
             repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
             machine_id INTEGER NOT NULL REFERENCES machines(id) ON DELETE CASCADE,
             checkout_path TEXT NOT NULL,
             worktree_root TEXT NOT NULL,
             PRIMARY KEY (repository_id, machine_id)
         );
         CREATE INDEX IF NOT EXISTS repository_locations_by_machine
             ON repository_locations (machine_id, repository_id);
         CREATE TABLE IF NOT EXISTS workspaces (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id),
             preparation_state TEXT NOT NULL DEFAULT 'pending'
         );
         CREATE INDEX IF NOT EXISTS workspaces_by_item
             ON workspaces (item_id, id);
         CREATE TABLE IF NOT EXISTS workspace_repositories (
             workspace_id INTEGER NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
             repository_id INTEGER NOT NULL REFERENCES repositories(id),
             branch TEXT NOT NULL,
             base_branch TEXT NOT NULL,
             PRIMARY KEY (workspace_id, repository_id)
         );
         CREATE INDEX IF NOT EXISTS workspace_repositories_by_repository
             ON workspace_repositories (repository_id);
         CREATE TABLE IF NOT EXISTS worktrees (
             id INTEGER PRIMARY KEY NOT NULL,
             workspace_id INTEGER NOT NULL REFERENCES workspaces(id),
             repository_id INTEGER NOT NULL REFERENCES repositories(id),
             machine_id INTEGER NOT NULL REFERENCES machines(id),
             path TEXT NOT NULL,
             branch TEXT NOT NULL,
             base_branch TEXT NOT NULL,
             is_dirty INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS worktrees_by_workspace
             ON worktrees (workspace_id, id);
         CREATE INDEX IF NOT EXISTS worktrees_by_repository
             ON worktrees (repository_id, id);
         CREATE TABLE IF NOT EXISTS runs (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id),
             workspace_id INTEGER REFERENCES workspaces(id),
             repository_id INTEGER REFERENCES repositories(id),
             worktree_id INTEGER REFERENCES worktrees(id),
             machine_id INTEGER NOT NULL REFERENCES machines(id),
             agent TEXT NOT NULL CHECK (agent IN ('claude', 'codex')),
             execution_profile TEXT NOT NULL
                 CHECK (execution_profile IN ('investigate', 'implement', 'review', 'custom', 'grill')),
             model TEXT,
             effort TEXT,
             skill_snapshot TEXT,
             prompt TEXT NOT NULL,
             working_directory TEXT NOT NULL,
             session_name TEXT NOT NULL,
             pane_id TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             state TEXT NOT NULL DEFAULT 'unknown',
             pane_status TEXT NOT NULL DEFAULT 'unknown',
             direct_checkouts_json TEXT NOT NULL DEFAULT '[]'
         );
         CREATE INDEX IF NOT EXISTS runs_by_item
             ON runs (item_id, id);
         CREATE INDEX IF NOT EXISTS runs_by_workspace
             ON runs (workspace_id, id);
         CREATE TABLE IF NOT EXISTS reminders (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             remind_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS reminders_by_item
             ON reminders (item_id, id);
         CREATE TABLE IF NOT EXISTS item_relationships (
             from_item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             to_item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             kind TEXT NOT NULL CHECK (kind IN ('blocks', 'blocked_by', 'related_to')),
             PRIMARY KEY (from_item_id, to_item_id, kind)
         );
         CREATE INDEX IF NOT EXISTS relationships_by_target
             ON item_relationships (to_item_id);
         CREATE TABLE IF NOT EXISTS external_objects (
             id INTEGER PRIMARY KEY NOT NULL,
             provider TEXT NOT NULL CHECK (provider IN ('github', 'generic')),
             kind TEXT NOT NULL CHECK (kind IN ('issue', 'pull_request', 'generic')),
             external_key TEXT NOT NULL,
             canonical_url TEXT NOT NULL,
             UNIQUE (provider, external_key)
         );
         CREATE TABLE IF NOT EXISTS external_links (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             external_object_id INTEGER NOT NULL REFERENCES external_objects(id) ON DELETE CASCADE,
             UNIQUE (item_id, external_object_id)
         );
         CREATE TABLE IF NOT EXISTS external_snapshots (
             external_object_id INTEGER PRIMARY KEY NOT NULL REFERENCES external_objects(id) ON DELETE CASCADE,
             title TEXT NOT NULL,
             state TEXT NOT NULL,
             metadata_json TEXT NOT NULL,
             fetched_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS link_attention_state (
             link_id INTEGER PRIMARY KEY NOT NULL REFERENCES external_links(id) ON DELETE CASCADE,
             reviewed_activity_id INTEGER NOT NULL DEFAULT 0,
             title_attention INTEGER,
             state_attention INTEGER,
             metadata_attention INTEGER,
             watch_until TEXT,
             review_at TEXT
         );
         CREATE TABLE IF NOT EXISTS activities (
             id INTEGER PRIMARY KEY NOT NULL,
             external_object_id INTEGER NOT NULL REFERENCES external_objects(id) ON DELETE CASCADE,
             observed_at INTEGER NOT NULL,
             changes_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS context_attention_defaults (
             context_id INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
             object_kind TEXT NOT NULL CHECK (object_kind IN ('issue', 'pull_request', 'generic')),
             title_attention INTEGER NOT NULL,
             state_attention INTEGER NOT NULL,
             metadata_attention INTEGER NOT NULL,
             PRIMARY KEY (context_id, object_kind)
         );
         CREATE INDEX IF NOT EXISTS external_links_by_item
             ON external_links (item_id);
         CREATE INDEX IF NOT EXISTS external_links_by_object
             ON external_links (external_object_id);
         CREATE INDEX IF NOT EXISTS activities_by_object
             ON activities (external_object_id, id);
         CREATE TABLE IF NOT EXISTS audit_entries (
             id INTEGER PRIMARY KEY NOT NULL,
             recorded_at INTEGER NOT NULL,
             action_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS audit_entries_by_recorded_at
             ON audit_entries (recorded_at, id);",
    )?;

    let repository_columns = table_columns(connection, "repositories")?;
    if !repository_columns.is_empty()
        && !repository_columns
            .iter()
            .any(|column| column == "base_branch")
    {
        connection.execute(
            "ALTER TABLE repositories ADD COLUMN base_branch TEXT NOT NULL DEFAULT 'main'",
            [],
        )?;
    }
    let workspace_columns = table_columns(connection, "workspaces")?;
    if !workspace_columns.is_empty()
        && !workspace_columns
            .iter()
            .any(|column| column == "preparation_state")
    {
        connection.execute(
            "ALTER TABLE workspaces ADD COLUMN preparation_state TEXT NOT NULL DEFAULT 'pending'",
            [],
        )?;
    }
    let run_columns = table_columns(connection, "runs")?;
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "state") {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN state TEXT NOT NULL DEFAULT 'unknown'",
            [],
        )?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "pane_status") {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN pane_status TEXT NOT NULL DEFAULT 'unknown'",
            [],
        )?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "direct_checkouts_json")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN direct_checkouts_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "repository_id") {
        connection.execute("ALTER TABLE runs ADD COLUMN repository_id INTEGER", [])?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "worktree_id") {
        connection.execute("ALTER TABLE runs ADD COLUMN worktree_id INTEGER", [])?;
    }
    ensure_sequence_at_least(connection, "next_context_id", "contexts", "id")?;
    ensure_sequence_at_least(connection, "next_project_id", "projects", "id")?;
    ensure_sequence_at_least(connection, "next_item_id", "items", "id")?;
    ensure_sequence_at_least(connection, "next_repository_id", "repositories", "id")?;
    ensure_sequence_at_least(connection, "next_workspace_id", "workspaces", "id")?;
    ensure_sequence_at_least(connection, "next_worktree_id", "worktrees", "id")?;
    ensure_sequence_at_least(connection, "next_machine_id", "machines", "id")?;
    ensure_sequence_at_least(connection, "next_run_id", "runs", "id")?;
    ensure_sequence_at_least(
        connection,
        "next_external_object_id",
        "external_objects",
        "id",
    )?;
    ensure_sequence_at_least(connection, "next_link_id", "external_links", "id")?;
    ensure_sequence_at_least(connection, "next_activity_id", "activities", "id")?;
    ensure_sequence_at_least(connection, "next_reminder_id", "reminders", "id")?;
    ensure_sequence_at_least(connection, "next_audit_id", "audit_entries", "id")?;

    Ok(())
}

/// Remove data owned by the legacy Workset model exactly once.
///
/// This deliberately only changes SQLite state. Any old Workset directories are
/// left for explicit human cleanup, and the transaction keeps all database
/// records intact if one cleanup step fails.
fn migrate_legacy_workset_data(connection: &mut Connection) -> Result<(), StoreError> {
    if table_columns(connection, "metadata")?.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    let worksets_exist = !table_columns(&transaction, "worksets")?.is_empty();
    let workset_repositories_exist =
        !table_columns(&transaction, "workset_repositories")?.is_empty();
    let run_columns = table_columns(&transaction, "runs")?;
    let runs_have_legacy_link = run_columns.iter().any(|column| column == "workset_id");
    let completed: Option<i64> = transaction
        .query_row(
            "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if completed == Some(1)
        && !worksets_exist
        && !workset_repositories_exist
        && !runs_have_legacy_link
    {
        return Ok(());
    }

    if runs_have_legacy_link {
        transaction.execute("DELETE FROM runs WHERE workset_id IS NOT NULL", [])?;
    }
    if worksets_exist {
        transaction.execute(
            "DELETE FROM activities
             WHERE external_object_id IN (
                 SELECT DISTINCT external_links.external_object_id
                 FROM external_links
                 JOIN worksets ON worksets.item_id = external_links.item_id
             )",
            [],
        )?;
    }
    transaction.execute(
        "DELETE FROM audit_entries WHERE LOWER(action_json) LIKE '%workset%'",
        [],
    )?;
    if workset_repositories_exist {
        transaction.execute("DELETE FROM workset_repositories", [])?;
    }
    if worksets_exist {
        transaction.execute("DELETE FROM worksets", [])?;
    }
    if runs_have_legacy_link {
        let direct_checkouts_select = if run_columns
            .iter()
            .any(|column| column == "direct_checkouts_json")
        {
            "direct_checkouts_json"
        } else {
            "'[]'"
        };
        transaction.execute_batch(&format!(
            "ALTER TABLE runs RENAME TO runs_legacy;
             CREATE TABLE runs (
                 id INTEGER PRIMARY KEY NOT NULL,
                 item_id INTEGER NOT NULL REFERENCES items(id),
                 workspace_id INTEGER REFERENCES workspaces(id),
                 repository_id INTEGER REFERENCES repositories(id),
                 worktree_id INTEGER REFERENCES worktrees(id),
                 machine_id INTEGER NOT NULL REFERENCES machines(id),
                 agent TEXT NOT NULL CHECK (agent IN ('claude', 'codex')),
                 execution_profile TEXT NOT NULL
                     CHECK (execution_profile IN ('investigate', 'implement', 'review', 'custom')),
                 prompt TEXT NOT NULL,
                 working_directory TEXT NOT NULL,
                 session_name TEXT NOT NULL,
                 pane_id TEXT NOT NULL,
                 started_at INTEGER NOT NULL,
                 state TEXT NOT NULL DEFAULT 'unknown',
                 pane_status TEXT NOT NULL DEFAULT 'unknown',
                 direct_checkouts_json TEXT NOT NULL DEFAULT '[]'
             );
             INSERT INTO runs (
                 id, item_id, workspace_id, repository_id, worktree_id, machine_id, agent,
                 execution_profile, prompt, working_directory, session_name, pane_id, started_at,
                 state, pane_status, direct_checkouts_json
             )
             SELECT id, item_id, NULL, NULL, NULL, machine_id, agent, execution_profile,
                    prompt, working_directory, session_name, pane_id, started_at,
                    state, pane_status, {direct_checkouts_select}
             FROM runs_legacy;
             DROP TABLE runs_legacy;"
        ))?;
    }
    if workset_repositories_exist {
        transaction.execute_batch("DROP INDEX IF EXISTS workset_repositories_by_repository; DROP TABLE workset_repositories;")?;
    }
    if worksets_exist {
        transaction.execute_batch("DROP INDEX IF EXISTS worksets_by_item; DROP TABLE worksets;")?;
    }
    transaction.execute(
        "INSERT INTO metadata (key, value) VALUES ('workset_migration_completed', 1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_runs_for_grill(connection: &mut Connection) -> Result<(), StoreError> {
    let columns = table_columns(connection, "runs")?;
    if columns.is_empty() || columns.iter().any(|column| column == "model") {
        return Ok(());
    }

    connection.execute_batch(
        "DROP INDEX IF EXISTS runs_by_item;
         DROP INDEX IF EXISTS runs_by_workspace;
         ALTER TABLE runs RENAME TO runs_legacy;
         CREATE TABLE runs (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id),
             workspace_id INTEGER REFERENCES workspaces(id),
             repository_id INTEGER REFERENCES repositories(id),
             worktree_id INTEGER REFERENCES worktrees(id),
             machine_id INTEGER NOT NULL REFERENCES machines(id),
             agent TEXT NOT NULL CHECK (agent IN ('claude', 'codex')),
             execution_profile TEXT NOT NULL
                 CHECK (execution_profile IN ('investigate', 'implement', 'review', 'custom', 'grill')),
             model TEXT,
             effort TEXT,
             skill_snapshot TEXT,
             prompt TEXT NOT NULL,
             working_directory TEXT NOT NULL,
             session_name TEXT NOT NULL,
             pane_id TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             state TEXT NOT NULL DEFAULT 'unknown',
             pane_status TEXT NOT NULL DEFAULT 'unknown',
             direct_checkouts_json TEXT NOT NULL DEFAULT '[]'
         );
         INSERT INTO runs (
             id, item_id, workspace_id, repository_id, worktree_id, machine_id, agent,
             execution_profile, prompt, working_directory, session_name, pane_id, started_at,
             state, pane_status, direct_checkouts_json
         )
         SELECT id, item_id, workspace_id, repository_id, worktree_id, machine_id, agent,
                execution_profile, prompt, working_directory, session_name, pane_id, started_at,
                state, pane_status, direct_checkouts_json
         FROM runs_legacy;
         DROP TABLE runs_legacy;
         CREATE INDEX runs_by_item ON runs (item_id, id);
         CREATE INDEX runs_by_workspace ON runs (workspace_id, id);",
    )?;
    Ok(())
}

fn ensure_default_projects(connection: &mut Connection) -> Result<(), StoreError> {
    let context_ids = {
        let mut statement = connection.prepare("SELECT id FROM contexts ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for context_id in context_ids {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM projects WHERE context_id = ?1 AND name = 'Default'
             )",
            params![context_id],
            |row| row.get(0),
        )?;
        if exists {
            continue;
        }

        let project_id: i64 = connection.query_row(
            "SELECT value FROM metadata WHERE key = 'next_project_id'",
            [],
            |row| row.get(0),
        )?;
        let next_project_id = project_id
            .checked_add(1)
            .ok_or(StoreError::SequenceExhausted)?;
        connection.execute(
            "INSERT INTO projects (id, context_id, name, default_item_status)
             VALUES (?1, ?2, 'Default', 'Inbox')",
            params![project_id, context_id],
        )?;
        connection.execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'next_project_id'",
            params![next_project_id],
        )?;
    }

    Ok(())
}

fn create_items_table(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE items (
             id INTEGER PRIMARY KEY NOT NULL,
             human_identifier TEXT NOT NULL UNIQUE,
             title TEXT NOT NULL,
             project_id INTEGER NOT NULL REFERENCES projects(id),
             status TEXT NOT NULL CHECK (status IN ('Inbox', 'Active', 'Waiting', 'Done')),
             notes TEXT NOT NULL DEFAULT ''
         );",
    )?;
    Ok(())
}

fn table_columns(connection: &Connection, table: &str) -> Result<Vec<String>, StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn ensure_sequence_at_least(
    connection: &Connection,
    key: &str,
    table: &str,
    column: &str,
) -> Result<(), StoreError> {
    let minimum: i64 = connection.query_row(
        &format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}"),
        [],
        |row| row.get(0),
    )?;
    connection.execute(
        "UPDATE metadata SET value = CASE WHEN value < ?1 THEN ?1 ELSE value END
         WHERE key = ?2",
        params![minimum, key],
    )?;
    Ok(())
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
    transaction.execute(
        "INSERT INTO link_attention_state
            (link_id, reviewed_activity_id, title_attention, state_attention, metadata_attention,
             watch_until, review_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(link_id) DO UPDATE SET
            reviewed_activity_id = excluded.reviewed_activity_id,
            title_attention = excluded.title_attention,
            state_attention = excluded.state_attention,
            metadata_attention = excluded.metadata_attention,
            watch_until = excluded.watch_until,
            review_at = excluded.review_at",
        params![
            link.id,
            link.reviewed_activity_id,
            title_attention,
            state_attention,
            metadata_attention,
            link.watch_until,
            link.review_at,
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

fn bool_as_i64(value: bool) -> i64 {
    i64::from(value)
}

fn agent_kind_as_str(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "claude",
        AgentKind::Codex => "codex",
    }
}

fn parse_agent_kind(agent: &str) -> Result<AgentKind, StoreError> {
    match agent {
        "claude" => Ok(AgentKind::Claude),
        "codex" => Ok(AgentKind::Codex),
        other => Err(StoreError::InvalidAgentKind(other.into())),
    }
}

fn execution_profile_as_str(profile: ExecutionProfile) -> &'static str {
    match profile {
        ExecutionProfile::Investigate => "investigate",
        ExecutionProfile::Implement => "implement",
        ExecutionProfile::Review => "review",
        ExecutionProfile::CustomPrompt => "custom",
        ExecutionProfile::Grill => "grill",
    }
}

fn parse_execution_profile(profile: &str) -> Result<ExecutionProfile, StoreError> {
    match profile {
        "investigate" => Ok(ExecutionProfile::Investigate),
        "implement" => Ok(ExecutionProfile::Implement),
        "review" => Ok(ExecutionProfile::Review),
        "custom" => Ok(ExecutionProfile::CustomPrompt),
        "grill" => Ok(ExecutionProfile::Grill),
        other => Err(StoreError::InvalidExecutionProfile(other.into())),
    }
}

fn execution_mode_as_str(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Direct => "direct",
        ExecutionMode::Worktree => "worktree",
    }
}

fn parse_execution_mode(mode: &str) -> Result<ExecutionMode, StoreError> {
    match mode {
        "direct" => Ok(ExecutionMode::Direct),
        "worktree" => Ok(ExecutionMode::Worktree),
        other => Err(StoreError::InvalidExecutionMode(other.into())),
    }
}

fn workspace_preparation_state_as_str(state: WorkspacePreparationState) -> &'static str {
    match state {
        WorkspacePreparationState::Pending => "pending",
        WorkspacePreparationState::Resumable => "resumable",
        WorkspacePreparationState::Ready => "ready",
    }
}

fn parse_workspace_preparation_state(state: &str) -> Result<WorkspacePreparationState, StoreError> {
    match state {
        "pending" => Ok(WorkspacePreparationState::Pending),
        "resumable" => Ok(WorkspacePreparationState::Resumable),
        "ready" => Ok(WorkspacePreparationState::Ready),
        other => Err(StoreError::InvalidWorkspacePreparationState(other.into())),
    }
}

fn run_state_as_str(state: RunState) -> &'static str {
    match state {
        RunState::Unknown => "unknown",
        RunState::Working => "working",
        RunState::Blocked => "blocked",
        RunState::Finished => "finished",
    }
}

fn parse_run_state(state: &str) -> Result<RunState, StoreError> {
    match state {
        "unknown" => Ok(RunState::Unknown),
        "working" => Ok(RunState::Working),
        "blocked" => Ok(RunState::Blocked),
        "finished" => Ok(RunState::Finished),
        other => Err(StoreError::InvalidRunState(other.into())),
    }
}

fn run_pane_status_as_str(status: RunPaneStatus) -> &'static str {
    match status {
        RunPaneStatus::Unknown => "unknown",
        RunPaneStatus::Available => "available",
        RunPaneStatus::Missing => "missing",
    }
}

fn parse_run_pane_status(status: &str) -> Result<RunPaneStatus, StoreError> {
    match status {
        "unknown" => Ok(RunPaneStatus::Unknown),
        "available" => Ok(RunPaneStatus::Available),
        "missing" => Ok(RunPaneStatus::Missing),
        other => Err(StoreError::InvalidRunPaneStatus(other.into())),
    }
}

fn machine_observation_as_str(observation: MachineObservation) -> &'static str {
    match observation {
        MachineObservation::Unknown => "unknown",
        MachineObservation::Available => "available",
        MachineObservation::Offline => "offline",
    }
}

fn parse_machine_observation(observation: &str) -> Result<MachineObservation, StoreError> {
    match observation {
        "unknown" => Ok(MachineObservation::Unknown),
        "available" => Ok(MachineObservation::Available),
        "offline" => Ok(MachineObservation::Offline),
        other => Err(StoreError::InvalidMachineObservation(other.into())),
    }
}

fn item_status_as_str(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Inbox => "Inbox",
        ItemStatus::Active => "Active",
        ItemStatus::Waiting => "Waiting",
        ItemStatus::Done => "Done",
    }
}

fn parse_item_status(status: &str) -> Result<ItemStatus, StoreError> {
    parse_status(status).map_err(|other| StoreError::InvalidItemStatus(other.into()))
}

fn parse_project_default_status(status: &str) -> Result<ItemStatus, StoreError> {
    parse_status(status).map_err(|other| StoreError::InvalidProjectDefaultStatus(other.into()))
}

fn item_relation_kind_as_str(kind: ItemRelationKind) -> &'static str {
    match kind {
        ItemRelationKind::Blocks => "blocks",
        ItemRelationKind::BlockedBy => "blocked_by",
        ItemRelationKind::RelatedTo => "related_to",
    }
}

fn parse_item_relation_kind(kind: &str) -> Result<ItemRelationKind, StoreError> {
    match kind {
        "blocks" => Ok(ItemRelationKind::Blocks),
        "blocked_by" => Ok(ItemRelationKind::BlockedBy),
        "related_to" => Ok(ItemRelationKind::RelatedTo),
        other => Err(StoreError::InvalidItemRelationKind(other.into())),
    }
}

fn external_provider_as_str(provider: ExternalProvider) -> &'static str {
    match provider {
        ExternalProvider::GitHub => "github",
        ExternalProvider::Generic => "generic",
    }
}

fn parse_external_provider(provider: &str) -> Result<ExternalProvider, StoreError> {
    match provider {
        "github" => Ok(ExternalProvider::GitHub),
        "generic" => Ok(ExternalProvider::Generic),
        other => Err(StoreError::InvalidExternalProvider(other.into())),
    }
}

fn external_object_kind_as_str(kind: ExternalObjectKind) -> &'static str {
    match kind {
        ExternalObjectKind::Issue => "issue",
        ExternalObjectKind::PullRequest => "pull_request",
        ExternalObjectKind::Generic => "generic",
    }
}

fn parse_external_object_kind(kind: &str) -> Result<ExternalObjectKind, StoreError> {
    match kind {
        "issue" => Ok(ExternalObjectKind::Issue),
        "pull_request" => Ok(ExternalObjectKind::PullRequest),
        "generic" => Ok(ExternalObjectKind::Generic),
        other => Err(StoreError::InvalidExternalObjectKind(other.into())),
    }
}

fn parse_status(status: &str) -> Result<ItemStatus, &str> {
    match status {
        "Inbox" => Ok(ItemStatus::Inbox),
        "Active" => Ok(ItemStatus::Active),
        "Waiting" => Ok(ItemStatus::Waiting),
        "Done" => Ok(ItemStatus::Done),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;
    use tempfile::tempdir;

    use crate::domain::{
        compose_grill_prompt, decide, AgentKind, Event, GrillConfiguration, MachineTransport,
        RunCheckout, WorkspaceRepositoryInput, GRILL_SKILL_SNAPSHOT,
    };

    use super::{migrate_legacy_workset_data, table_columns, SqliteStore};

    fn apply_event(
        store: &mut SqliteStore,
        state: crate::domain::DomainState,
        event: Event,
    ) -> crate::domain::DomainState {
        let decision = decide(state, event).expect("event should be accepted");
        store
            .apply(&decision.effects)
            .expect("event effects should persist");
        decision.state
    }

    fn create_legacy_schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE metadata (
                     key TEXT PRIMARY KEY NOT NULL,
                     value INTEGER NOT NULL
                 );
                 INSERT INTO metadata (key, value)
                 VALUES ('workset_migration_completed', 0);
                 CREATE TABLE worksets (
                     id INTEGER PRIMARY KEY NOT NULL,
                     item_id INTEGER NOT NULL,
                     root_directory TEXT NOT NULL,
                     branch TEXT NOT NULL,
                     archived INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE workset_repositories (
                     workset_id INTEGER NOT NULL,
                     repository_id INTEGER NOT NULL,
                     branch_override TEXT,
                     base_branch_override TEXT,
                     current_branch TEXT NOT NULL,
                     is_dirty INTEGER NOT NULL DEFAULT 0,
                     PRIMARY KEY (workset_id, repository_id)
                 );
                 CREATE TABLE external_links (
                     item_id INTEGER NOT NULL,
                     external_object_id INTEGER NOT NULL
                 );
                 CREATE TABLE activities (
                     id INTEGER PRIMARY KEY NOT NULL,
                     external_object_id INTEGER NOT NULL,
                     observed_at INTEGER NOT NULL,
                     changes_json TEXT NOT NULL
                 );
                 CREATE TABLE audit_entries (
                     id INTEGER PRIMARY KEY NOT NULL,
                     recorded_at INTEGER NOT NULL,
                     action_json TEXT NOT NULL
                 );
                 CREATE TABLE items (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE workspaces (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE repositories (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE machines (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE worktrees (id INTEGER PRIMARY KEY NOT NULL);
                 INSERT INTO items (id) VALUES (1);
                 INSERT INTO machines (id) VALUES (1);
                 CREATE TABLE runs (
                     id INTEGER PRIMARY KEY NOT NULL,
                     item_id INTEGER NOT NULL,
                     workset_id INTEGER,
                     workspace_id INTEGER,
                     machine_id INTEGER NOT NULL,
                     agent TEXT NOT NULL,
                     execution_profile TEXT NOT NULL,
                     prompt TEXT NOT NULL,
                     working_directory TEXT NOT NULL,
                     session_name TEXT NOT NULL,
                     pane_id TEXT NOT NULL,
                     started_at INTEGER NOT NULL,
                     state TEXT NOT NULL DEFAULT 'unknown',
                     pane_status TEXT NOT NULL DEFAULT 'unknown'
                 );
                 CREATE INDEX runs_by_workset ON runs (workset_id, id);",
            )
            .expect("legacy schema should be created");
    }

    #[test]
    fn migrates_actual_legacy_runs_schema_without_direct_checkouts_column() {
        let directory = tempdir().expect("temporary directory should exist");
        let legacy_workset_directory = directory.path().join("legacy-workset");
        fs::create_dir(&legacy_workset_directory).expect("legacy directory should exist");
        fs::write(legacy_workset_directory.join("keep.txt"), "keep")
            .expect("legacy directory marker should be written");

        let mut connection = Connection::open_in_memory().expect("database should open");
        create_legacy_schema(&connection);
        let legacy_workset_directory_path = legacy_workset_directory.to_string_lossy().to_string();
        connection
            .execute(
                "INSERT INTO worksets (id, item_id, root_directory, branch)
                 VALUES (1, 1, ?1, 'legacy-branch')",
                [&legacy_workset_directory_path],
            )
            .expect("legacy workset should be inserted");
        connection
            .execute(
                "INSERT INTO runs
                    (id, item_id, workset_id, workspace_id, machine_id, agent,
                     execution_profile, prompt, working_directory, session_name, pane_id,
                     started_at)
                 VALUES (1, 1, 1, NULL, 1, 'codex', 'implement', 'remove me', '/tmp',
                         'session-1', '%1', 123)",
                [],
            )
            .expect("legacy run should be inserted");
        connection
            .execute(
                "INSERT INTO runs
                    (id, item_id, workset_id, workspace_id, machine_id, agent,
                     execution_profile, prompt, working_directory, session_name, pane_id,
                     started_at)
                 VALUES (2, 1, NULL, NULL, 1, 'codex', 'implement', 'keep me', '/tmp',
                         'session-2', '%2', 456)",
                [],
            )
            .expect("unassociated legacy run should be inserted");

        migrate_legacy_workset_data(&mut connection).expect("legacy data should migrate");

        assert!(table_columns(&connection, "worksets")
            .expect("workset table lookup should succeed")
            .is_empty());
        assert!(table_columns(&connection, "workset_repositories")
            .expect("workset repository table lookup should succeed")
            .is_empty());
        assert!(table_columns(&connection, "runs")
            .expect("runs table lookup should succeed")
            .contains(&"direct_checkouts_json".to_owned()));
        let direct_checkouts: String = connection
            .query_row(
                "SELECT direct_checkouts_json FROM runs WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .expect("surviving run should have direct checkout data");
        assert_eq!(direct_checkouts, "[]");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM runs WHERE id = 1", [], |row| row
                    .get::<_, i64>(0))
                .expect("deleted run count should be readable"),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("migration marker should be written"),
            1
        );
        assert!(legacy_workset_directory.join("keep.txt").exists());
    }

    #[test]
    fn rolls_back_legacy_cleanup_when_a_later_step_fails() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        create_legacy_schema(&connection);
        connection
            .execute(
                "INSERT INTO worksets (id, item_id, root_directory, branch)
                 VALUES (1, 1, '/tmp/legacy-workset', 'legacy-branch')",
                [],
            )
            .expect("legacy workset should be inserted");
        connection
            .execute(
                "INSERT INTO runs
                    (id, item_id, workset_id, workspace_id, machine_id, agent,
                     execution_profile, prompt, working_directory, session_name, pane_id,
                     started_at)
                 VALUES (1, 1, 1, NULL, 1, 'codex', 'implement', 'keep me', '/tmp',
                         'session-1', '%1', 123)",
                [],
            )
            .expect("legacy run should be inserted");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_workset_cleanup
                 BEFORE DELETE ON worksets
                 BEGIN
                     SELECT RAISE(ABORT, 'forced migration failure');
                 END;",
            )
            .expect("failure trigger should be created");

        let result = migrate_legacy_workset_data(&mut connection);
        assert!(result.is_err());

        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM worksets", [], |row| row
                    .get::<_, i64>(0))
                .expect("workset count should be readable"),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get::<_, i64>(0))
                .expect("run count should be readable"),
            1
        );
        assert!(table_columns(&connection, "runs")
            .expect("runs table lookup should succeed")
            .contains(&"workset_id".to_owned()));
        assert!(!table_columns(&connection, "runs")
            .expect("runs table lookup should succeed")
            .contains(&"direct_checkouts_json".to_owned()));
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("migration marker should be readable"),
            0
        );
    }

    #[test]
    fn round_trips_context_grill_defaults_and_run_snapshot() {
        let directory = tempdir().expect("temporary directory should exist");
        let database_path = directory.path().join("mission-manager.sqlite");
        let mut store = SqliteStore::open(&database_path).expect("database should open");
        let mut state = store.load_state().expect("initial state should load");

        state = apply_event(
            &mut store,
            state,
            Event::CreateContext {
                name: "Architecture".into(),
            },
        );
        state = apply_event(
            &mut store,
            state,
            Event::RegisterMachine {
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
            },
        );
        state = apply_event(
            &mut store,
            state,
            Event::RegisterRepositoryAtLocation {
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/mission-manager".into(),
                base_branch: "main".into(),
                machine_id: 1,
                checkout_path: "/tmp/mission-manager".into(),
                worktree_root: "/tmp/worktrees".into(),
            },
        );
        state = apply_event(
            &mut store,
            state,
            Event::CreateItem {
                title: "Choose an architecture".into(),
                context_id: 1,
                project_id: 1,
            },
        );
        state = apply_event(
            &mut store,
            state,
            Event::CreateWorkspace {
                item_id: 1,
                repositories: vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "main".into(),
                    base_branch: "main".into(),
                }],
            },
        );

        let configuration = GrillConfiguration {
            agent: AgentKind::Codex,
            model: "codex-luna".into(),
            effort: "xhigh".into(),
        };
        state = apply_event(
            &mut store,
            state,
            Event::SetContextGrillDefaults {
                context_id: 1,
                defaults: configuration.clone(),
            },
        );
        let prompt = compose_grill_prompt(
            &state,
            1,
            &configuration,
            "Stress-test the proposed architecture.",
        )
        .expect("Grill prompt should compose");
        state = apply_event(
            &mut store,
            state,
            Event::StartGrillRun {
                item_id: 1,
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                configuration,
                prompt,
                skill_snapshot: GRILL_SKILL_SNAPSHOT.into(),
                working_directory: "/tmp/mission-manager".into(),
                session_name: "mission-item-1-grill-1".into(),
                pane_id: "%1".into(),
                started_at: 123,
                checkouts: vec![RunCheckout {
                    repository_id: 1,
                    path: "/tmp/mission-manager".into(),
                    branch: "main".into(),
                    is_dirty: false,
                }],
            },
        );

        let reloaded = store.load_state().expect("persisted state should load");
        assert_eq!(reloaded, state);
        assert_eq!(reloaded.contexts[0].grill_defaults.model, "codex-luna");
        assert_eq!(reloaded.contexts[0].grill_defaults.effort, "xhigh");
        assert_eq!(
            reloaded.runs[0].execution_profile,
            crate::domain::ExecutionProfile::Grill
        );
        assert_eq!(reloaded.runs[0].model.as_deref(), Some("codex-luna"));
        assert_eq!(reloaded.runs[0].effort.as_deref(), Some("xhigh"));
        assert_eq!(
            reloaded.runs[0].skill_snapshot.as_deref(),
            Some(GRILL_SKILL_SNAPSHOT)
        );
    }
}
