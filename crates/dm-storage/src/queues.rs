use crate::{Result, Storage, StorageError};
use dm_common::{DownloadPriority, QueueRecord, QueueState};
use rusqlite::{OptionalExtension, Row, params};
use std::{collections::HashSet, str::FromStr};
use uuid::Uuid;

impl Storage {
    pub fn get_queue(&self, id: &str) -> Result<Option<QueueRecord>> {
        let connection = self.connection()?;
        let stored = connection
            .query_row(
                r#"
                SELECT
                    id, name, enabled, state, sort_order, max_concurrent,
                    max_concurrent_per_host, default_priority, created_at, updated_at
                FROM queues
                WHERE id = ?1;
                "#,
                [id],
                StoredQueueRow::from_row,
            )
            .optional()?;

        stored.map(StoredQueueRow::into_record).transpose()
    }

    pub fn list_queues(&self) -> Result<Vec<QueueRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT
                id,
                name,
                enabled,
                state,
                sort_order,
                max_concurrent,
                max_concurrent_per_host,
                default_priority,
                created_at,
                updated_at
            FROM queues
            ORDER BY sort_order ASC, name ASC;
            "#,
        )?;

        let rows = statement.query_map([], StoredQueueRow::from_row)?;
        let mut queues = Vec::new();

        for row in rows {
            queues.push(row?.into_record()?);
        }

        Ok(queues)
    }

    pub fn create_queue(
        &self,
        name: &str,
        max_concurrent: u32,
        max_concurrent_per_host: Option<u32>,
        default_priority: DownloadPriority,
        created_at: i64,
    ) -> Result<QueueRecord> {
        let name = name.trim();

        if name.is_empty() {
            return Err(StorageError::InvalidQueueConfiguration(
                "queue name must not be empty".to_owned(),
            ));
        }

        if max_concurrent == 0 || max_concurrent_per_host == Some(0) {
            return Err(StorageError::InvalidQueueConfiguration(
                "queue concurrency must be greater than zero".to_owned(),
            ));
        }

        let max_concurrent = i64::from(max_concurrent);
        let max_concurrent_per_host = max_concurrent_per_host.map(i64::from);
        let id = Uuid::new_v4().to_string();
        let connection = self.connection()?;
        let sort_order: i64 = connection.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM queues;",
            [],
            |row| row.get(0),
        )?;

        connection.execute(
            r#"
            INSERT INTO queues (
                id, name, enabled, state, sort_order, max_concurrent,
                max_concurrent_per_host, default_priority, created_at, updated_at
            )
            VALUES (?1, ?2, 1, 'stopped', ?3, ?4, ?5, ?6, ?7, ?7);
            "#,
            params![
                id,
                name,
                sort_order,
                max_concurrent,
                max_concurrent_per_host,
                default_priority.as_str(),
                created_at,
            ],
        )?;
        drop(connection);

        self.get_queue(&id)?
            .ok_or_else(|| StorageError::QueueNotFound(id))
    }

    pub fn set_queue_state(
        &self,
        id: &str,
        state: QueueState,
        updated_at: i64,
    ) -> Result<QueueRecord> {
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE queues SET state = ?2, updated_at = ?3 WHERE id = ?1;",
            params![id, state.as_str(), updated_at],
        )?;
        drop(connection);

        if changed == 0 {
            return Err(StorageError::QueueNotFound(id.to_owned()));
        }

        self.get_queue(id)?
            .ok_or_else(|| StorageError::QueueNotFound(id.to_owned()))
    }

    /// Turns a queue on or off as a configuration switch, independently of
    /// whether it is currently started. Disabling also stops it, so a disabled
    /// queue can never be left with a runner scheduling work.
    pub fn set_queue_enabled(
        &self,
        id: &str,
        enabled: bool,
        updated_at: i64,
    ) -> Result<QueueRecord> {
        let connection = self.connection()?;
        let changed = connection.execute(
            r#"
            UPDATE queues
            SET
                enabled = ?2,
                state = CASE WHEN ?2 = 0 THEN 'stopped' ELSE state END,
                updated_at = ?3
            WHERE id = ?1;
            "#,
            params![id, enabled, updated_at],
        )?;
        drop(connection);

        if changed == 0 {
            return Err(StorageError::QueueNotFound(id.to_owned()));
        }

        self.get_queue(id)?
            .ok_or_else(|| StorageError::QueueNotFound(id.to_owned()))
    }

    pub fn enqueue_download(
        &self,
        download_id: &str,
        queue_id: &str,
        priority: Option<DownloadPriority>,
    ) -> Result<dm_common::DownloadRecord> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let default_priority = queue_default_priority(&transaction, queue_id)?;
        let priority = priority.unwrap_or(default_priority);
        let position: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(queue_position), -1) + 1 FROM downloads WHERE queue_id = ?1 AND status = 'queued';",
            [queue_id],
            |row| row.get(0),
        )?;
        let changed = transaction.execute(
            r#"
            UPDATE downloads
            SET status = 'queued', queue_id = ?2, priority = ?3, queue_position = ?4
            WHERE id = ?1 AND status = 'created';
            "#,
            params![download_id, queue_id, priority.as_str(), position],
        )?;

        ensure_download_transition(
            &transaction,
            download_id,
            changed,
            dm_common::DownloadStatus::Queued,
        )?;
        transaction.commit()?;
        drop(connection);
        required_download(self, download_id)
    }

    pub fn move_queued_download(
        &self,
        download_id: &str,
        queue_id: &str,
    ) -> Result<dm_common::DownloadRecord> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        queue_default_priority(&transaction, queue_id)?;
        let position: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(queue_position), -1) + 1 FROM downloads WHERE queue_id = ?1 AND status = 'queued';",
            [queue_id],
            |row| row.get(0),
        )?;
        let changed = transaction.execute(
            "UPDATE downloads SET queue_id = ?2, queue_position = ?3 WHERE id = ?1 AND status = 'queued';",
            params![download_id, queue_id, position],
        )?;

        if changed == 0 {
            ensure_download_exists(&transaction, download_id)?;
            return Err(StorageError::InvalidStatusTransition {
                id: download_id.to_owned(),
                from: current_download_status(&transaction, download_id)?,
                to: dm_common::DownloadStatus::Queued,
            });
        }

        transaction.commit()?;
        drop(connection);
        required_download(self, download_id)
    }

    pub fn remove_download_from_queue(
        &self,
        download_id: &str,
    ) -> Result<dm_common::DownloadRecord> {
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE downloads SET status = 'created', queue_id = NULL, queue_position = NULL WHERE id = ?1 AND status = 'queued';",
            [download_id],
        )?;
        ensure_download_transition(
            &connection,
            download_id,
            changed,
            dm_common::DownloadStatus::Created,
        )?;
        drop(connection);
        required_download(self, download_id)
    }

    pub fn set_download_priority(
        &self,
        download_id: &str,
        priority: DownloadPriority,
    ) -> Result<dm_common::DownloadRecord> {
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE downloads SET priority = ?2 WHERE id = ?1 AND status IN ('created', 'queued');",
            params![download_id, priority.as_str()],
        )?;

        if changed == 0 {
            ensure_download_exists(&connection, download_id)?;
            return Err(StorageError::InvalidStatusTransition {
                id: download_id.to_owned(),
                from: current_download_status(&connection, download_id)?,
                to: current_download_status(&connection, download_id)?,
            });
        }

        drop(connection);
        required_download(self, download_id)
    }

    pub fn reorder_queue_downloads(&self, queue_id: &str, ordered_ids: &[String]) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        queue_default_priority(&transaction, queue_id)?;
        let mut statement = transaction
            .prepare("SELECT id FROM downloads WHERE queue_id = ?1 AND status = 'queued';")?;
        let existing = statement
            .query_map([queue_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<HashSet<_>, _>>()?;
        drop(statement);
        let supplied: HashSet<_> = ordered_ids.iter().cloned().collect();

        if existing != supplied || supplied.len() != ordered_ids.len() {
            return Err(StorageError::QueueOrderMismatch(queue_id.to_owned()));
        }

        for (position, id) in ordered_ids.iter().enumerate() {
            let position = i64::try_from(position).map_err(|_| {
                StorageError::InvalidQueueConfiguration("queue contains too many tasks".to_owned())
            })?;
            transaction.execute(
                "UPDATE downloads SET queue_position = ?2 WHERE id = ?1;",
                params![id, position],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }
}

fn queue_default_priority(
    connection: &rusqlite::Connection,
    queue_id: &str,
) -> Result<DownloadPriority> {
    let value = connection
        .query_row(
            "SELECT default_priority FROM queues WHERE id = ?1 AND enabled = 1;",
            [queue_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| StorageError::QueueNotFound(queue_id.to_owned()))?;

    DownloadPriority::from_str(&value).map_err(|_| StorageError::InvalidDownloadPriority(value))
}

fn required_download(storage: &Storage, id: &str) -> Result<dm_common::DownloadRecord> {
    storage
        .get_download(id)?
        .ok_or_else(|| StorageError::DownloadNotFound(id.to_owned()))
}

fn ensure_download_exists(connection: &rusqlite::Connection, id: &str) -> Result<()> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM downloads WHERE id = ?1);",
        [id],
        |row| row.get::<_, bool>(0),
    )?;

    if exists {
        Ok(())
    } else {
        Err(StorageError::DownloadNotFound(id.to_owned()))
    }
}

