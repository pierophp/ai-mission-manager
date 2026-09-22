use super::*;

impl SqliteStore {
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
}

pub(super) fn append_audit_actions(
    transaction: &rusqlite::Transaction<'_>,
    audit_actions: &[AuditAction],
) -> Result<(), StoreError> {
    if audit_actions.is_empty() {
        return Ok(());
    }

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
    Ok(())
}
