//! A speed limit for one download, on top of the global one.

use crate::{Result, Storage, StorageError};
use rusqlite::{OptionalExtension, params};

impl Storage {
    /// The download's own limit in bytes per second, if it has one.
    pub fn get_speed_limit(&self, download_id: &str) -> Result<Option<u64>> {
        let connection = self.connection()?;
        let value = connection
            .query_row(
                "SELECT bytes_per_second FROM download_limits WHERE download_id = ?1;",
                [download_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        Ok(value.and_then(|value| u64::try_from(value).ok()))
    }

    /// Sets or (with `None` or zero) removes the download's own limit.
    pub fn set_speed_limit(&self, download_id: &str, limit: Option<u64>) -> Result<()> {
        let connection = self.connection()?;
        let exists = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM downloads WHERE id = ?1);",
            [download_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StorageError::DownloadNotFound(download_id.to_owned()));
        }
        match limit.filter(|limit| *limit > 0) {
            Some(limit) => {
                let limit = i64::try_from(limit).unwrap_or(i64::MAX);
                connection.execute(
                    "INSERT INTO download_limits (download_id, bytes_per_second) VALUES (?1, ?2)
                     ON CONFLICT(download_id) DO UPDATE SET bytes_per_second = excluded.bytes_per_second;",
                    params![download_id, limit],
                )?;
            }
            None => {
                connection.execute(
                    "DELETE FROM download_limits WHERE download_id = ?1;",
                    [download_id],
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Storage, StorageError};
    use tempfile::tempdir;

    #[test]
    fn a_download_keeps_its_own_limit_until_removed() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/a.iso", 1)
            .unwrap();

        assert_eq!(storage.get_speed_limit(&task.id).unwrap(), None);
        storage.set_speed_limit(&task.id, Some(500_000)).unwrap();
        storage.set_speed_limit(&task.id, Some(250_000)).unwrap();
        assert_eq!(storage.get_speed_limit(&task.id).unwrap(), Some(250_000));
        storage.set_speed_limit(&task.id, Some(0)).unwrap();
        assert_eq!(storage.get_speed_limit(&task.id).unwrap(), None);

        storage.set_speed_limit(&task.id, Some(1_000)).unwrap();
        storage.remove_download_record(&task.id).unwrap();
        assert!(matches!(
            storage.set_speed_limit(&task.id, Some(1)),
            Err(StorageError::DownloadNotFound(_))
        ));
    }
}
