use std::path::Path;

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::domain::{Context, DomainState, Effect, Item, ItemStatus};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid Item status in database: {0}")]
    InvalidItemStatus(String),
    #[error("invalid {key} value in database: {value}")]
    InvalidSequence { key: String, value: String },
}

pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS metadata (
                 key TEXT PRIMARY KEY NOT NULL,
                 value INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS contexts (
                 id INTEGER PRIMARY KEY NOT NULL,
                 name TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS items (
                 id INTEGER PRIMARY KEY NOT NULL,
                 human_identifier TEXT NOT NULL UNIQUE,
                 title TEXT NOT NULL,
                 context_id INTEGER NOT NULL REFERENCES contexts(id),
                 status TEXT NOT NULL CHECK (status IN ('Inbox', 'Active', 'Waiting', 'Done'))
             );
             CREATE INDEX IF NOT EXISTS items_by_context_and_status
                 ON items (context_id, status);
             INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_id', 1);
             INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_number', 1);
             INSERT OR IGNORE INTO contexts (id, name) VALUES (1, 'Personal');",
        )?;

        Ok(Self { connection })
    }

    pub fn load_state(&self) -> Result<DomainState, StoreError> {
        let next_item_id = self.sequence("next_item_id")?;
        let next_item_number = self.sequence("next_item_number")?;
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
        let items = {
            let mut statement = self.connection.prepare(
                "SELECT id, human_identifier, title, context_id, status
                 FROM items
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let status: String = row.get(4)?;
                Ok(Item {
                    id: row.get(0)?,
                    human_identifier: row.get(1)?,
                    title: row.get(2)?,
                    context_id: row.get(3)?,
                    status: parse_item_status(&status).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        Ok(DomainState {
            next_item_id,
            next_item_number,
            contexts,
            items,
        })
    }

    pub fn apply(&mut self, effects: &[Effect]) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        for effect in effects {
            match effect {
                Effect::PersistItem {
                    item,
                    next_item_number,
                    next_item_id,
                } => {
                    transaction.execute(
                        "INSERT INTO items
                            (id, human_identifier, title, context_id, status)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            item.id,
                            item.human_identifier,
                            item.title,
                            item.context_id,
                            item_status_as_str(item.status),
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
    match status {
        "Inbox" => Ok(ItemStatus::Inbox),
        "Active" => Ok(ItemStatus::Active),
        "Waiting" => Ok(ItemStatus::Waiting),
        "Done" => Ok(ItemStatus::Done),
        other => Err(StoreError::InvalidItemStatus(other.into())),
    }
}

#[cfg(test)]
mod tests {

    use tempfile::tempdir;

    use super::*;
    use crate::domain::{decide, Event, ItemStatus};

    #[test]
    fn an_item_is_available_after_reopening_the_database() {
        let directory = tempdir().expect("temporary database directory should exist");
        let path = directory.path().join("mission-manager.sqlite");

        {
            let mut store = SqliteStore::open(&path).expect("database should open");
            let decision = decide(
                store.load_state().expect("initial state should load"),
                Event::CreateItem {
                    title: "Remember this after restart".into(),
                    context_id: 1,
                },
            )
            .expect("item creation should succeed");

            store
                .apply(&decision.effects)
                .expect("item should be persisted");
        }

        let reopened = SqliteStore::open(&path).expect("database should reopen");
        let state = reopened.load_state().expect("persisted state should load");

        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].human_identifier, "MC-1");
        assert_eq!(state.items[0].title, "Remember this after restart");
        assert_eq!(state.items[0].context_id, 1);
        assert_eq!(state.items[0].status, ItemStatus::Inbox);
        assert_eq!(state.next_item_number, 2);
    }
}
