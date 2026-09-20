use crate::{Result, Storage, StorageError};
use dm_common::{DownloadCompletion, DownloadRecord, DownloadStatus};
use rusqlite::{OptionalExtension, Row, params};
use std::str::FromStr;
use uuid::Uuid;

const DOWNLOAD_SELECT: &str = r#"
SELECT
    id,
    source_url,
    resolved_url,
    filename,
    destination_path,
    temp_path,
    mime_type,
    total_bytes,
    downloaded_bytes,
    etag,
    last_modified,
    range_supported,
    status,
    queue_position,
    created_at,
    started_at,
    completed_at,
    error_code,
    error_message
FROM downloads
"#;

#[derive(Debug)]
struct StoredDownloadRow {
    id: String,
    source_url: String,
    resolved_url: Option<String>,
    filename: Option<String>,
    destination_path: Option<String>,
    temp_path: Option<String>,
    mime_type: Option<String>,
    total_bytes: Option<i64>,
    downloaded_bytes: i64,
    etag: Option<String>,
    last_modified: Option<String>,
    range_supported: Option<i64>,
    status: String,
    queue_position: Option<i64>,
    created_at: i64,
    started_at: Option<i64>,
    completed_at: Option<i64>,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl StoredDownloadRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            source_url: row.get(1)?,
            resolved_url: row.get(2)?,
            filename: row.get(3)?,
            destination_path: row.get(4)?,
            temp_path: row.get(5)?,
            mime_type: row.get(6)?,
            total_bytes: row.get(7)?,
            downloaded_bytes: row.get(8)?,
            etag: row.get(9)?,
            last_modified: row.get(10)?,
            range_supported: row.get(11)?,
            status: row.get(12)?,
            queue_position: row.get(13)?,
            created_at: row.get(14)?,
            started_at: row.get(15)?,
            completed_at: row.get(16)?,
            error_code: row.get(17)?,
            error_message: row.get(18)?,
        })
    }

    fn into_record(self) -> Result<DownloadRecord> {
        let status = DownloadStatus::from_str(&self.status)
            .map_err(|_| StorageError::InvalidDownloadStatus(self.status.clone()))?;

        let total_bytes = self
            .total_bytes
            .map(|value| i64_to_u64(value, "total_bytes"))
            .transpose()?;

        let downloaded_bytes = i64_to_u64(self.downloaded_bytes, "downloaded_bytes")?;

        Ok(DownloadRecord {
            id: self.id,
            source_url: self.source_url,
            resolved_url: self.resolved_url,
            filename: self.filename,
            destination_path: self.destination_path,
            temp_path: self.temp_path,
            mime_type: self.mime_type,
            total_bytes,
            downloaded_bytes,
            etag: self.etag,
            last_modified: self.last_modified,
            range_supported: self.range_supported.map(|value| value != 0),
            status,
            queue_position: self.queue_position,
            created_at: self.created_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
            error_code: self.error_code,
            error_message: self.error_message,
        })
    }
}

impl Storage {
    pub fn create_download(&self, source_url: &str, created_at: i64) -> Result<DownloadRecord> {
        let record = DownloadRecord {
            id: Uuid::new_v4().to_string(),
            source_url: source_url.to_owned(),
            resolved_url: None,
            filename: None,
            destination_path: None,
            temp_path: None,
            mime_type: None,
            total_bytes: None,
            downloaded_bytes: 0,
            etag: None,
            last_modified: None,
            range_supported: None,
            status: DownloadStatus::Created,
            queue_position: None,
            created_at,
            started_at: None,
            completed_at: None,
            error_code: None,
            error_message: None,
        };

        let connection = self.connection()?;

        connection.execute(
            r#"
            INSERT INTO downloads (
                id,
                source_url,
                downloaded_bytes,
                status,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5);
            "#,
            params![
                &record.id,
                &record.source_url,
                0_i64,
                record.status.as_str(),
                record.created_at,
            ],
        )?;

        Ok(record)
    }

    pub fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>> {
        let connection = self.connection()?;
        let sql = format!("{DOWNLOAD_SELECT} WHERE id = ?1");

        let stored = connection
            .query_row(&sql, [id], StoredDownloadRow::from_row)
            .optional()?;

        stored.map(StoredDownloadRow::into_record).transpose()
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>> {
        let connection = self.connection()?;
        let sql = format!("{DOWNLOAD_SELECT} ORDER BY created_at DESC, id ASC");

        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], StoredDownloadRow::from_row)?;

        let mut downloads = Vec::new();

        for row in rows {
            downloads.push(row?.into_record()?);
        }

