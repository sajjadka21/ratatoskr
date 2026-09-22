use crate::{Result, Storage, StorageError};
use dm_common::{
    DownloadCompletion, DownloadPriority, DownloadRecord, DownloadStatus, TransferPlan,
};
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
    attempts,
    retry_at,
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
    attempts: i64,
    retry_at: Option<i64>,
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
            attempts: row.get(19)?,
            retry_at: row.get(20)?,
            error_code: row.get(21)?,
            error_message: row.get(22)?,
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
            attempts: u32::try_from(self.attempts).map_err(|_| StorageError::NegativeInteger {
                field: "attempts",
                value: self.attempts,
            })?,
            retry_at: self.retry_at,
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
            attempts: 0,
            retry_at: None,
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
    /// Partial bytes are kept: a row with a temp file and a byte count becomes
    /// paused or returns to its queue, and the engine decides on the next
    /// attempt whether those bytes are still valid for the remote content. A
    /// row with nothing on disk goes back to created.
    pub fn recover_orphaned_downloads(
        &self,
        error_code: &str,
        error_message: &str,
    ) -> Result<Vec<DownloadRecord>> {
        let orphaned = status_in_clause(
            &DownloadStatus::ALL
                .into_iter()
                .filter(|status| status.is_orphaned_by_restart())
                .collect::<Vec<_>>(),
        );

        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;

        let select = format!("SELECT id FROM downloads WHERE status IN ({orphaned});");
        let mut statement = transaction.prepare(&select)?;

        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        drop(statement);

        transaction.execute(
            &format!(
                r#"
                UPDATE downloads
                SET
                    status = CASE
                        WHEN queue_id IS NOT NULL THEN '{queued}'
                        WHEN temp_path IS NOT NULL AND downloaded_bytes > 0 THEN '{paused}'
                        ELSE '{created}'
                    END,
                    downloaded_bytes = CASE
                        WHEN temp_path IS NOT NULL THEN downloaded_bytes
                        ELSE 0
                    END,
                    error_code = ?1,
                    error_message = ?2
                WHERE status IN ({orphaned});
                "#,
                queued = DownloadStatus::restart_recovery_status(true, true).as_str(),
                paused = DownloadStatus::restart_recovery_status(false, true).as_str(),
                created = DownloadStatus::restart_recovery_status(false, false).as_str(),
            ),
            params![error_code, error_message],
        )?;

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

    /// Claims a task for a direct start, resume or retry.
    ///
    /// The canonical machine also allows `queued -> probing`, but that path
    /// belongs to the queue runner alone: a queued task must not be able to
    /// jump its queue by being started directly.
    pub fn mark_probing(&self, id: &str) -> Result<()> {
        let sources = DownloadStatus::sources_of(DownloadStatus::Probing)
            .into_iter()
            .filter(|status| *status != DownloadStatus::Queued)
            .collect::<Vec<_>>();

        let connection = self.connection()?;

        let changed = connection.execute(
            &format!(
                r#"
                UPDATE downloads
                SET
                    status = '{probing}',
                    retry_at = NULL,
                    error_code = NULL,
                    error_message = NULL
                WHERE id = ?1
                  AND status IN ({sources});
                "#,
                probing = DownloadStatus::Probing.as_str(),
                sources = status_in_clause(&sources),
            ),
            [id],
        )?;

        ensure_transitioned(&connection, id, changed, DownloadStatus::Probing)
    }

    pub fn claim_queued_download(&self, id: &str, queue_id: &str) -> Result<DownloadRecord> {
        let connection = self.connection()?;
        let changed = connection.execute(
            &format!(
                r#"
                UPDATE downloads
                SET
                    status = '{probing}',
                    retry_at = NULL
                WHERE id = ?1
                  AND queue_id = ?2
                  AND status = '{queued}';
                "#,
                probing = DownloadStatus::Probing.as_str(),
                queued = DownloadStatus::Queued.as_str(),
            ),
            params![id, queue_id],
        )?;
        ensure_transitioned(&connection, id, changed, DownloadStatus::Probing)?;
        drop(connection);

        self.get_download(id)?
            .ok_or_else(|| StorageError::DownloadNotFound(id.to_owned()))
    }

    /// Records what probing learned before a single byte is written, so a
    /// process that dies mid-transfer leaves behind enough to validate the
    /// partial file against the source on the next attempt.
    pub fn set_transfer_plan(&self, id: &str, plan: &TransferPlan) -> Result<()> {
        let total_bytes = plan
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
                temp_path = ?5,
                mime_type = COALESCE(?6, mime_type),
                total_bytes = ?7,
                etag = ?8,
                last_modified = ?9,
                range_supported = ?10
            WHERE id = ?1;
            "#,
            params![
                id,
                &plan.resolved_url,
                &plan.filename,
                &plan.destination_path,
                &plan.temp_path,
                &plan.mime_type,
                total_bytes,
                &plan.etag,
                &plan.last_modified,
                plan.range_supported,
            ],
        )?;

        ensure_updated(id, changed)
    }

    /// Starts the transfer. Any explanation left by probing - a restart from
    /// zero, for instance - is deliberately kept, because it describes the
    /// transfer that is about to run.
    pub fn mark_downloading(&self, id: &str, started_at: i64) -> Result<()> {
        self.transition(
            id,
            DownloadStatus::Downloading,
            "started_at = ?2",
            params![id, started_at],
        )
    }

    /// Persists a transfer that was stopped on purpose. The partial file is
    /// kept exactly as it is so the next attempt can continue from it.
    pub fn mark_paused(&self, id: &str, downloaded_bytes: u64) -> Result<()> {
        let downloaded_bytes = u64_to_i64(downloaded_bytes, "downloaded_bytes")?;

        self.transition(
            id,
            DownloadStatus::Paused,
            "downloaded_bytes = ?2, error_code = NULL, error_message = NULL",
            params![id, downloaded_bytes],
        )
    }

    /// Cancellation is terminal and discards the partial transfer, so the row
    /// no longer points at a file that has been deleted.
    pub fn mark_cancelled(&self, id: &str) -> Result<()> {
        self.transition(
            id,
            DownloadStatus::Cancelled,
            "downloaded_bytes = 0, temp_path = NULL, retry_at = NULL,
             error_code = NULL, error_message = NULL",
            params![id],
        )
    }

    /// Resets a reusable task to `created` without changing its identity or
    /// source URL. Transfer metadata and retry state are cleared so the next
    /// probe builds a fresh destination and validator snapshot.
    pub fn reset_for_restart(&self, id: &str) -> Result<()> {
        self.transition(
            id,
            DownloadStatus::Created,
            "downloaded_bytes = 0,
             resolved_url = NULL,
             filename = NULL,
             destination_path = NULL,
             temp_path = NULL,
             mime_type = NULL,
             total_bytes = NULL,
             etag = NULL,
             last_modified = NULL,
             range_supported = NULL,
             started_at = NULL,
             completed_at = NULL,
             retry_at = NULL,
             error_code = NULL,
             error_message = NULL,
             queue_position = NULL",
            params![id],
        )
    }

    /// Replaces the source URL and resets the transfer metadata in one guarded
    /// lifecycle transition. The caller validates the URL before reaching the
    /// storage layer; this method never concatenates user input into SQL.
    pub fn update_source_url(&self, id: &str, source_url: &str) -> Result<()> {
        self.transition(
            id,
            DownloadStatus::Created,
            "source_url = ?2,
             downloaded_bytes = 0,
             resolved_url = NULL,
             filename = NULL,
             destination_path = NULL,
             temp_path = NULL,
             mime_type = NULL,
             total_bytes = NULL,
             etag = NULL,
             last_modified = NULL,
             range_supported = NULL,
             started_at = NULL,
             completed_at = NULL,
             retry_at = NULL,
             error_code = NULL,
             error_message = NULL,
             queue_position = NULL",
            params![id, source_url],
        )
    }

    /// Schedules an automatic retry. `attempts` is authoritative in the row so
    /// the budget survives a restart.
    pub fn mark_retrying(
        &self,
        id: &str,
        attempts: u32,
        retry_at: i64,
        error_code: &str,
        error_message: &str,
    ) -> Result<()> {
        self.transition(
            id,
            DownloadStatus::Retrying,
            "attempts = ?2, retry_at = ?3, error_code = ?4, error_message = ?5",
            params![id, attempts, retry_at, error_code, error_message],
        )
    }

    /// Retrying tasks whose backoff has elapsed, oldest first.
    pub fn list_due_retries(&self, now: i64) -> Result<Vec<DownloadRecord>> {
        let connection = self.connection()?;
        let sql = format!(
            "{DOWNLOAD_SELECT}
             WHERE status = '{retrying}'
               AND retry_at IS NOT NULL
               AND retry_at <= ?1
             ORDER BY retry_at ASC, id ASC",
            retrying = DownloadStatus::Retrying.as_str(),
        );

        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([now], StoredDownloadRow::from_row)?;
        let mut downloads = Vec::new();

        for row in rows {
            downloads.push(row?.into_record()?);
        }

        Ok(downloads)
    }

    /// Clears a partial transfer that can no longer be trusted, so the next
    /// attempt starts from zero instead of appending to stale bytes.
    pub fn reset_transfer_progress(&self, id: &str) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            "UPDATE downloads SET downloaded_bytes = 0 WHERE id = ?1;",
            [id],
        )?;

        ensure_updated(id, changed)
    }

    /// Records an explanation on a row without changing its status.
    ///
    /// The `error_*` columns are the row's explanation, not strictly its
    /// failure: a task that had to restart from zero, or one interrupted by a
    /// restart, uses them to say so while still being perfectly healthy. The
    /// next successful transition clears them.
    pub fn record_notice(&self, id: &str, code: &str, message: &str) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            "UPDATE downloads SET error_code = ?2, error_message = ?3 WHERE id = ?1;",
            params![id, code, message],
        )?;

        ensure_updated(id, changed)
    }

    pub fn record_attempt(&self, id: &str, attempts: u32) -> Result<()> {
        let connection = self.connection()?;

        let changed = connection.execute(
            "UPDATE downloads SET attempts = ?2 WHERE id = ?1;",
            params![id, attempts],
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

        self.transition(
            id,
            DownloadStatus::Completed,
            "resolved_url = ?2, filename = ?3, destination_path = ?4, temp_path = NULL,
             mime_type = ?5, total_bytes = ?6, downloaded_bytes = ?7, completed_at = ?8,
             retry_at = NULL, error_code = NULL, error_message = NULL",
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
        )
    }

    pub fn mark_finalizing(&self, id: &str) -> Result<()> {
        self.transition(id, DownloadStatus::Finalizing, "", params![id])
    }

    pub fn mark_failed(&self, id: &str, error_code: &str, error_message: &str) -> Result<()> {
        self.transition(
            id,
            DownloadStatus::Failed,
            "retry_at = NULL, error_code = ?2, error_message = ?3",
            params![id, error_code, error_message],
        )
    }

    /// Applies a status change whose legality comes from the canonical state
    /// machine rather than from a status list written into each statement.
    fn transition(
        &self,
        id: &str,
        to: DownloadStatus,
        assignments: &str,
        parameters: &[&dyn rusqlite::ToSql],
    ) -> Result<()> {
        let assignments = if assignments.trim().is_empty() {
            String::new()
        } else {
            format!(", {assignments}")
        };

        let connection = self.connection()?;

        let changed = connection.execute(
            &format!(
                r#"
                UPDATE downloads
                SET status = '{to}'{assignments}
                WHERE id = ?1
                  AND status IN ({sources});
                "#,
                to = to.as_str(),
                sources = status_in_clause(&DownloadStatus::sources_of(to)),
            ),
            parameters,
        )?;

        ensure_transitioned(&connection, id, changed, to)
    }
}

