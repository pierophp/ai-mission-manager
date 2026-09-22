use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use crate::domain::{
    Activity, AgentKind, AuditAction, AuditEntry, Context, ContextAttentionDefault, DomainState,
    Effect, ExecutionMode, ExecutionProfile, ExternalChangePolicy, ExternalMetadata,
    ExternalObject, ExternalObjectKind, ExternalProvider, ExternalSnapshot, Item, ItemRelation,
    ItemRelationKind, ItemStatus, Link, Machine, MachineObservation, Project, ProjectDefaults,
    Reminder, Repository, RepositoryLocation, Run, RunPaneStatus, RunState, Workspace,
    WorkspacePreparationState, WorkspaceRepository, Worktree,
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
            let mut statement = self
                .connection
                .prepare("SELECT id, name FROM contexts ORDER BY id")?;
            let rows = statement.query_map([], |row| {
                Ok(Context {
                    id: row.get(0)?,
                    name: row.get(1)?,
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
                        machine_id, agent, execution_profile,
                        prompt, working_directory, session_name, pane_id, started_at, state,
                        pane_status, direct_checkouts_json
                 FROM runs
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let agent: String = row.get(6)?;
                let execution_profile: String = row.get(7)?;
                let direct_checkouts_json: String = row.get(15)?;
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
                    prompt: row.get(8)?,
                    working_directory: row.get(9)?,
                    session_name: row.get(10)?,
                    pane_id: row.get(11)?,
                    started_at: row.get(12)?,
                    state: parse_run_state(&row.get::<_, String>(13)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            13,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    pane_status: parse_run_pane_status(&row.get::<_, String>(14)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                14,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    direct_checkouts: serde_json::from_str(&direct_checkouts_json).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                15,
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
                        "INSERT INTO contexts (id, name) VALUES (?1, ?2)",
                        params![context.id, context.name],
                    )?;
                    transaction.execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'next_context_id'",
                        params![next_context_id],
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
                        "INSERT INTO contexts (id, name) VALUES (?1, ?2)",
                        params![context.id, context.name],
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
                            execution_profile, prompt, working_directory, session_name, pane_id,
                            started_at, state, pane_status, direct_checkouts_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                        params![
                            run.id,
                            run.item_id,
                            run.workspace_id,
                            run.repository_id,
                            run.worktree_id,
                            run.machine_id,
                            agent_kind_as_str(run.agent),
                            execution_profile_as_str(run.execution_profile),
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
             name TEXT NOT NULL UNIQUE
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

fn migrate_runs_for_workspace_execution(connection: &mut Connection) -> Result<(), StoreError> {
    let columns = table_columns(connection, "runs")?;
    if columns.is_empty() || columns.iter().any(|column| column == "workspace_id") {
        return Ok(());
    }

    connection.execute_batch(
        "ALTER TABLE runs RENAME TO runs_legacy;
         CREATE TABLE runs (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id),
             workset_id INTEGER REFERENCES worksets(id),
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
             id, item_id, workset_id, workspace_id, repository_id, worktree_id, machine_id,
             agent, execution_profile,
             prompt, working_directory, session_name, pane_id, started_at, state, pane_status,
             direct_checkouts_json
         )
         SELECT id, item_id, workset_id, NULL, NULL, NULL, machine_id, agent, execution_profile,
                prompt, working_directory, session_name, pane_id, started_at, state, pane_status,
                '[]'
         FROM runs_legacy;
         DROP TABLE runs_legacy;",
    )?;
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
    let workset_repositories_exist = !table_columns(&transaction, "workset_repositories")?.is_empty();
    let run_columns = table_columns(&transaction, "runs")?;
    let runs_have_legacy_link = run_columns.iter().any(|column| column == "workset_id");
    let completed: Option<i64> = transaction
        .query_row(
            "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if completed == Some(1) && !worksets_exist && !workset_repositories_exist && !runs_have_legacy_link {
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
        transaction.execute_batch(
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
                    state, pane_status, direct_checkouts_json
             FROM runs_legacy;
             DROP TABLE runs_legacy;",
        )?;
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
    }
}

fn parse_execution_profile(profile: &str) -> Result<ExecutionProfile, StoreError> {
    match profile {
        "investigate" => Ok(ExecutionProfile::Investigate),
        "implement" => Ok(ExecutionProfile::Implement),
        "review" => Ok(ExecutionProfile::Review),
        "custom" => Ok(ExecutionProfile::CustomPrompt),
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

#[cfg(any())]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::domain::{
        decide, AgentKind, AttachedRepositoryInput, AuditAction, Event, ExecutionMode,
        ExternalChangePolicy, ExternalMetadata, ExternalObjectInput, ExternalObjectKind,
        ExternalProvider, ExternalSnapshotData, MachineTransport, RunPaneStatus, RunState,
        WorksetRepositoryInput, WorkspaceRepositoryInput,
    };

    #[test]
    fn audit_history_is_append_only_and_survives_reopening() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            store
                .apply_with_audit(
                    &[],
                    &[
                        AuditAction::ItemCreated { item_id: 1 },
                        AuditAction::ItemStatusChanged {
                            item_id: 1,
                            from: ItemStatus::Inbox,
                            to: ItemStatus::Done,
                        },
                    ],
                )
                .expect("audit actions should persist");

            let history = store
                .list_audit_history()
                .expect("audit history should be readable");
            assert_eq!(history.len(), 2);
            assert_eq!(history[0].id, 2);
            assert_eq!(history[1].id, 1);
            assert!(matches!(
                history[0].action,
                AuditAction::ItemStatusChanged {
                    item_id: 1,
                    from: ItemStatus::Inbox,
                    to: ItemStatus::Done,
                }
            ));
        }

        let history = SqliteStore::open(&path)
            .expect("database should reopen")
            .list_audit_history()
            .expect("audit history should survive reopening");
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|entry| entry.recorded_at > 0));
    }

    fn seed_legacy_workset(store: &mut SqliteStore) {
        store
            .connection
            .execute_batch(
                r#"DELETE FROM metadata WHERE key = 'workset_migration_completed';
                 INSERT INTO repositories (id, project_id, name, remote_url)
                     VALUES (1, 1, 'app', 'https://example.com/app.git');
                 INSERT INTO items (id, human_identifier, title, project_id, status, notes)
                     VALUES (1, 'MC-1', 'Legacy work', 1, 'Active', '');
                 INSERT INTO worksets (id, item_id, root_directory, branch, archived)
                     VALUES (1, 1, '/tmp/legacy-workset', 'main', 0);
                 INSERT INTO workset_repositories
                     (workset_id, repository_id, current_branch, is_dirty)
                     VALUES (1, 1, 'main', 0);
                 INSERT INTO machines
                     (id, context_id, name, socket_name, transport_json,
                      last_observed, last_observed_at)
                     VALUES (1, 1, 'Local Mac', 'mission', '{"kind":"local"}',
                             'available', 10);
                 INSERT INTO runs
                     (id, item_id, workset_id, machine_id, agent, execution_profile,
                      prompt, working_directory, session_name, pane_id, started_at,
                      state, pane_status)
                     VALUES (1, 1, 1, 1, 'codex', 'implement', 'prompt',
                             '/tmp/legacy-workset', 'legacy-session', '%1', 10,
                             'finished', 'available');
                 INSERT INTO external_objects
                     (id, provider, kind, external_key, canonical_url)
                     VALUES (1, 'github', 'issue', 'acme/app#1',
                             'https://github.com/acme/app/issues/1');
                 INSERT INTO external_links (id, item_id, external_object_id)
                     VALUES (1, 1, 1);
                 INSERT INTO activities
                     (id, external_object_id, observed_at, changes_json)
                     VALUES (1, 1, 10, '[]');
                 INSERT INTO audit_entries (id, recorded_at, action_json)
                     VALUES (1, 10, '{"action":"worksetCreated","workset_id":1}');"#,
            )
            .expect("legacy Workset data should be seeded");
    }

    #[test]
    fn opening_an_old_database_removes_legacy_workset_dependents_atomically() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        let legacy_directory = directory.path().join("legacy-workset");
        std::fs::create_dir(&legacy_directory).expect("legacy directory should exist");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            seed_legacy_workset(&mut store);
        }

        let store = SqliteStore::open(&path).expect("legacy database should migrate");
        let state = store.load_state().expect("migrated state should load");
        assert!(state.worksets.is_empty());
        assert!(state.runs.is_empty());
        assert!(state.activities.is_empty());
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM workset_repositories", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("workset repository count should be readable"),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_entries
                     WHERE LOWER(action_json) LIKE '%workset%'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("legacy audit count should be readable"),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("migration marker should be readable"),
            1
        );
        assert!(legacy_directory.exists());
    }

    #[test]
    fn legacy_workset_migration_rolls_back_when_cleanup_fails() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            seed_legacy_workset(&mut store);
            store
                .connection
                .execute_batch(
                    "CREATE TRIGGER fail_legacy_workset_cleanup
                     BEFORE DELETE ON runs
                     WHEN OLD.workset_id IS NOT NULL
                     BEGIN
                         SELECT RAISE(ABORT, 'forced legacy migration failure');
                     END;",
                )
                .expect("migration failure trigger should be created");
        }

        let error = match SqliteStore::open(&path) {
            Ok(_) => panic!("legacy migration should fail"),
            Err(error) => error,
        };
        assert!(error
            .to_string()
            .contains("forced legacy migration failure"));

        let connection = Connection::open(&path).expect("database should remain readable");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM worksets", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("Workset count should be readable"),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get::<_, i64>(0))
                .expect("Run count should be readable"),
            1
        );
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
    fn resetting_local_data_clears_every_local_table_and_preserves_setup_and_sequences() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        let mut store = SqliteStore::open(&path).expect("database should open");
        store
            .set_setting("setup_completed", "true")
            .expect("setup setting should persist");
        store
            .set_setting("provider_choice", "github")
            .expect("provider setting should persist");
        store
            .set_setting("tmux_executable_path", "/custom/tmux")
            .expect("dependency setting should persist");
        store
            .connection
            .execute_batch(
                r#"INSERT INTO contexts (id, name) VALUES (8, 'Work');
                 INSERT INTO projects (id, context_id, name, default_item_status)
                     VALUES (14, 8, 'Billing', 'Active');
                 INSERT INTO repositories (id, project_id, name, remote_url)
                     VALUES (1, 1, 'app', 'https://example.com/app.git');
                 INSERT INTO items (id, human_identifier, title, project_id, status, notes)
                     VALUES (1, 'MC-1', 'Old work', 1, 'Active', 'old notes');
                 INSERT INTO reminders (id, item_id, remind_at)
                     VALUES (1, 1, '2026-09-20T12:00');
                 INSERT INTO worksets (id, item_id, root_directory, branch, archived)
                     VALUES (1, 1, '/tmp/workset', 'main', 0);
                 INSERT INTO workset_repositories
                     (workset_id, repository_id, current_branch, is_dirty)
                     VALUES (1, 1, 'main', 0);
                 INSERT INTO machines
                     (id, context_id, name, socket_name, transport_json,
                      last_observed, last_observed_at)
                     VALUES (1, 1, 'Local Mac', 'mission', '{"kind":"local"}',
                             'available', 10);
                 INSERT INTO runs
                     (id, item_id, workset_id, machine_id, agent, execution_profile,
                      prompt, working_directory, session_name, pane_id, started_at,
                      state, pane_status)
                     VALUES (1, 1, 1, 1, 'codex', 'implement', 'prompt',
                             '/tmp/workset', 'session', '%1', 10, 'finished', 'available');
                 INSERT INTO item_relationships (from_item_id, to_item_id, kind)
                     VALUES (1, 1, 'related_to');
                 INSERT INTO external_objects
                     (id, provider, kind, external_key, canonical_url)
                     VALUES (1, 'github', 'issue', 'acme/app#1', 'https://github.com/acme/app/issues/1');
                 INSERT INTO external_links (id, item_id, external_object_id)
                     VALUES (1, 1, 1);
                 INSERT INTO external_snapshots
                     (external_object_id, title, state, metadata_json, fetched_at)
                     VALUES (1, 'Old issue', 'OPEN', '[]', 10);
                 INSERT INTO link_attention_state
                     (link_id, reviewed_activity_id, title_attention, state_attention,
                      metadata_attention, watch_until, review_at)
                     VALUES (1, 0, 1, 1, 1, NULL, NULL);
                 INSERT INTO activities
                     (id, external_object_id, observed_at, changes_json)
                     VALUES (1, 1, 10, '[]');
                 INSERT INTO context_attention_defaults
                     (context_id, object_kind, title_attention, state_attention,
                      metadata_attention)
                     VALUES (1, 'issue', 1, 1, 1);
                 INSERT INTO audit_entries (id, recorded_at, action_json)
                     VALUES (1, 10, '{"action":"itemCreated","item_id":1}');
                 UPDATE metadata SET value = 9 WHERE key = 'next_context_id';
                 UPDATE metadata SET value = 15 WHERE key = 'next_project_id';
                 UPDATE metadata SET value = 4 WHERE key = 'next_item_id';
                 UPDATE metadata SET value = 8 WHERE key = 'next_item_number';
                 UPDATE metadata SET value = 6 WHERE key = 'next_repository_id';
                 UPDATE metadata SET value = 7 WHERE key = 'next_workset_id';
                 UPDATE metadata SET value = 8 WHERE key = 'next_machine_id';
                 UPDATE metadata SET value = 9 WHERE key = 'next_run_id';
                 UPDATE metadata SET value = 10 WHERE key = 'next_external_object_id';
                 UPDATE metadata SET value = 11 WHERE key = 'next_link_id';
                 UPDATE metadata SET value = 12 WHERE key = 'next_activity_id';
                 UPDATE metadata SET value = 13 WHERE key = 'next_reminder_id';
                 UPDATE metadata SET value = 2 WHERE key = 'next_audit_id';"#,
            )
            .expect("local data should be seeded");

        let decision = decide(
            store.load_state().expect("state should load"),
            Event::ResetLocalData,
        )
        .expect("reset should be decided");
        store
            .apply_with_audit(
                &decision.effects,
                &[AuditAction::ResetBoundary {
                    context_id: 9,
                    project_id: 15,
                }],
            )
            .expect("reset should persist atomically");

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("reset state should load");
        assert_eq!(state.contexts.len(), 1);
        assert_eq!(state.contexts[0].name, "Personal");
        assert_eq!(state.projects.len(), 1);
        assert_eq!(state.projects[0].name, "Default");
        assert_eq!(state.next_context_id, 10);
        assert_eq!(state.next_project_id, 16);
        assert_eq!(state.next_item_id, 4);
        assert_eq!(state.next_item_number, 8);
        assert_eq!(state.next_repository_id, 6);
        assert_eq!(state.next_workset_id, 7);
        assert_eq!(state.next_machine_id, 8);
        assert_eq!(state.next_run_id, 9);
        assert_eq!(state.next_external_object_id, 10);
        assert_eq!(state.next_link_id, 11);
        assert_eq!(state.next_activity_id, 12);
        assert_eq!(state.next_reminder_id, 13);
        assert!(state.repositories.is_empty());
        assert!(state.items.is_empty());
        assert!(state.worksets.is_empty());
        assert!(state.machines.is_empty());
        assert!(state.runs.is_empty());
        assert!(state.relationships.is_empty());
        assert!(state.external_objects.is_empty());
        assert!(state.links.is_empty());
        assert!(state.snapshots.is_empty());
        assert!(state.activities.is_empty());
        assert!(state.attention_defaults.is_empty());
        assert_eq!(
            reopened.setting("setup_completed").unwrap().as_deref(),
            Some("true")
        );
        assert_eq!(
            reopened.setting("provider_choice").unwrap().as_deref(),
            Some("github")
        );
        assert_eq!(
            reopened.setting("tmux_executable_path").unwrap().as_deref(),
            Some("/custom/tmux")
        );
        let history = reopened
            .list_audit_history()
            .expect("reset boundary should be readable");
        assert!(matches!(
            history.as_slice(),
            [AuditEntry {
                id: 2,
                action: AuditAction::ResetBoundary {
                    context_id: 9,
                    project_id: 15,
                },
                ..
            }]
        ));
    }

    #[test]
    fn reset_boundary_audit_entries_survive_reopening_as_the_new_audit_boundary() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            store
                .apply_with_audit(
                    &[],
                    &[AuditAction::ResetBoundary {
                        context_id: 8,
                        project_id: 9,
                    }],
                )
                .expect("reset boundary should persist");
        }

        let history = SqliteStore::open(&path)
            .expect("database should reopen")
            .list_audit_history()
            .expect("audit history should load");
        assert!(matches!(
            history.as_slice(),
            [AuditEntry {
                action: AuditAction::ResetBoundary {
                    context_id: 8,
                    project_id: 9,
                },
                ..
            }]
        ));
    }

    #[test]
    fn older_deletion_audit_entries_remain_readable_after_summary_fields_are_added() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        let store = SqliteStore::open(&path).expect("database should open");
        let legacy_actions = [
            r#"{"action":"worksetRemoved","workset_id":1}"#,
            r#"{"action":"repositoryDeleted","repository_id":2}"#,
            r#"{"action":"machineDeleted","machine_id":3}"#,
            r#"{"action":"linkDeleted","link_id":4}"#,
            r#"{"action":"externalObjectDeleted","external_object_id":5}"#,
        ];
        for (index, action) in legacy_actions.iter().enumerate() {
            store
                .connection
                .execute(
                    "INSERT INTO audit_entries (id, recorded_at, action_json)
                     VALUES (?1, 1, ?2)",
                    params![index as i64 + 1, action],
                )
                .expect("legacy audit entry should insert");
        }

        let history = store
            .list_audit_history()
            .expect("legacy audit entries should remain readable");
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::WorksetRemoved {
                workset_id: 1,
                repository_count: None,
            }
        )));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::RepositoryDeleted {
                repository_id: 2,
                workset_count: None,
            }
        )));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::MachineDeleted {
                machine_id: 3,
                run_count: None,
            }
        )));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::LinkDeleted {
                link_id: 4,
                external_object_id: None,
                external_object_deleted: None,
            }
        )));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::ExternalObjectDeleted {
                external_object_id: 5,
                link_count: None,
                snapshot_count: None,
                activity_count: None,
            }
        )));
    }

    #[test]
    fn parent_deletion_cascades_survive_reopening_and_keep_context_lifecycle_valid() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let context = decide(
                store.load_state().expect("state should load"),
                Event::CreateContext {
                    name: "Work".into(),
                },
            )
            .expect("Context should be created");
            store
                .apply(&context.effects)
                .expect("Context should persist");
            let project = decide(
                context.state,
                Event::CreateProject {
                    context_id: 2,
                    name: "Billing".into(),
                    defaults: ProjectDefaults::default(),
                },
            )
            .expect("Project should be created");
            store
                .apply(&project.effects)
                .expect("Project should persist");
            let repository = decide(
                project.state,
                Event::RegisterRepository {
                    project_id: 3,
                    name: "billing".into(),
                    remote_url: "https://example.com/billing.git".into(),
                },
            )
            .expect("Repository should be registered");
            store
                .apply(&repository.effects)
                .expect("Repository should persist");
            let item = decide(
                repository.state,
                Event::CreateItem {
                    title: "Delete this graph".into(),
                    context_id: 2,
                    project_id: 3,
                },
            )
            .expect("Item should be created");
            store.apply(&item.effects).expect("Item should persist");
            let workset = decide(
                item.state,
                Event::CreateWorkset {
                    item_id: 1,
                    root_directory: "/tmp/parent-deletion-round-trip".into(),
                    branch: "feature/parent-deletion".into(),
                    repositories: vec![WorksetRepositoryInput {
                        repository_id: 1,
                        branch_override: None,
                        base_branch_override: None,
                    }],
                },
            )
            .expect("Workset should be created");
            store
                .apply(&workset.effects)
                .expect("Workset should persist");
            let machine = decide(
                workset.state,
                Event::RegisterMachine {
                    context_id: 2,
                    name: "Work Mac".into(),
                    socket_name: "work".into(),
                    transport: MachineTransport::Local,
                },
            )
            .expect("Machine should be registered");
            store
                .apply(&machine.effects)
                .expect("Machine should persist");
            let run = decide(
                machine.state,
                Event::AttachRun {
                    item_id: 1,
                    workset_id: 1,
                    machine_id: 1,
                    agent: AgentKind::Codex,
                    working_directory: "/tmp/parent-deletion-round-trip".into(),
                    session_name: "parent-deletion".into(),
                    pane_id: "%1".into(),
                    attached_at: 1,
                },
            )
            .expect("Run should attach");
            store.apply(&run.effects).expect("Run should persist");
            let finished = decide(
                run.state,
                Event::UpdateRunState {
                    run_id: 1,
                    state: RunState::Finished,
                },
            )
            .expect("Run should finish");
            store.apply(&finished.effects).expect("Run should persist");
            let reminded = decide(
                finished.state,
                Event::AddItemReminder {
                    item_id: 1,
                    remind_at: "2026-09-20T09:00".into(),
                },
            )
            .expect("Reminder should be added");
            store
                .apply(&reminded.effects)
                .expect("Reminder should persist");
            let linked = decide(
                reminded.state,
                Event::LinkExternalObject {
                    item_id: 1,
                    object: ExternalObjectInput {
                        provider: ExternalProvider::Generic,
                        kind: ExternalObjectKind::Generic,
                        external_key: "parent-deletion-object".into(),
                        canonical_url: "https://example.com/parent-deletion".into(),
                    },
                    snapshot: Some(ExternalSnapshotData {
                        title: "Local object".into(),
                        state: "OPEN".into(),
                        metadata: Vec::new(),
                        fetched_at: 1,
                    }),
                },
            )
            .expect("External Object should link");
            store.apply(&linked.effects).expect("Link should persist");

            let plan = crate::domain::plan_project_deletion(&linked.state, 3)
                .expect("Project deletion plan should load");
            let deletion = decide(
                linked.state,
                Event::DeleteProject {
                    project_id: 3,
                    item_ids: plan.items.iter().map(|item| item.id).collect(),
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workset_ids: plan.worksets.iter().map(|workset| workset.id).collect(),
                },
            )
            .expect("Project cascade should decide");
            store
                .apply_with_audit(
                    &deletion.effects,
                    &[AuditAction::ProjectDeleted {
                        summary: plan.summary(),
                    }],
                )
                .expect("Project cascade should persist transactionally");
        }

        {
            let store = SqliteStore::open(&path).expect("database should reopen");
            let state = store.load_state().expect("state should reload");
            assert!(state.projects.iter().all(|project| project.id != 3));
            assert!(state.items.is_empty());
            assert!(state.repositories.is_empty());
            assert!(state.worksets.is_empty());
            assert!(state.runs.is_empty());
            assert!(state.machines.len() == 1);
            assert!(state.external_objects.is_empty());
            assert!(state.snapshots.is_empty());
            assert!(state.links.is_empty());
            assert_eq!(state.next_item_id, 2);
            assert_eq!(state.next_repository_id, 2);
            assert_eq!(state.next_workset_id, 2);
            assert_eq!(state.next_run_id, 2);
            assert!(store
                .list_audit_history()
                .expect("audit history should load")
                .iter()
                .any(|entry| matches!(entry.action, AuditAction::ProjectDeleted { .. })));
        }

        {
            let mut store = SqliteStore::open(&path).expect("database should reopen");
            let state = store.load_state().expect("state should load");
            let remove_default = decide(
                state,
                Event::DeleteProject {
                    project_id: 2,
                    item_ids: Vec::new(),
                    repository_ids: Vec::new(),
                    workset_ids: Vec::new(),
                },
            )
            .expect("the last Project in a Context may be removed");
            store
                .apply(&remove_default.effects)
                .expect("empty Project deletion should persist");
            let state = store.load_state().expect("state should load after cleanup");
            assert!(state.contexts.iter().any(|context| context.id == 2));
            assert!(state.projects.iter().all(|project| project.context_id != 2));

            let plan = crate::domain::plan_context_deletion(&state, 2)
                .expect("Context deletion plan should load");
            let remove_context = decide(
                state,
                Event::DeleteContext {
                    context_id: 2,
                    project_ids: plan.projects.iter().map(|project| project.id).collect(),
                    item_ids: plan.items.iter().map(|item| item.id).collect(),
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workset_ids: plan.worksets.iter().map(|workset| workset.id).collect(),
                    machine_ids: plan.machines.iter().map(|machine| machine.id).collect(),
                },
            )
            .expect("a non-last Context should be deletable");
            store
                .apply(&remove_context.effects)
                .expect("Context cascade should persist");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen after cascades");
        let state = reopened
            .load_state()
            .expect("state should reload after cascades");
        assert_eq!(state.contexts.len(), 1);
        assert_eq!(state.contexts[0].name, "Personal");
        assert!(state.projects.iter().all(|project| project.context_id == 1));
    }

    #[test]
    fn remote_machine_configuration_and_observation_survive_reopening() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let decision = decide(
                store.load_state().expect("state should load"),
                Event::RegisterMachine {
                    context_id: 1,
                    name: "Build host".into(),
                    socket_name: "mission-manager".into(),
                    transport: MachineTransport::Ssh {
                        host: "build.example.com".into(),
                        user: Some("runner".into()),
                        port: Some(2222),
                        identity_file: Some("/Users/me/.ssh/mission".into()),
                        known_hosts_file: Some("/Users/me/.ssh/known_hosts".into()),
                        strict_host_key_checking: Some("accept-new".into()),
                    },
                },
            )
            .expect("remote Machine should register");
            store
                .apply(&decision.effects)
                .expect("Machine should persist");
            let observed = decide(
                decision.state,
                Event::ObserveMachine {
                    machine_id: 1,
                    observation: crate::domain::MachineObservation::Available,
                    observed_at: 123,
                },
            )
            .expect("Machine observation should persist");
            store
                .apply(&observed.effects)
                .expect("Machine observation should be persisted");
        }

        let state = SqliteStore::open(&path)
            .expect("database should reopen")
            .load_state()
            .expect("state should reload");
        assert_eq!(state.machines[0].name, "Build host");
        assert_eq!(
            state.machines[0].transport,
            MachineTransport::Ssh {
                host: "build.example.com".into(),
                user: Some("runner".into()),
                port: Some(2222),
                identity_file: Some("/Users/me/.ssh/mission".into()),
                known_hosts_file: Some("/Users/me/.ssh/known_hosts".into()),
                strict_host_key_checking: Some("accept-new".into()),
            }
        );
        assert_eq!(
            state.machines[0].last_observed,
            crate::domain::MachineObservation::Available
        );
        assert_eq!(state.machines[0].last_observed_at, Some(123));
    }

    #[test]
    fn item_deletion_cascade_survives_reopening_and_keeps_shared_external_data() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let repository = decide(
                store.load_state().expect("state should load"),
                Event::RegisterRepository {
                    project_id: 1,
                    name: "service".into(),
                    remote_url: "https://example.com/service.git".into(),
                },
            )
            .expect("Repository should register");
            store
                .apply(&repository.effects)
                .expect("Repository should persist");
            let first_item = decide(
                repository.state,
                Event::CreateItem {
                    title: "Delete me".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("first Item should be created");
            store
                .apply(&first_item.effects)
                .expect("first Item should persist");
            let second_item = decide(
                first_item.state,
                Event::CreateItem {
                    title: "Keep me".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("second Item should be created");
            store
                .apply(&second_item.effects)
                .expect("second Item should persist");
            let workset = decide(
                second_item.state,
                Event::CreateWorkset {
                    item_id: 1,
                    root_directory: "/tmp/item-deletion-roundtrip".into(),
                    branch: "feature/delete-me".into(),
                    repositories: vec![WorksetRepositoryInput {
                        repository_id: 1,
                        branch_override: None,
                        base_branch_override: None,
                    }],
                },
            )
            .expect("Workset should be created");
            store
                .apply(&workset.effects)
                .expect("Workset should persist");
            let machine = decide(
                workset.state,
                Event::RegisterMachine {
                    context_id: 1,
                    name: "Local".into(),
                    socket_name: "mission-manager".into(),
                    transport: MachineTransport::Local,
                },
            )
            .expect("Machine should register");
            store
                .apply(&machine.effects)
                .expect("Machine should persist");
            let run = decide(
                machine.state,
                Event::AttachRun {
                    item_id: 1,
                    workset_id: 1,
                    machine_id: 1,
                    agent: AgentKind::Codex,
                    working_directory: "/tmp/item-deletion-roundtrip".into(),
                    session_name: "delete-me".into(),
                    pane_id: "%1".into(),
                    attached_at: 1,
                },
            )
            .expect("Run should attach");
            store.apply(&run.effects).expect("Run should persist");
            let finished = decide(
                run.state,
                Event::UpdateRunState {
                    run_id: 1,
                    state: RunState::Finished,
                },
            )
            .expect("Run should finish");
            store
                .apply(&finished.effects)
                .expect("Run state should persist");
            let pane_missing = decide(
                finished.state,
                Event::SetRunPaneStatus {
                    run_id: 1,
                    status: RunPaneStatus::Missing,
                },
            )
            .expect("Run Pane should be marked missing");
            store
                .apply(&pane_missing.effects)
                .expect("Pane state should persist");
            let reminded = decide(
                pane_missing.state,
                Event::AddItemReminder {
                    item_id: 1,
                    remind_at: "2026-09-20T09:00".into(),
                },
            )
            .expect("Reminder should be added");
            store
                .apply(&reminded.effects)
                .expect("Reminder should persist");
            let related = decide(
                reminded.state,
                Event::SetItemRelation {
                    from_item_id: 1,
                    to_item_id: 2,
                    kind: crate::domain::ItemRelationKind::RelatedTo,
                },
            )
            .expect("Items should be related");
            store
                .apply(&related.effects)
                .expect("Relationship should persist");
            let linked = decide(
                related.state,
                Event::LinkExternalObject {
                    item_id: 1,
                    object: ExternalObjectInput {
                        provider: ExternalProvider::GitHub,
                        kind: ExternalObjectKind::Issue,
                        external_key: "issue:shared".into(),
                        canonical_url: "https://example.com/shared".into(),
                    },
                    snapshot: Some(ExternalSnapshotData {
                        title: "Shared issue".into(),
                        state: "OPEN".into(),
                        metadata: vec![ExternalMetadata {
                            key: "author".into(),
                            value: "octocat".into(),
                        }],
                        fetched_at: 1,
                    }),
                },
            )
            .expect("External Object should link");
            store
                .apply(&linked.effects)
                .expect("External Object should persist");
            let second_link = decide(
                linked.state,
                Event::LinkExternalObject {
                    item_id: 2,
                    object: ExternalObjectInput {
                        provider: ExternalProvider::GitHub,
                        kind: ExternalObjectKind::Issue,
                        external_key: "issue:shared".into(),
                        canonical_url: "https://example.com/shared".into(),
                    },
                    snapshot: None,
                },
            )
            .expect("the shared External Object should link twice");
            store
                .apply(&second_link.effects)
                .expect("second Link should persist");
            let deletion = decide(second_link.state, Event::DeleteItem { item_id: 1 })
                .expect("the finished Item should delete");
            let summary = crate::domain::ItemDeletionSummary {
                item_id: 1,
                reminder_count: 1,
                relationship_count: 1,
                workset_count: 1,
                run_count: 1,
                link_count: 1,
                external_object_count: 0,
                snapshot_count: 0,
                activity_count: 0,
            };
            store
                .apply_with_audit(&deletion.effects, &[AuditAction::ItemDeleted { summary }])
                .expect("the Item cascade should persist transactionally");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("state should reload");
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].title, "Keep me");
        assert!(state.worksets.is_empty());
        assert!(state.runs.is_empty());
        assert!(state.relationships.is_empty());
        assert_eq!(state.links.len(), 1);
        assert_eq!(state.links[0].item_id, 2);
        assert_eq!(state.external_objects.len(), 1);
        assert_eq!(state.snapshots.len(), 1);
        assert!(reopened
            .list_audit_history()
            .expect("audit history should load")
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::ItemDeleted { .. })));
        assert_eq!(state.next_item_id, 3);
        assert_eq!(state.next_run_id, 2);
        assert_eq!(state.next_link_id, 3);
    }

    #[test]
    fn repository_deletion_round_trip_removes_worksets_and_preserves_audit_and_sequences() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let repository = decide(
                store.load_state().expect("state should load"),
                Event::RegisterRepository {
                    project_id: 1,
                    name: "service".into(),
                    remote_url: "https://example.com/service.git".into(),
                },
            )
            .expect("Repository should register");
            store
                .apply(&repository.effects)
                .expect("Repository should persist");
            let item = decide(
                repository.state,
                Event::CreateItem {
                    title: "Delete its Repository".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("Item should be created");
            store.apply(&item.effects).expect("Item should persist");
            let workset = decide(
                item.state,
                Event::CreateWorkset {
                    item_id: 1,
                    root_directory: "/tmp/repository-deletion-round-trip".into(),
                    branch: "feature/remove-repository".into(),
                    repositories: vec![WorksetRepositoryInput {
                        repository_id: 1,
                        branch_override: None,
                        base_branch_override: None,
                    }],
                },
            )
            .expect("Workset should be created");
            store
                .apply(&workset.effects)
                .expect("Workset should persist");
            let archived = decide(
                workset.state,
                Event::SetWorksetArchived {
                    workset_id: 1,
                    archived: true,
                },
            )
            .expect("Workset should be archived");
            store
                .apply(&archived.effects)
                .expect("archive should persist");
            let deletion = decide(
                archived.state,
                Event::DeleteRepository {
                    repository_id: 1,
                    workset_ids: vec![1],
                },
            )
            .expect("Repository deletion should be decided");
            store
                .apply_with_audit(
                    &deletion.effects,
                    &[
                        AuditAction::WorksetRemoved {
                            workset_id: 1,
                            repository_count: Some(1),
                        },
                        AuditAction::RepositoryDeleted {
                            repository_id: 1,
                            workset_count: Some(1),
                        },
                    ],
                )
                .expect("Repository deletion should persist transactionally");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("state should reload");
        assert!(state.repositories.is_empty());
        assert!(state.worksets.is_empty());
        assert_eq!(state.next_repository_id, 2);
        assert_eq!(state.next_workset_id, 2);
        let history = reopened
            .list_audit_history()
            .expect("audit history should load");
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::WorksetRemoved { workset_id: 1, .. }
        )));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::RepositoryDeleted {
                repository_id: 1,
                ..
            }
        )));
    }

    #[test]
    fn machine_deletion_round_trip_removes_finished_runs_and_preserves_the_workset() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let item = decide(
                store.load_state().expect("state should load"),
                Event::CreateItem {
                    title: "Delete the old Machine".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("Item should be created");
            store.apply(&item.effects).expect("Item should persist");
            let workset = decide(
                item.state,
                Event::AttachWorkset {
                    item_id: 1,
                    root_directory: "/tmp/machine-round-trip".into(),
                    repositories: vec![AttachedRepositoryInput {
                        name: "service".into(),
                        remote_url: "https://example.com/service.git".into(),
                        current_branch: "main".into(),
                        is_dirty: false,
                    }],
                },
            )
            .expect("Workset should attach");
            store
                .apply(&workset.effects)
                .expect("Workset should persist");
            let machine = decide(
                workset.state,
                Event::RegisterMachine {
                    context_id: 1,
                    name: "Old Machine".into(),
                    socket_name: "old-machine".into(),
                    transport: MachineTransport::Local,
                },
            )
            .expect("Machine should register");
            store
                .apply(&machine.effects)
                .expect("Machine should persist");
            let run = decide(
                machine.state,
                Event::AttachRun {
                    item_id: 1,
                    workset_id: 1,
                    machine_id: 1,
                    agent: AgentKind::Codex,
                    working_directory: "/tmp/machine-round-trip".into(),
                    session_name: "old-machine-run".into(),
                    pane_id: "%1".into(),
                    attached_at: 1,
                },
            )
            .expect("Run should attach");
            store.apply(&run.effects).expect("Run should persist");
            let finished = decide(
                run.state,
                Event::UpdateRunState {
                    run_id: 1,
                    state: RunState::Finished,
                },
            )
            .expect("Run should finish");
            store
                .apply(&finished.effects)
                .expect("finished state should persist");
            let deletion = decide(
                finished.state,
                Event::DeleteMachine {
                    machine_id: 1,
                    run_ids: vec![1],
                },
            )
            .expect("finished Machine Runs should be deletable");
            store
                .apply_with_audit(
                    &deletion.effects,
                    &[
                        AuditAction::RunDeleted { run_id: 1 },
                        AuditAction::MachineDeleted {
                            machine_id: 1,
                            run_count: Some(1),
                        },
                    ],
                )
                .expect("Machine deletion should persist transactionally");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("state should reload");
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.worksets.len(), 1);
        assert!(state.runs.is_empty());
        assert!(state.machines.is_empty());
        let history = reopened
            .list_audit_history()
            .expect("audit history should load");
        assert!(history
            .iter()
            .any(|entry| matches!(entry.action, AuditAction::RunDeleted { run_id: 1 })));
        assert!(history.iter().any(|entry| matches!(
            entry.action,
            AuditAction::MachineDeleted { machine_id: 1, .. }
        )));
    }

    #[test]
    fn contexts_projects_and_items_are_available_after_reopening_the_database() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let initial_state = store.load_state().expect("initial state should load");
            let context_decision = decide(
                initial_state,
                Event::CreateContext {
                    name: "Work".into(),
                },
            )
            .expect("Context creation should succeed");
            store
                .apply(&context_decision.effects)
                .expect("Context should be persisted");

            let project_decision = decide(
                context_decision.state,
                Event::CreateProject {
                    context_id: 2,
                    name: "Billing".into(),
                    defaults: ProjectDefaults {
                        item_status: ItemStatus::Active,
                        execution_mode: ExecutionMode::Worktree,
                    },
                },
            )
            .expect("Project creation should succeed");
            store
                .apply(&project_decision.effects)
                .expect("Project should be persisted");

            let item_decision = decide(
                project_decision.state,
                Event::CreateItem {
                    title: "Remember this after restart".into(),
                    context_id: 2,
                    project_id: 3,
                },
            )
            .expect("Item creation should succeed");
            store
                .apply(&item_decision.effects)
                .expect("Item should be persisted");

            let noted = decide(
                item_decision.state,
                Event::SetItemNotes {
                    item_id: 1,
                    notes: "Keep the migration checklist nearby".into(),
                },
            )
            .expect("Item notes should update");
            store
                .apply(&noted.effects)
                .expect("Item notes should be persisted");

            let reminded = decide(
                noted.state,
                Event::AddItemReminder {
                    item_id: 1,
                    remind_at: "2026-09-20T09:00".into(),
                },
            )
            .expect("first Item reminder should update");
            store
                .apply(&reminded.effects)
                .expect("first Item reminder should be persisted");
            let second_reminder = decide(
                reminded.state,
                Event::AddItemReminder {
                    item_id: 1,
                    remind_at: "2026-09-21T09:00".into(),
                },
            )
            .expect("second Item reminder should update");
            store
                .apply(&second_reminder.effects)
                .expect("second Item reminder should be persisted");

            let second_item = decide(
                second_reminder.state,
                Event::CreateItem {
                    title: "Review the migration".into(),
                    context_id: 2,
                    project_id: 3,
                },
            )
            .expect("the second Item should be created");
            store
                .apply(&second_item.effects)
                .expect("the second Item should be persisted");

            let relation = decide(
                second_item.state,
                Event::SetItemRelation {
                    from_item_id: 1,
                    to_item_id: 2,
                    kind: ItemRelationKind::Blocks,
                },
            )
            .expect("the Item relationship should be created");
            store
                .apply(&relation.effects)
                .expect("the Item relationship should be persisted");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("persisted state should load");

        assert_eq!(state.contexts.len(), 2);
        assert_eq!(state.projects.len(), 3);
        assert_eq!(state.projects[2].name, "Billing");
        assert_eq!(state.projects[2].defaults.item_status, ItemStatus::Active);
        assert_eq!(state.items.len(), 2);
        assert_eq!(state.items[0].human_identifier, "MC-1");
        assert_eq!(state.items[0].title, "Remember this after restart");
        assert_eq!(state.items[0].project_id, 3);
        assert_eq!(state.items[0].status, ItemStatus::Active);
        assert_eq!(state.items[0].notes, "Keep the migration checklist nearby");
        assert_eq!(state.items[0].reminders.len(), 2);
        assert_eq!(state.items[0].reminders[0].remind_at, "2026-09-20T09:00");
        assert_eq!(state.items[0].reminders[1].remind_at, "2026-09-21T09:00");
        assert_eq!(state.items[1].human_identifier, "MC-2");
        assert_eq!(state.relationships.len(), 1);
        assert_eq!(state.relationships[0].from_item_id, 1);
        assert_eq!(state.relationships[0].to_item_id, 2);
        assert_eq!(state.relationships[0].kind, ItemRelationKind::Blocks);
        assert_eq!(state.next_context_id, 3);
        assert_eq!(state.next_project_id, 4);
        assert_eq!(state.next_item_number, 3);
        assert_eq!(state.next_item_id, 3);
        assert_eq!(state.next_reminder_id, 3);
    }

    #[test]
    fn repositories_and_worksets_survive_reopening_the_database() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let registered = decide(
                store.load_state().expect("state should load"),
                Event::RegisterRepository {
                    project_id: 1,
                    name: "service-a".into(),
                    remote_url: "https://example.com/service-a.git".into(),
                },
            )
            .expect("Repository should register");
            store
                .apply(&registered.effects)
                .expect("Repository should persist");
            let item = decide(
                registered.state,
                Event::CreateItem {
                    title: "Build the platform change".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("Item should be created");
            store.apply(&item.effects).expect("Item should persist");
            let workset = decide(
                item.state,
                Event::CreateWorkset {
                    item_id: 1,
                    root_directory: "/tmp/worksets/platform-change".into(),
                    branch: "feature/platform-change".into(),
                    repositories: vec![WorksetRepositoryInput {
                        repository_id: 1,
                        branch_override: Some("feature/service-a".into()),
                        base_branch_override: Some("develop".into()),
                    }],
                },
            )
            .expect("Workset should be created");
            store
                .apply(&workset.effects)
                .expect("Workset should persist");
            let machine = decide(
                workset.state,
                Event::RegisterMachine {
                    context_id: 1,
                    name: "Local Mac".into(),
                    socket_name: "ai-mission-manager".into(),
                    transport: MachineTransport::Local,
                },
            )
            .expect("Machine should register");
            store
                .apply(&machine.effects)
                .expect("Machine should persist");
            let run = decide(
                machine.state,
                Event::AttachRun {
                    item_id: 1,
                    workset_id: 1,
                    machine_id: 1,
                    agent: AgentKind::Codex,
                    working_directory: "/tmp/worksets/platform-change".into(),
                    session_name: "mission-item-1-run-1".into(),
                    pane_id: "%1".into(),
                    attached_at: 123,
                },
            )
            .expect("Run should attach");
            store.apply(&run.effects).expect("Run should persist");
            let blocked = decide(
                run.state,
                Event::UpdateRunState {
                    run_id: 1,
                    state: RunState::Blocked,
                },
            )
            .expect("Run state should update");
            store
                .apply(&blocked.effects)
                .expect("Run state should persist");
            let missing = decide(
                blocked.state,
                Event::SetRunPaneStatus {
                    run_id: 1,
                    status: RunPaneStatus::Missing,
                },
            )
            .expect("Run Pane status should update");
            store
                .apply(&missing.effects)
                .expect("Run Pane status should persist");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("persisted state should load");

        assert_eq!(state.repositories.len(), 1);
        assert_eq!(state.repositories[0].name, "service-a");
        assert_eq!(state.repositories[0].project_id, 1);
        assert_eq!(state.worksets.len(), 1);
        assert_eq!(state.worksets[0].item_id, 1);
        assert_eq!(
            state.worksets[0].root_directory,
            "/tmp/worksets/platform-change"
        );
        assert_eq!(state.worksets[0].branch, "feature/platform-change");
        assert_eq!(state.worksets[0].repositories.len(), 1);
        assert_eq!(
            state.worksets[0].repositories[0],
            WorksetRepository {
                repository_id: 1,
                branch_override: Some("feature/service-a".into()),
                base_branch_override: Some("develop".into()),
                current_branch: "feature/service-a".into(),
                is_dirty: false,
            }
        );
        assert_eq!(state.next_repository_id, 2);
        assert_eq!(state.next_workset_id, 2);
        assert_eq!(state.machines.len(), 1);
        assert_eq!(state.machines[0].name, "Local Mac");
        assert_eq!(state.runs.len(), 1);
        assert_eq!(state.runs[0].agent, AgentKind::Codex);
        assert_eq!(state.runs[0].session_name, "mission-item-1-run-1");
        assert_eq!(state.runs[0].pane_id, "%1");
        assert_eq!(state.runs[0].state, RunState::Blocked);
        assert_eq!(state.runs[0].pane_status, RunPaneStatus::Missing);
        assert_eq!(state.next_machine_id, 2);
        assert_eq!(state.next_run_id, 2);
    }

    #[test]
    fn external_objects_links_snapshots_and_cli_path_survive_reopening() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        let gh_path = directory.path().join("gh");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let item = decide(
                store.load_state().expect("state should load"),
                Event::CreateItem {
                    title: "Track the external work".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("Item should be created");
            store.apply(&item.effects).expect("Item should persist");

            let linked = decide(
                item.state,
                Event::LinkExternalObject {
                    item_id: 1,
                    object: ExternalObjectInput {
                        provider: ExternalProvider::GitHub,
                        kind: ExternalObjectKind::Issue,
                        external_key: "issue:acme/app#7".into(),
                        canonical_url: "https://github.com/acme/app/issues/7".into(),
                    },
                    snapshot: Some(ExternalSnapshotData {
                        title: "Track the issue".into(),
                        state: "OPEN".into(),
                        metadata: vec![ExternalMetadata {
                            key: "author".into(),
                            value: "octocat".into(),
                        }],
                        fetched_at: 123,
                    }),
                },
            )
            .expect("Link should be created");
            store
                .apply(&linked.effects)
                .expect("External Object should persist");
            let configured = decide(
                linked.state,
                Event::SetContextAttentionDefault {
                    context_id: 1,
                    object_kind: ExternalObjectKind::Issue,
                    policy: ExternalChangePolicy {
                        title: false,
                        state: true,
                        metadata: false,
                    },
                },
            )
            .expect("the Context attention default should be configurable");
            store
                .apply(&configured.effects)
                .expect("the Context attention default should persist");
            let refreshed = decide(
                configured.state,
                Event::RefreshExternalObject {
                    external_object_id: 1,
                    snapshot: ExternalSnapshotData {
                        title: "Updated issue".into(),
                        state: "CLOSED".into(),
                        metadata: vec![ExternalMetadata {
                            key: "author".into(),
                            value: "new-author".into(),
                        }],
                        fetched_at: 456,
                    },
                },
            )
            .expect("the changed snapshot should persist");
            store
                .apply(&refreshed.effects)
                .expect("the Activity should persist");
            let reviewed = decide(refreshed.state, Event::MarkLinkReviewed { link_id: 1 })
                .expect("the review watermark should persist");
            store
                .apply(&reviewed.effects)
                .expect("the review watermark should persist");
            let overridden = decide(
                reviewed.state,
                Event::SetLinkAttentionPolicy {
                    link_id: 1,
                    policy: Some(ExternalChangePolicy {
                        title: true,
                        state: false,
                        metadata: true,
                    }),
                },
            )
            .expect("the Link attention policy should persist");
            store
                .apply(&overridden.effects)
                .expect("the Link attention policy should persist");
            let watched = decide(
                overridden.state,
                Event::SetLinkWatchUntil {
                    link_id: 1,
                    watch_until: Some("2026-09-20T09:00".into()),
                },
            )
            .expect("the watch period should persist");
            store
                .apply(&watched.effects)
                .expect("the watch period should persist");
            let scheduled = decide(
                watched.state,
                Event::SetLinkReviewAt {
                    link_id: 1,
                    review_at: Some("2026-09-25T09:00".into()),
                },
            )
            .expect("the review date should persist");
            store
                .apply(&scheduled.effects)
                .expect("the review date should persist");
            store
                .set_gh_executable_path(&gh_path)
                .expect("the resolved CLI path should persist");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("persisted state should load");

        assert_eq!(state.external_objects.len(), 1);
        assert_eq!(state.external_objects[0].external_key, "issue:acme/app#7");
        assert_eq!(state.links.len(), 1);
        assert_eq!(state.snapshots.len(), 1);
        assert_eq!(state.snapshots[0].title, "Updated issue");
        assert_eq!(state.snapshots[0].metadata[0].value, "new-author");
        assert_eq!(state.activities.len(), 1);
        assert_eq!(state.activities[0].changes.len(), 3);
        assert_eq!(state.links[0].reviewed_activity_id, 1);
        assert_eq!(
            state.links[0].watch_until.as_deref(),
            Some("2026-09-20T09:00")
        );
        assert_eq!(
            state.links[0].review_at.as_deref(),
            Some("2026-09-25T09:00")
        );
        assert_eq!(
            state.links[0].attention_policy,
            Some(ExternalChangePolicy {
                title: true,
                state: false,
                metadata: true,
            })
        );
        assert_eq!(state.attention_defaults.len(), 1);
        assert!(state.attention_defaults[0].policy.state);
        assert_eq!(
            reopened.gh_executable_path().expect("setting should load"),
            Some(gh_path)
        );
        assert_eq!(state.next_external_object_id, 2);
        assert_eq!(state.next_link_id, 2);
        assert_eq!(state.next_activity_id, 2);
    }

    #[test]
    fn link_and_external_object_deletion_round_trip_cleans_local_data() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let first_item = decide(
                store.load_state().expect("state should load"),
                Event::CreateItem {
                    title: "First commitment".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("first Item should be created");
            store
                .apply(&first_item.effects)
                .expect("first Item should persist");
            let second_item = decide(
                first_item.state,
                Event::CreateItem {
                    title: "Second commitment".into(),
                    context_id: 1,
                    project_id: 1,
                },
            )
            .expect("second Item should be created");
            store
                .apply(&second_item.effects)
                .expect("second Item should persist");
            let first_link = decide(
                second_item.state,
                Event::LinkExternalObject {
                    item_id: 1,
                    object: ExternalObjectInput {
                        provider: ExternalProvider::GitHub,
                        kind: ExternalObjectKind::Issue,
                        external_key: "issue:shared-lifecycle".into(),
                        canonical_url: "https://github.com/acme/app/issues/9".into(),
                    },
                    snapshot: Some(ExternalSnapshotData {
                        title: "Shared issue".into(),
                        state: "OPEN".into(),
                        metadata: vec![ExternalMetadata {
                            key: "author".into(),
                            value: "octocat".into(),
                        }],
                        fetched_at: 10,
                    }),
                },
            )
            .expect("first Link should be created");
            store
                .apply(&first_link.effects)
                .expect("first Link should persist");
            let second_link = decide(
                first_link.state,
                Event::LinkExternalObject {
                    item_id: 2,
                    object: ExternalObjectInput {
                        provider: ExternalProvider::GitHub,
                        kind: ExternalObjectKind::Issue,
                        external_key: "issue:shared-lifecycle".into(),
                        canonical_url: "https://github.com/acme/app/issues/9".into(),
                    },
                    snapshot: None,
                },
            )
            .expect("second Link should be created");
            store
                .apply(&second_link.effects)
                .expect("second Link should persist");
            let configured = decide(
                second_link.state,
                Event::SetLinkAttentionPolicy {
                    link_id: 2,
                    policy: Some(ExternalChangePolicy {
                        title: true,
                        state: false,
                        metadata: true,
                    }),
                },
            )
            .expect("the remaining Link attention state should be configurable");
            store
                .apply(&configured.effects)
                .expect("the remaining Link attention state should persist");
            let refreshed = decide(
                configured.state,
                Event::RefreshExternalObject {
                    external_object_id: 1,
                    snapshot: ExternalSnapshotData {
                        title: "Updated shared issue".into(),
                        state: "OPEN".into(),
                        metadata: vec![ExternalMetadata {
                            key: "author".into(),
                            value: "octocat".into(),
                        }],
                        fetched_at: 20,
                    },
                },
            )
            .expect("the changed snapshot should create an Activity");
            store
                .apply(&refreshed.effects)
                .expect("refresh should persist");

            let unlinked = decide(refreshed.state, Event::DeleteLink { link_id: 1 })
                .expect("the first Link should be removable");
            store
                .apply(&unlinked.effects)
                .expect("unlink should persist");
        }

        {
            let mut store = SqliteStore::open(&path).expect("database should reopen");
            let state = store.load_state().expect("shared state should reload");
            assert_eq!(state.links.len(), 1);
            assert_eq!(state.links[0].item_id, 2);
            assert_eq!(state.external_objects.len(), 1);
            assert_eq!(state.snapshots.len(), 1);
            assert_eq!(state.activities.len(), 1);
            assert_eq!(
                state.links[0].attention_policy,
                Some(ExternalChangePolicy {
                    title: true,
                    state: false,
                    metadata: true,
                })
            );
            let attention_rows: i64 = store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM link_attention_state WHERE link_id = 1",
                    [],
                    |row| row.get(0),
                )
                .expect("Link attention state should be queryable");
            assert_eq!(attention_rows, 0);

            let deleted = decide(
                state,
                Event::DeleteExternalObject {
                    external_object_id: 1,
                },
            )
            .expect("the shared External Object should be removable locally");
            store
                .apply(&deleted.effects)
                .expect("External Object deletion should persist");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen after deletion");
        let state = reopened.load_state().expect("deleted state should load");
        assert!(state.links.is_empty());
        assert!(state.external_objects.is_empty());
        assert!(state.snapshots.is_empty());
        assert!(state.activities.is_empty());
    }

    #[test]
    fn workspace_worktree_and_execution_defaults_survive_reopening() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        let mut store = SqliteStore::open(&path).expect("database should open");

        let context = decide(
            store.load_state().expect("initial state should load"),
            Event::CreateContext {
                name: "Work".into(),
            },
        )
        .expect("Context should be created");
        store
            .apply(&context.effects)
            .expect("Context should persist");

        let project = decide(
            context.state,
            Event::CreateProject {
                context_id: 1,
                name: "Direct execution".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Inbox,
                    execution_mode: ExecutionMode::Direct,
                },
            },
        )
        .expect("Project execution defaults should be configurable");
        store
            .apply(&project.effects)
            .expect("Project execution defaults should persist");
        assert_eq!(
            store
                .load_state()
                .expect("Project state should reload")
                .projects
                .iter()
                .find(|project| project.name == "Direct execution")
                .expect("Direct execution project should exist")
                .defaults
                .execution_mode,
            ExecutionMode::Direct
        );

        let repository = decide(
            project.state,
            Event::RegisterRepository {
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.com/mission-manager.git".into(),
            },
        )
        .expect("Repository should be registered");
        store
            .apply(&repository.effects)
            .expect("Repository should persist");

        let second_repository = decide(
            repository.state,
            Event::RegisterRepository {
                project_id: 1,
                name: "mission-manager-web".into(),
                remote_url: "https://example.com/mission-manager-web.git".into(),
            },
        )
        .expect("the second Repository should be registered");
        store
            .apply(&second_repository.effects)
            .expect("the second Repository should persist");

        let item = decide(
            second_repository.state,
            Event::CreateItem {
                title: "Implement execution contract".into(),
                context_id: 1,
                project_id: 1,
            },
        )
        .expect("Item should be created");
        store.apply(&item.effects).expect("Item should persist");

        let machine = decide(
            item.state,
            Event::RegisterMachine {
                context_id: 1,
                name: "Local Mac".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
            },
        )
        .expect("Machine should be registered");
        store
            .apply(&machine.effects)
            .expect("Machine should persist");

        let workspace = decide(
            machine.state,
            Event::CreateWorkspace {
                item_id: 1,
                repositories: vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "feature/contracts".into(),
                    base_branch: "main".into(),
                }],
            },
        )
        .expect("Workspace should be created");
        store
            .apply(&workspace.effects)
            .expect("Workspace should persist");

        let second_workspace = decide(
            workspace.state,
            Event::CreateWorkspace {
                item_id: 1,
                repositories: vec![WorkspaceRepositoryInput {
                    repository_id: 2,
                    branch: "feature/web-contracts".into(),
                    base_branch: "main".into(),
                }],
            },
        )
        .expect("the second Workspace should be created");
        store
            .apply(&second_workspace.effects)
            .expect("the second Workspace should persist");

        let worktree = decide(
            second_workspace.state,
            Event::CreateWorktree {
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                path: "/Users/piero/worktrees/feature-contracts/mission-manager".into(),
                branch: "feature/contracts".into(),
                base_branch: "main".into(),
                is_dirty: false,
            },
        )
        .expect("Worktree should be created");
        store
            .apply(&worktree.effects)
            .expect("Worktree should persist");

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened
            .load_state()
            .expect("new execution state should load");
        assert_eq!(
            state.projects[0].defaults.execution_mode,
            ExecutionMode::Worktree
        );
        assert_eq!(
            state
                .projects
                .iter()
                .find(|project| project.name == "Direct execution")
                .expect("Direct execution project should exist")
                .defaults
                .execution_mode,
            ExecutionMode::Direct
        );
        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.workspaces[0].repositories.len(), 1);
        assert_eq!(state.workspaces[0].repositories[0].base_branch, "main");
        assert_eq!(state.workspaces[1].id, 2);
        assert_eq!(state.workspaces[1].item_id, 1);
        assert_eq!(state.workspaces[1].repositories[0].repository_id, 2);
        assert_eq!(
            state.workspaces[1].repositories[0].branch,
            "feature/web-contracts"
        );
        assert_eq!(state.worktrees.len(), 1);
        assert_eq!(state.worktrees[0].workspace_id, state.workspaces[0].id);
        assert_eq!(
            state.worktrees[0].path,
            "/Users/piero/worktrees/feature-contracts/mission-manager"
        );
        assert_eq!(state.next_workspace_id, 3);
        assert_eq!(state.next_worktree_id, 2);
    }

    #[test]
    fn repository_base_branch_and_machine_location_survive_reopening() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");
        let mut store = SqliteStore::open(&path).expect("database should open");

        let machine = decide(
            store.load_state().expect("initial state should load"),
            Event::RegisterMachine {
                context_id: 1,
                name: "Local Mac".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
            },
        )
        .expect("Machine should be registered");
        store
            .apply(&machine.effects)
            .expect("Machine should persist");

        let repository = decide(
            machine.state,
            Event::RegisterRepositoryAtLocation {
                project_id: 1,
                name: "service-a".into(),
                remote_url: "https://example.com/service-a.git".into(),
                base_branch: "trunk".into(),
                machine_id: 1,
                checkout_path: "~/src/service-a".into(),
                worktree_root: "~/worktrees".into(),
            },
        )
        .expect("Repository should register");
        store
            .apply(&repository.effects)
            .expect("Repository location should persist");

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("state should reload");
        assert_eq!(state.repositories[0].base_branch, "trunk");
        assert_eq!(state.repository_locations.len(), 1);
        assert_eq!(
            state.repository_locations[0].checkout_path,
            "~/src/service-a"
        );
        assert_eq!(state.repository_locations[0].worktree_root, "~/worktrees");
    }
}
