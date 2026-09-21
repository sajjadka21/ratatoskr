use crate::{Result, Storage, StorageError};
use dm_common::{DownloadPriority, QueueRecord, QueueState};
use rusqlite::Row;
use std::str::FromStr;

impl Storage {
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
