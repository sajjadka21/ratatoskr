use crate::{Result, Storage, StorageError};
use dm_common::{DownloadCompletion, DownloadPriority, DownloadRecord, DownloadStatus};
use rusqlite::{Connection, OptionalExtension, Row, params};
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
    queue_id,
    priority,
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
    queue_id: Option<String>,
    priority: String,
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
            queue_id: row.get(13)?,
            priority: row.get(14)?,
            queue_position: row.get(15)?,
            created_at: row.get(16)?,
            started_at: row.get(17)?,
            completed_at: row.get(18)?,
            error_code: row.get(19)?,
            error_message: row.get(20)?,
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
        let priority = DownloadPriority::from_str(&self.priority)
            .map_err(|_| StorageError::InvalidDownloadPriority(self.priority))?;

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
            queue_id: self.queue_id,
            priority,
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
            queue_id: None,
            priority: DownloadPriority::Normal,
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

    pub fn list_queued_downloads(&self, queue_id: &str) -> Result<Vec<DownloadRecord>> {
        let connection = self.connection()?;
        let sql = format!(
            "{DOWNLOAD_SELECT}
             WHERE queue_id = ?1 AND status = 'queued'
             ORDER BY
                CASE priority
                    WHEN 'very_high' THEN 0
                    WHEN 'high' THEN 1
                    WHEN 'normal' THEN 2
                    WHEN 'low' THEN 3
                END,
                queue_position ASC,
                id ASC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([queue_id], StoredDownloadRow::from_row)?;
        let mut downloads = Vec::new();

        for row in rows {
            downloads.push(row?.into_record()?);
        }

        Ok(downloads)
    }

    /// Deletes a history row, but only while no executor can still be writing
    /// to it. The status guard lives in the `DELETE` itself so a queue runner
    /// that claims the task at the same moment wins the race instead of
    /// losing its row mid-transfer.
    pub fn remove_download_record(&self, id: &str) -> Result<()> {
        let connection = self.connection()?;
        let removable = status_list(DownloadStatus::is_removable);

        let changed = connection.execute(
            &format!(
                "DELETE FROM downloads WHERE id = ?1 AND status IN ({});",
                sql_placeholders(removable.len(), 2)
            ),
            rusqlite::params_from_iter(std::iter::once(id).chain(removable.iter().copied())),
        )?;

        if changed > 0 {
            return Ok(());
        }

        let status = current_status(&connection, id)?;

        Err(StorageError::DownloadNotRemovable {
            id: id.to_owned(),
            status,
        })
    }

    /// Returns rows that a previous process left mid-transfer to a state the
    /// user or a queue runner can act on again, and reports what was changed.
    ///
    /// Progress bytes are cleared because the current engine restarts an
    /// interrupted transfer from zero; Phase 3 replaces this with a real
    /// resume that keeps the partial bytes and their validators.
    pub fn recover_orphaned_downloads(
        &self,
        error_code: &str,
        error_message: &str,
    ) -> Result<Vec<DownloadRecord>> {
        let orphaned = status_list(DownloadStatus::is_orphaned_by_restart);
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;

        let select = format!(
            "SELECT id FROM downloads WHERE status IN ({});",
            sql_placeholders(orphaned.len(), 1)
        );

        let mut statement = transaction.prepare(&select)?;

        let ids = statement
            .query_map(
                rusqlite::params_from_iter(orphaned.iter().copied()),
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        drop(statement);

        for id in &ids {
            transaction.execute(
                r#"
                UPDATE downloads
                SET
                    status = CASE WHEN queue_id IS NULL THEN ?2 ELSE ?3 END,
                    downloaded_bytes = 0,
                    temp_path = NULL,
                    started_at = NULL,
                    error_code = ?4,
                    error_message = ?5
                WHERE id = ?1;
                "#,
                params![
                    id,
                    DownloadStatus::restart_recovery_status(false).as_str(),
                    DownloadStatus::restart_recovery_status(true).as_str(),
                    error_code,
                    error_message,
                ],
            )?;
        }

        transaction.commit()?;
        drop(connection);

        let mut recovered = Vec::with_capacity(ids.len());

        for id in ids {
            if let Some(record) = self.get_download(&id)? {
                recovered.push(record);
            }
        }

        Ok(recovered)
    }

    pub fn mark_probing(&self, id: &str) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET
                status = 'probing',
                error_code = NULL,
                error_message = NULL
            WHERE id = ?1
              AND status = 'created';
            "#,
            [id],
        )?;

        ensure_transitioned(&connection, id, changed, DownloadStatus::Probing)
    }

    pub fn claim_queued_download(&self, id: &str, queue_id: &str) -> Result<DownloadRecord> {
        let connection = self.connection()?;
        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET status = 'probing'
            WHERE id = ?1
              AND queue_id = ?2
              AND status = 'queued';
            "#,
            params![id, queue_id],
        )?;
        ensure_transitioned(&connection, id, changed, DownloadStatus::Probing)?;
        drop(connection);

        self.get_download(id)?
            .ok_or_else(|| StorageError::DownloadNotFound(id.to_owned()))
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
            WHERE id = ?1
              AND status = 'probing';
            "#,
            params![id, started_at],
        )?;

        ensure_transitioned(&connection, id, changed, DownloadStatus::Downloading)
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
            WHERE id = ?1
              AND status = 'finalizing';
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

        ensure_transitioned(&connection, id, changed, DownloadStatus::Completed)
    }

    pub fn mark_finalizing(&self, id: &str) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            r#"
            UPDATE downloads
            SET status = 'finalizing'
            WHERE id = ?1
              AND status = 'downloading';
            "#,
            [id],
        )?;

        ensure_transitioned(&connection, id, changed, DownloadStatus::Finalizing)
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
            WHERE id = ?1
              AND status IN ('probing', 'downloading', 'finalizing');
            "#,
            params![id, error_code, error_message],
        )?;

        ensure_transitioned(&connection, id, changed, DownloadStatus::Failed)
    }
}

