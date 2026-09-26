//! SQLite adapter facade.
//!
//! Storage responsibilities live in the sibling modules: schema and legacy
//! data migration, state loading, effect application, audit history, settings,
//! and codecs. `SqliteStore` remains the one infrastructure boundary consumed
//! by the application Runtime.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use crate::domain::{
    Activity, AgentKind, AuditAction, AuditEntry, Context, ContextAttentionDefault, DomainState,
    Effect, ExecutionMode, ExecutionProfile, ExternalChangePolicy, ExternalMetadata,
    ExternalObject, ExternalObjectKind, ExternalProvider, ExternalSnapshot, GrillAnswer,
    GrillConfiguration, GrillContinuationAction, GrillPhase, GrillQuestionGroup,
    ImplementationQueue, Item, ItemRelation, ItemRelationKind, ItemStatus, Link, LinkProvenance,
    LinkPurpose, Machine, MachineObservation, Project, ProjectDefaults, Reminder, Repository,
    RepositoryLocation, Run, RunPaneStatus, RunState, Workspace, WorkspacePreparationState,
    WorkspaceRepository, Worktree,
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
    #[error("invalid Link purpose in database: {0}")]
    InvalidLinkPurpose(String),
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
    #[error("invalid Grill phase in database: {0}")]
    InvalidGrillPhase(String),
    #[error("invalid downstream action in database: {0}")]
    InvalidDownstreamAction(String),
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
        if !link_attention_columns.is_empty()
            && !link_attention_columns
                .iter()
                .any(|column| column == "provenance_json")
        {
            connection.execute(
                "ALTER TABLE link_attention_state ADD COLUMN provenance_json TEXT",
                [],
            )?;
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
}

mod audit;
mod codecs;
mod effects;
mod load;
mod schema;
mod settings;
#[cfg(test)]
mod tests;

use schema::{
    initialize_schema, migrate_legacy_workset_data, migrate_runs_for_grill, table_columns,
};