/// Renders a canonical status group as SQL literals. The values come from a
/// closed enum of lowercase identifiers, never from user input.
pub(crate) fn status_in_clause(statuses: &[DownloadStatus]) -> String {
    statuses
        .iter()
        .map(|status| format!("'{}'", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
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

    fn sample_plan(temp_path: &str) -> TransferPlan {
        TransferPlan {
            resolved_url: "https://cdn.example.com/file.bin".to_owned(),
            filename: "file.bin".to_owned(),
            destination_path: "C:\\Downloads\\file.bin".to_owned(),
            temp_path: temp_path.to_owned(),
            mime_type: Some("application/octet-stream".to_owned()),
            total_bytes: Some(8_192),
            etag: Some("\"v1\"".to_owned()),
            last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_owned()),
            range_supported: true,
        }
    }

    #[test]
    fn recovers_a_partial_transfer_as_resumable() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/direct.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage
            .set_transfer_plan(&record.id, &sample_plan("C:\\Downloads\\file.bin.part"))
            .unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage
            .update_progress(&record.id, 4_096, Some(8_192))
            .unwrap();

        let recovered = storage
            .recover_orphaned_downloads("interrupted", "interrupted by a restart")
            .unwrap();

        assert_eq!(recovered.len(), 1);

        let record = storage.get_download(&record.id).unwrap().unwrap();

        assert_eq!(record.status, DownloadStatus::Paused);
        assert_eq!(
            record.downloaded_bytes, 4_096,
            "partial bytes must survive a restart"
        );
        assert_eq!(record.etag.as_deref(), Some("\"v1\""));
        assert_eq!(
            record.temp_path.as_deref(),
            Some("C:\\Downloads\\file.bin.part")
        );
        assert_eq!(record.error_code.as_deref(), Some("interrupted"));
    }

    #[test]
    fn recovers_orphaned_downloads_to_actionable_states() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        // Interrupted before probing wrote a plan, so there is nothing on disk
        // to resume from.
        let direct = storage
            .create_download("https://example.com/direct.bin", 1_000)
            .unwrap();
        storage.mark_probing(&direct.id).unwrap();

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
        assert_eq!(direct.error_code.as_deref(), Some("interrupted"));

        let queued = storage.get_download(&queued.id).unwrap().unwrap();
        assert_eq!(
            queued.status,
            DownloadStatus::Queued,
            "queued work returns to its queue rather than to the user"
        );
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
    fn pauses_and_resumes_without_losing_partial_bytes() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage
            .set_transfer_plan(&record.id, &sample_plan("C:\\Downloads\\file.bin.part"))
            .unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage.mark_paused(&record.id, 2_048).unwrap();

        let paused = storage.get_download(&record.id).unwrap().unwrap();
        assert_eq!(paused.status, DownloadStatus::Paused);
        assert_eq!(paused.downloaded_bytes, 2_048);
        assert_eq!(paused.range_supported, Some(true));

        storage.mark_probing(&record.id).unwrap();

        let resumed = storage.get_download(&record.id).unwrap().unwrap();
        assert_eq!(resumed.status, DownloadStatus::Probing);
        assert_eq!(
            resumed.downloaded_bytes, 2_048,
            "resuming must not discard what was already transferred"
        );
    }

    #[test]
    fn cancelling_discards_the_partial_transfer() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage
            .set_transfer_plan(&record.id, &sample_plan("C:\\Downloads\\file.bin.part"))
            .unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage.update_progress(&record.id, 900, None).unwrap();
        storage.mark_cancelled(&record.id).unwrap();

        let cancelled = storage.get_download(&record.id).unwrap().unwrap();

        assert_eq!(cancelled.status, DownloadStatus::Cancelled);
        assert_eq!(cancelled.downloaded_bytes, 0);
        assert!(cancelled.temp_path.is_none());
    }

    #[test]
    fn a_completed_download_cannot_be_reopened() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage.mark_finalizing(&record.id).unwrap();
        storage
            .mark_completed(
                &record.id,
                &DownloadCompletion {
                    resolved_url: "https://example.com/file.bin".to_owned(),
                    filename: "file.bin".to_owned(),
                    destination_path: "C:\\Downloads\\file.bin".to_owned(),
                    mime_type: None,
                    total_bytes: Some(4),
                    downloaded_bytes: 4,
                },
                1_200,
            )
            .unwrap();

        let error = storage.mark_probing(&record.id).unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidStatusTransition {
                from: DownloadStatus::Completed,
                to: DownloadStatus::Probing,
                ..
            }
        ));
    }

    #[test]
    fn restart_from_zero_clears_transfer_metadata_but_keeps_identity() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage
            .set_transfer_plan(&record.id, &sample_plan("C:\\Downloads\\file.bin.part"))
            .unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage
            .mark_failed(&record.id, "network", "connection reset")
            .unwrap();

        storage.reset_for_restart(&record.id).unwrap();
        let restarted = storage.get_download(&record.id).unwrap().unwrap();

        assert_eq!(restarted.id, record.id);
        assert_eq!(restarted.status, DownloadStatus::Created);
        assert_eq!(restarted.downloaded_bytes, 0);
        assert!(restarted.destination_path.is_none());
        assert!(restarted.total_bytes.is_none());
        assert!(restarted.completed_at.is_none());
        assert_eq!(restarted.source_url, record.source_url);
    }

    #[test]
    fn source_refresh_reuses_identity_and_resets_transfer_state() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let record = storage
            .create_download("https://example.com/expired.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage
            .mark_failed(&record.id, "http_404", "not found")
            .unwrap();

        storage
            .update_source_url(&record.id, "https://cdn.example.com/fresh.bin")
            .unwrap();
        let refreshed = storage.get_download(&record.id).unwrap().unwrap();

        assert_eq!(refreshed.id, record.id);
        assert_eq!(refreshed.source_url, "https://cdn.example.com/fresh.bin");
        assert_eq!(refreshed.status, DownloadStatus::Created);
        assert!(refreshed.error_code.is_none());
    }

    #[test]
    fn a_failed_download_can_be_retried_with_the_same_identity() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage
            .mark_failed(&record.id, "download_error", "HTTP request failed")
            .unwrap();

        storage.mark_probing(&record.id).unwrap();

        let retried = storage.get_download(&record.id).unwrap().unwrap();

        assert_eq!(retried.id, record.id);
        assert_eq!(retried.status, DownloadStatus::Probing);
        assert!(retried.error_message.is_none());
    }

    #[test]
    fn due_retries_are_listed_only_once_their_backoff_elapsed() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let record = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage.mark_probing(&record.id).unwrap();
        storage.mark_downloading(&record.id, 1_050).unwrap();
        storage
            .mark_retrying(&record.id, 1, 2_000, "network", "connection reset")
            .unwrap();

        assert!(storage.list_due_retries(1_999).unwrap().is_empty());

        let due = storage.list_due_retries(2_000).unwrap();

        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, record.id);
        assert_eq!(due[0].attempts, 1);
        assert_eq!(due[0].retry_at, Some(2_000));
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
