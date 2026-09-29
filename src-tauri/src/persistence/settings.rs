use super::*;

impl SqliteStore {
    pub(super) fn sequence(&self, key: &str) -> Result<i64, StoreError> {
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

    pub fn set_context_gh_executable_path(
        &mut self,
        context_id: i64,
        path: &Path,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE contexts SET gh_executable_path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), context_id],
        )?;
        Ok(())
    }

    pub fn set_context_twg_executable_path(
        &mut self,
        context_id: i64,
        path: &Path,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE contexts SET twg_executable_path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), context_id],
        )?;
        Ok(())
    }

    pub fn set_context_az_executable_path(
        &mut self,
        context_id: i64,
        path: &Path,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE contexts SET az_executable_path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), context_id],
        )?;
        Ok(())
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
