use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use crate::domain::{
    Activity, Context, ContextAttentionDefault, DomainState, Effect, ExternalChangePolicy,
    ExternalMetadata, ExternalObject, ExternalObjectKind, ExternalProvider, ExternalSnapshot, Item,
    ItemRelation, ItemRelationKind, ItemStatus, Link, Project, ProjectDefaults,
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
                || !item_columns.iter().any(|column| column == "notes")
                || !item_columns.iter().any(|column| column == "reminder_at"))
        {
            return Err(StoreError::IncompatibleSchema);
        }
        initialize_schema(&mut connection)?;

        Ok(Self { connection })
    }

    pub fn load_state(&self) -> Result<DomainState, StoreError> {
        let next_context_id = self.sequence("next_context_id")?;
        let next_project_id = self.sequence("next_project_id")?;
        let next_item_id = self.sequence("next_item_id")?;
        let next_item_number = self.sequence("next_item_number")?;
        let next_external_object_id = self.sequence("next_external_object_id")?;
        let next_link_id = self.sequence("next_link_id")?;
        let next_activity_id = self.sequence("next_activity_id")?;
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
                "SELECT id, context_id, name, default_item_status
                 FROM projects
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let status: String = row.get(3)?;
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
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let items = {
            let mut statement = self.connection.prepare(
                "SELECT id, human_identifier, title, project_id, status, notes, reminder_at
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
                    reminder_at: row.get(6)?,
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
                        link_attention_state.metadata_attention
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
            next_external_object_id,
            next_link_id,
            next_activity_id,
            contexts,
            projects,
            items,
            relationships,
            external_objects,
            links,
            snapshots,
            activities,
            attention_defaults,
        })
    }

    pub fn apply(&mut self, effects: &[Effect]) -> Result<(), StoreError> {
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
                            (id, human_identifier, title, project_id, status, notes, reminder_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            item.id,
                            item.human_identifier,
                            item.title,
                            item.project_id,
                            item_status_as_str(item.status),
                            item.notes,
                            item.reminder_at,
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
                         SET status = ?1, notes = ?2, reminder_at = ?3
                         WHERE id = ?4",
                        params![
                            item_status_as_str(item.status),
                            item.notes,
                            item.reminder_at,
                            item.id,
                        ],
                    )?;
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
        self.connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'gh_executable_path'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|path| path.map(PathBuf::from))
            .map_err(StoreError::from)
    }

    pub fn set_gh_executable_path(&mut self, path: &Path) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES ('gh_executable_path', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![path.to_string_lossy().into_owned()],
        )?;
        Ok(())
    }
}

fn initialize_schema(connection: &mut Connection) -> Result<(), StoreError> {
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
             UNIQUE (context_id, name)
         );
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_context_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_project_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_number', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_external_object_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_link_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_activity_id', 1);
         INSERT OR IGNORE INTO contexts (id, name) VALUES (1, 'Personal');",
    )?;

    ensure_default_projects(connection)?;

    if table_columns(connection, "items")?.is_empty() {
        create_items_table(connection)?;
    }

    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS projects_by_context
             ON projects (context_id);
         CREATE INDEX IF NOT EXISTS items_by_project_and_status
             ON items (project_id, status);
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
             metadata_attention INTEGER
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
             ON activities (external_object_id, id);",
    )?;
    ensure_sequence_at_least(connection, "next_context_id", "contexts", "id")?;
    ensure_sequence_at_least(connection, "next_project_id", "projects", "id")?;
    ensure_sequence_at_least(connection, "next_item_id", "items", "id")?;
    ensure_sequence_at_least(
        connection,
        "next_external_object_id",
        "external_objects",
        "id",
    )?;
    ensure_sequence_at_least(connection, "next_link_id", "external_links", "id")?;
    ensure_sequence_at_least(connection, "next_activity_id", "activities", "id")?;

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
             notes TEXT NOT NULL DEFAULT '',
             reminder_at TEXT
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
            (link_id, reviewed_activity_id, title_attention, state_attention, metadata_attention)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(link_id) DO UPDATE SET
            reviewed_activity_id = excluded.reviewed_activity_id,
            title_attention = excluded.title_attention,
            state_attention = excluded.state_attention,
            metadata_attention = excluded.metadata_attention",
        params![
            link.id,
            link.reviewed_activity_id,
            title_attention,
            state_attention,
            metadata_attention,
        ],
    )?;
    Ok(())
}

fn bool_as_i64(value: bool) -> i64 {
    i64::from(value)
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
    use tempfile::tempdir;

    use super::*;
    use crate::domain::{
        decide, Event, ExternalChangePolicy, ExternalMetadata, ExternalObjectInput,
        ExternalObjectKind, ExternalProvider, ExternalSnapshotData,
    };

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
                Event::SetItemReminder {
                    item_id: 1,
                    reminder_at: Some("2026-09-20T09:00".into()),
                },
            )
            .expect("Item reminder should update");
            store
                .apply(&reminded.effects)
                .expect("Item reminder should be persisted");

            let second_item = decide(
                reminded.state,
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
        assert_eq!(
            state.items[0].reminder_at.as_deref(),
            Some("2026-09-20T09:00")
        );
        assert_eq!(state.items[1].human_identifier, "MC-2");
        assert_eq!(state.relationships.len(), 1);
        assert_eq!(state.relationships[0].from_item_id, 1);
        assert_eq!(state.relationships[0].to_item_id, 2);
        assert_eq!(state.relationships[0].kind, ItemRelationKind::Blocks);
        assert_eq!(state.next_context_id, 3);
        assert_eq!(state.next_project_id, 4);
        assert_eq!(state.next_item_number, 3);
        assert_eq!(state.next_item_id, 3);
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
}
