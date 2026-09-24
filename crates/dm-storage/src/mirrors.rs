use crate::{Result, Storage, StorageError};
use rusqlite::params;

/// Most mirrors kept for one download.
pub const MAX_MIRRORS: usize = 16;

impl Storage {
    /// The other addresses of a download's file, in the order given.
    pub fn list_mirrors(&self, download_id: &str) -> Result<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT url FROM download_mirrors WHERE download_id = ?1 ORDER BY position ASC;",
        )?;
        let rows = statement.query_map([download_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| row.map_err(StorageError::from)).collect()
    }

    /// Replaces the mirrors of a download. Duplicates are dropped and the
    /// list is capped; callers validate that each entry is an http(s) URL.
    pub fn set_mirrors(&self, download_id: &str, urls: &[String]) -> Result<Vec<String>> {
        let mut unique: Vec<String> = Vec::new();
        for url in urls
            .iter()
            .map(|url| url.trim())
            .filter(|url| !url.is_empty())
        {
            if url.len() > 4096 || url.chars().any(char::is_control) {
                return Err(StorageError::InvalidMirror(
                    "a mirror address is too long or contains control characters".to_owned(),
                ));
            }
            if !unique.iter().any(|known| known == url) {
                unique.push(url.to_owned());
            }
        }
        if unique.len() > MAX_MIRRORS {
            return Err(StorageError::InvalidMirror(format!(
                "at most {MAX_MIRRORS} mirrors can be kept for one download"
            )));
        }

        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM downloads WHERE id = ?1);",
            [download_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StorageError::DownloadNotFound(download_id.to_owned()));
        }
        transaction.execute(
            "DELETE FROM download_mirrors WHERE download_id = ?1;",
            [download_id],
        )?;
        for (position, url) in unique.iter().enumerate() {
            transaction.execute(
                "INSERT INTO download_mirrors (download_id, url, position) VALUES (?1, ?2, ?3);",
                params![download_id, url, position as i64],
            )?;
        }
        transaction.commit()?;
        Ok(unique)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn mirrors_keep_their_order_without_duplicates_and_go_with_the_download() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://a.example/file.iso", 1)
            .unwrap();

        let saved = storage
            .set_mirrors(
                &task.id,
                &[
                    "https://b.example/file.iso".to_owned(),
                    " ".to_owned(),
                    "https://c.example/file.iso".to_owned(),
                    "https://b.example/file.iso".to_owned(),
                ],
            )
            .unwrap();
        assert_eq!(saved, storage.list_mirrors(&task.id).unwrap());
        assert_eq!(
            saved,
            vec!["https://b.example/file.iso", "https://c.example/file.iso"]
        );

        storage.remove_download_record(&task.id).unwrap();
        assert!(storage.list_mirrors(&task.id).unwrap().is_empty());
    }

    #[test]
    fn unknown_downloads_and_oversized_lists_are_refused() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        assert!(storage.set_mirrors("missing", &[]).is_err());

        let task = storage
            .create_download("https://a.example/file.iso", 1)
            .unwrap();
        let many = (0..=MAX_MIRRORS)
            .map(|index| format!("https://m{index}.example/file.iso"))
            .collect::<Vec<_>>();
        assert!(storage.set_mirrors(&task.id, &many).is_err());
    }
}
