//! Settings of one download: a speed limit on top of the global one, and a
//! folder chosen for it when it was added.

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

impl Storage {
    /// The folder chosen for this download, if one was.
    pub fn get_download_folder(&self, download_id: &str) -> Result<Option<String>> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT directory FROM download_folders WHERE download_id = ?1;",
                [download_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Sets or (with `None`) removes the folder chosen for this download.
    pub fn set_download_folder(&self, download_id: &str, directory: Option<&str>) -> Result<()> {
        let connection = self.connection()?;
        let exists = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM downloads WHERE id = ?1);",
            [download_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StorageError::DownloadNotFound(download_id.to_owned()));
        }
        match directory.map(str::trim).filter(|value| !value.is_empty()) {
            Some(directory) => {
                connection.execute(
                    "INSERT INTO download_folders (download_id, directory) VALUES (?1, ?2)
                     ON CONFLICT(download_id) DO UPDATE SET directory = excluded.directory;",
                    params![download_id, directory],
                )?;
            }
            None => {
                connection.execute(
                    "DELETE FROM download_folders WHERE download_id = ?1;",
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

    #[test]
    fn a_download_keeps_the_folder_chosen_for_it() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/a.iso", 1)
            .unwrap();

        assert_eq!(storage.get_download_folder(&task.id).unwrap(), None);
        storage
            .set_download_folder(&task.id, Some(r"D:\Movies"))
            .unwrap();
        assert_eq!(
            storage.get_download_folder(&task.id).unwrap().as_deref(),
            Some(r"D:\Movies")
        );
        storage.set_download_folder(&task.id, Some("  ")).unwrap();
        assert_eq!(storage.get_download_folder(&task.id).unwrap(), None);

        storage
            .set_download_folder(&task.id, Some("/tmp/x"))
            .unwrap();
        storage.remove_download_record(&task.id).unwrap();
        assert!(matches!(
            storage.set_download_folder(&task.id, Some("/tmp/x")),
            Err(StorageError::DownloadNotFound(_))
        ));
    }
}