        Ok(downloads)
    }

    pub fn mark_downloading(&self, id: &str, started_at: i64) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET
                status = 'downloading',
                started_at = ?2,
                error_code = NULL,
                error_message = NULL
            WHERE id = ?1;
            "#,
            params![id, started_at],
        )?;

        ensure_updated(id, changed)
    }

    pub fn update_progress(
        &self,
        id: &str,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<()> {
        let downloaded_bytes = u64_to_i64(downloaded_bytes, "downloaded_bytes")?;

        let total_bytes = total_bytes
            .map(|value| u64_to_i64(value, "total_bytes"))
            .transpose()?;

        let connection = self.connection()?;

        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET
                downloaded_bytes = ?2,
                total_bytes = COALESCE(?3, total_bytes)
            WHERE id = ?1;
            "#,
            params![id, downloaded_bytes, total_bytes],
        )?;

        ensure_updated(id, changed)
    }

    pub fn mark_completed(
        &self,
        id: &str,
        completion: &DownloadCompletion,
        completed_at: i64,
    ) -> Result<()> {
        let downloaded_bytes = u64_to_i64(completion.downloaded_bytes, "downloaded_bytes")?;

        let total_bytes = completion
            .total_bytes
            .map(|value| u64_to_i64(value, "total_bytes"))
            .transpose()?;

        let connection = self.connection()?;

        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET
                resolved_url = ?2,
                filename = ?3,
                destination_path = ?4,
                temp_path = NULL,
                mime_type = ?5,
                total_bytes = ?6,
                downloaded_bytes = ?7,
                status = 'completed',
                completed_at = ?8,
                error_code = NULL,
                error_message = NULL
            WHERE id = ?1;
            "#,
            params![
                id,
                &completion.resolved_url,
                &completion.filename,
                &completion.destination_path,
                &completion.mime_type,
                total_bytes,
                downloaded_bytes,
                completed_at,
            ],
        )?;

        ensure_updated(id, changed)
    }

    pub fn mark_failed(&self, id: &str, error_code: &str, error_message: &str) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET
                status = 'failed',
                error_code = ?2,
                error_message = ?3
            WHERE id = ?1;
            "#,
            params![id, error_code, error_message],
        )?;

        ensure_updated(id, changed)
    }
}

fn ensure_updated(id: &str, changed: usize) -> Result<()> {
    if changed == 0 {
        Err(StorageError::DownloadNotFound(id.to_owned()))
    } else {
        Ok(())
    }
}

fn u64_to_i64(value: u64, field: &'static str) -> Result<i64> {
    i64::try_from(value).map_err(|_| StorageError::IntegerTooLarge { field, value })
}

fn i64_to_u64(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| StorageError::NegativeInteger { field, value })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_complete_download_lifecycle() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let created = storage
            .create_download("https://example.com/file.zip", 1_000)
            .unwrap();

        assert_eq!(created.status, DownloadStatus::Created);
        assert_eq!(created.downloaded_bytes, 0);

        storage.mark_downloading(&created.id, 1_100).unwrap();

        storage
            .update_progress(&created.id, 512, Some(1_024))
            .unwrap();

        let active = storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(active.status, DownloadStatus::Downloading);
        assert_eq!(active.started_at, Some(1_100));
        assert_eq!(active.downloaded_bytes, 512);
        assert_eq!(active.total_bytes, Some(1_024));

        let completion = DownloadCompletion {
            resolved_url: "https://cdn.example.com/file.zip".to_owned(),
            filename: "file.zip".to_owned(),
            destination_path: "C:\\\\Downloads\\\\file.zip".to_owned(),
            mime_type: Some("application/zip".to_owned()),
            total_bytes: Some(1_024),
            downloaded_bytes: 1_024,
        };

        storage
            .mark_completed(&created.id, &completion, 1_200)
            .unwrap();

        let completed = storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(completed.status, DownloadStatus::Completed);
        assert_eq!(completed.downloaded_bytes, 1_024);
        assert_eq!(completed.total_bytes, Some(1_024));
        assert_eq!(completed.completed_at, Some(1_200));
        assert_eq!(completed.filename.as_deref(), Some("file.zip"));
        assert_eq!(
            completed.destination_path.as_deref(),
            Some("C:\\\\Downloads\\\\file.zip")
        );

        let downloads = storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
    }

    #[test]
    fn persists_failed_download() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let created = storage
            .create_download("https://example.com/broken.bin", 2_000)
            .unwrap();

        storage.mark_downloading(&created.id, 2_100).unwrap();

        storage
            .mark_failed(&created.id, "http_error", "server returned an error")
            .unwrap();

        let failed = storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(failed.status, DownloadStatus::Failed);
        assert_eq!(failed.error_code.as_deref(), Some("http_error"));
        assert_eq!(
            failed.error_message.as_deref(),
            Some("server returned an error")
        );
    }

    #[test]
    fn updating_unknown_download_returns_not_found() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let error = storage.mark_downloading("missing-id", 1_000).unwrap_err();

        assert!(matches!(error, StorageError::DownloadNotFound(_)));
    }
}
