use crate::{Result, Storage};
use rusqlite::{OptionalExtension, params};

impl Storage {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let connection = self.connection()?;

        let value = connection
            .query_row(
                "
                SELECT value
                FROM settings
                WHERE key = ?1;
                ",
                [key],
                |row| row.get(0),
            )
            .optional()?;

        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let connection = self.connection()?;

        connection.execute(
            "
            INSERT INTO settings (key, value)
            VALUES (?1, ?2)
            ON CONFLICT(key)
            DO UPDATE SET value = excluded.value;
            ",
            params![key, value],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_setting() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        let storage = Storage::open(&database_path).unwrap();

        assert_eq!(storage.get_setting("input_mode").unwrap(), None);

        storage.set_setting("input_mode", "clipboard").unwrap();

        assert_eq!(
            storage.get_setting("input_mode").unwrap().as_deref(),
            Some("clipboard")
        );

        storage.set_setting("input_mode", "manual").unwrap();

        assert_eq!(
            storage.get_setting("input_mode").unwrap().as_deref(),
            Some("manual")
        );
    }
}