fn current_download_status(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<dm_common::DownloadStatus> {
    let value: String =
        connection.query_row("SELECT status FROM downloads WHERE id = ?1;", [id], |row| {
            row.get(0)
        })?;

    dm_common::DownloadStatus::from_str(&value)
        .map_err(|_| StorageError::InvalidDownloadStatus(value))
}

fn ensure_download_transition(
    connection: &rusqlite::Connection,
    id: &str,
    changed: usize,
    to: dm_common::DownloadStatus,
) -> Result<()> {
    if changed > 0 {
        return Ok(());
    }

    ensure_download_exists(connection, id)?;
    Err(StorageError::InvalidStatusTransition {
        id: id.to_owned(),
        from: current_download_status(connection, id)?,
        to,
    })
}

struct StoredQueueRow {
    id: String,
    name: String,
    enabled: i64,
    state: String,
    sort_order: i64,
    max_concurrent: i64,
    max_concurrent_per_host: Option<i64>,
    default_priority: String,
    created_at: i64,
    updated_at: i64,
}

impl StoredQueueRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            enabled: row.get(2)?,
            state: row.get(3)?,
            sort_order: row.get(4)?,
            max_concurrent: row.get(5)?,
            max_concurrent_per_host: row.get(6)?,
            default_priority: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }

    fn into_record(self) -> Result<QueueRecord> {
        let state = QueueState::from_str(&self.state)
            .map_err(|_| StorageError::InvalidQueueState(self.state))?;
        let default_priority = DownloadPriority::from_str(&self.default_priority)
            .map_err(|_| StorageError::InvalidDownloadPriority(self.default_priority))?;

        Ok(QueueRecord {
            id: self.id,
            name: self.name,
            enabled: self.enabled != 0,
            state,
            sort_order: self.sort_order,
            max_concurrent: u32::try_from(self.max_concurrent)
                .map_err(|_| StorageError::InvalidQueueConcurrency(self.max_concurrent))?,
            max_concurrent_per_host: self
                .max_concurrent_per_host
                .map(|value| {
                    u32::try_from(value).map_err(|_| StorageError::InvalidQueueConcurrency(value))
                })
                .transpose()?,
            default_priority,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dm_common::{DownloadPriority, DownloadStatus, QueueState};
    use tempfile::tempdir;

    #[test]
    fn creates_named_queue_and_persists_runtime_limits() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let queue = storage
            .create_queue("Large Files", 2, Some(1), DownloadPriority::High, 1_000)
            .unwrap();

        assert_eq!(queue.name, "Large Files");
        assert_eq!(queue.state, QueueState::Stopped);
        assert_eq!(queue.max_concurrent, 2);
        assert_eq!(queue.max_concurrent_per_host, Some(1));
        assert_eq!(queue.default_priority, DownloadPriority::High);
        assert_eq!(storage.list_queues().unwrap().len(), 2);
    }

    #[test]
    fn enqueues_tasks_with_default_priority_and_stable_order() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let first = storage
            .create_download("https://example.com/first.bin", 1_000)
            .unwrap();
        let second = storage
            .create_download("https://example.com/second.bin", 1_001)
            .unwrap();

        let first = storage
            .enqueue_download(&first.id, "default", None)
            .unwrap();
        let second = storage
            .enqueue_download(&second.id, "default", Some(DownloadPriority::High))
            .unwrap();

        assert_eq!(first.status, DownloadStatus::Queued);
        assert_eq!(first.queue_id.as_deref(), Some("default"));
        assert_eq!(first.priority, DownloadPriority::Normal);
        assert_eq!(first.queue_position, Some(0));
        assert_eq!(second.priority, DownloadPriority::High);
        assert_eq!(second.queue_position, Some(1));

        let queued = storage.list_queued_downloads("default").unwrap();
        assert_eq!(queued[0].id, second.id);
        assert_eq!(queued[1].id, first.id);
    }

    #[test]
    fn moves_reorders_and_removes_queued_tasks() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let named = storage
            .create_queue("Night", 1, None, DownloadPriority::Low, 1_000)
            .unwrap();
        let first = storage
            .create_download("https://example.com/first.bin", 1_000)
            .unwrap();
        let second = storage
            .create_download("https://example.com/second.bin", 1_001)
            .unwrap();

        storage
            .enqueue_download(&first.id, "default", None)
            .unwrap();
        storage
            .enqueue_download(&second.id, "default", None)
            .unwrap();
        storage
            .reorder_queue_downloads("default", &[second.id.clone(), first.id.clone()])
            .unwrap();

        let ordered = storage.list_queued_downloads("default").unwrap();
        assert_eq!(ordered[0].id, second.id);
        assert_eq!(ordered[1].id, first.id);

        let moved = storage.move_queued_download(&first.id, &named.id).unwrap();
        assert_eq!(moved.queue_id.as_deref(), Some(named.id.as_str()));
        assert_eq!(moved.queue_position, Some(0));

        let removed = storage.remove_download_from_queue(&first.id).unwrap();
        assert_eq!(removed.status, DownloadStatus::Created);
        assert!(removed.queue_id.is_none());
        assert!(removed.queue_position.is_none());
    }

    #[test]
    fn queue_running_state_persists() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        {
            let storage = Storage::open(&database_path).unwrap();
            storage
                .set_queue_state("default", QueueState::Running, 2_000)
                .unwrap();
        }

        let storage = Storage::open(&database_path).unwrap();
        let queue = storage.get_queue("default").unwrap().unwrap();
        assert_eq!(queue.state, QueueState::Running);
    }
}