/// Builds the canonical status group matching `predicate` as SQL literals, so
/// status groups are derived from `dm-common` instead of restated in SQL.
fn status_list(predicate: fn(DownloadStatus) -> bool) -> Vec<&'static str> {
    DownloadStatus::ALL
        .into_iter()
        .filter(|status| predicate(*status))
        .map(DownloadStatus::as_str)
        .collect()
}

fn sql_placeholders(count: usize, first_index: usize) -> String {
    (0..count)
        .map(|offset| format!("?{}", first_index + offset))
        .collect::<Vec<_>>()
        .join(", ")
}

fn current_status(connection: &Connection, id: &str) -> Result<DownloadStatus> {
    let status = connection
        .query_row("SELECT status FROM downloads WHERE id = ?1;", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .ok_or_else(|| StorageError::DownloadNotFound(id.to_owned()))?;

    DownloadStatus::from_str(&status).map_err(|_| StorageError::InvalidDownloadStatus(status))
}

fn ensure_updated(id: &str, changed: usize) -> Result<()> {
    if changed == 0 {
        Err(StorageError::DownloadNotFound(id.to_owned()))
    } else {
        Ok(())
    }
}

fn ensure_transitioned(
    connection: &Connection,
    id: &str,
    changed: usize,
    to: DownloadStatus,
) -> Result<()> {
    if changed > 0 {
        return Ok(());
    }

    let status = connection
        .query_row("SELECT status FROM downloads WHERE id = ?1;", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;

    let Some(status) = status else {
        return Err(StorageError::DownloadNotFound(id.to_owned()));
    };

    let from = DownloadStatus::from_str(&status)
        .map_err(|_| StorageError::InvalidDownloadStatus(status))?;

    Err(StorageError::InvalidStatusTransition {
        id: id.to_owned(),
        from,
        to,
    })
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

        storage.mark_probing(&created.id).unwrap();

        let probing = storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(probing.status, DownloadStatus::Probing);

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

        storage.mark_finalizing(&created.id).unwrap();
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
    fn recovers_orphaned_downloads_to_actionable_states() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let direct = storage
            .create_download("https://example.com/direct.bin", 1_000)
            .unwrap();
        storage.mark_probing(&direct.id).unwrap();
        storage.mark_downloading(&direct.id, 1_050).unwrap();
        storage
            .update_progress(&direct.id, 4_096, Some(8_192))
            .unwrap();

        let queued = storage
            .create_download("https://example.com/queued.bin", 1_001)
            .unwrap();
        storage
            .enqueue_download(&queued.id, "default", None)
            .unwrap();
        storage
            .claim_queued_download(&queued.id, "default")
            .unwrap();

        let untouched = storage
            .create_download("https://example.com/idle.bin", 1_002)
            .unwrap();

        let recovered = storage
            .recover_orphaned_downloads("interrupted", "interrupted by a restart")
            .unwrap();

        assert_eq!(recovered.len(), 2);

        let direct = storage.get_download(&direct.id).unwrap().unwrap();
        assert_eq!(direct.status, DownloadStatus::Created);
        assert_eq!(direct.downloaded_bytes, 0);
        assert!(direct.started_at.is_none());
        assert_eq!(direct.error_code.as_deref(), Some("interrupted"));

        let queued = storage.get_download(&queued.id).unwrap().unwrap();
        assert_eq!(queued.status, DownloadStatus::Queued);
        assert_eq!(queued.queue_id.as_deref(), Some("default"));

        let untouched = storage.get_download(&untouched.id).unwrap().unwrap();
        assert_eq!(untouched.status, DownloadStatus::Created);
        assert!(untouched.error_code.is_none());
    }

    #[test]
    fn recovery_is_a_no_op_when_nothing_was_interrupted() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();

        let recovered = storage
            .recover_orphaned_downloads("interrupted", "interrupted by a restart")
            .unwrap();

        assert!(recovered.is_empty());
    }

    #[test]
    fn removes_tasks_that_no_executor_owns() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let created = storage
            .create_download("https://example.com/created.bin", 1_000)
            .unwrap();
        let queued = storage
            .create_download("https://example.com/queued.bin", 1_001)
            .unwrap();
        storage
            .enqueue_download(&queued.id, "default", None)
            .unwrap();

        storage.remove_download_record(&created.id).unwrap();
        storage.remove_download_record(&queued.id).unwrap();

        assert!(storage.list_downloads().unwrap().is_empty());
    }

    #[test]
    fn refuses_to_remove_a_task_while_it_is_being_transferred() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();

        let error = storage.remove_download_record(&record.id).unwrap_err();

        assert!(matches!(
            error,
            StorageError::DownloadNotRemovable {
                status: DownloadStatus::Downloading,
                ..
            }
        ));
        assert_eq!(storage.list_downloads().unwrap().len(), 1);
    }

    #[test]
    fn persists_failed_download() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let created = storage
            .create_download("https://example.com/broken.bin", 2_000)
            .unwrap();

        storage.mark_probing(&created.id).unwrap();
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
    fn removes_download_record_without_deleting_file() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let destination_path = directory.path().join("file.zip");

        std::fs::write(&destination_path, b"downloaded file").unwrap();

        let storage = Storage::open(&database_path).unwrap();

        let created = storage
            .create_download("https://example.com/file.zip", 3_000)
            .unwrap();

        storage.mark_probing(&created.id).unwrap();
        storage.mark_downloading(&created.id, 3_100).unwrap();

        let completion = DownloadCompletion {
            resolved_url: "https://example.com/file.zip".to_owned(),
            filename: "file.zip".to_owned(),
            destination_path: destination_path.to_string_lossy().into_owned(),
            mime_type: Some("application/zip".to_owned()),
            total_bytes: Some(15),
            downloaded_bytes: 15,
        };

        storage.mark_finalizing(&created.id).unwrap();
        storage
            .mark_completed(&created.id, &completion, 3_200)
            .unwrap();

        assert!(destination_path.exists());
        assert!(storage.get_download(&created.id).unwrap().is_some());

        storage.remove_download_record(&created.id).unwrap();

        assert!(storage.get_download(&created.id).unwrap().is_none());

        assert!(destination_path.exists());
        assert!(storage.list_downloads().unwrap().is_empty());
    }

    #[test]
    fn removing_unknown_download_returns_not_found() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let error = storage.remove_download_record("missing-id").unwrap_err();

        assert!(matches!(error, StorageError::DownloadNotFound(_)));
    }
    #[test]
    fn updating_unknown_download_returns_not_found() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let error = storage.mark_downloading("missing-id", 1_000).unwrap_err();

        assert!(matches!(error, StorageError::DownloadNotFound(_)));
    }

    #[test]
    fn rejects_duplicate_start_claim_without_creating_another_record() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let created = storage
            .create_download("https://example.com/file.zip", 4_000)
            .unwrap();

        storage.mark_probing(&created.id).unwrap();

        let error = storage.mark_probing(&created.id).unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidStatusTransition {
                from: DownloadStatus::Probing,
                to: DownloadStatus::Probing,
                ..
            }
        ));

        let downloads = storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
        assert_eq!(downloads[0].status, DownloadStatus::Probing);
    }

    #[test]
    fn rejects_skipping_the_probing_state() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();

        let created = storage
            .create_download("https://example.com/file.zip", 5_000)
            .unwrap();

        let error = storage.mark_downloading(&created.id, 5_100).unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidStatusTransition {
                from: DownloadStatus::Created,
                to: DownloadStatus::Downloading,
                ..
            }
        ));
    }

    #[test]
    fn queued_download_can_only_be_claimed_from_its_queue_once() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let storage = Storage::open(&database_path).unwrap();
        let created = storage
            .create_download("https://example.com/queued.bin", 6_000)
            .unwrap();
        storage
            .enqueue_download(&created.id, "default", None)
            .unwrap();

        let wrong_queue = storage
            .claim_queued_download(&created.id, "missing")
            .unwrap_err();
        assert!(matches!(
            wrong_queue,
            StorageError::InvalidStatusTransition {
                from: DownloadStatus::Queued,
                to: DownloadStatus::Probing,
                ..
            }
        ));

        let claimed = storage
            .claim_queued_download(&created.id, "default")
            .unwrap();
        assert_eq!(claimed.status, DownloadStatus::Probing);
        assert_eq!(claimed.id, created.id);

        let duplicate = storage
            .claim_queued_download(&created.id, "default")
            .unwrap_err();
        assert!(matches!(
            duplicate,
            StorageError::InvalidStatusTransition {
                from: DownloadStatus::Probing,
                to: DownloadStatus::Probing,
                ..
            }
        ));
        assert_eq!(storage.list_downloads().unwrap().len(), 1);
    }
}
