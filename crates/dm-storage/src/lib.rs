use dm_common::DownloadStatus;
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use thiserror::Error;

mod downloads;
mod queues;
mod segments;
mod settings;

const LATEST_SCHEMA_VERSION: i32 = 4;

const MIGRATION_V1: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS downloads (
    id TEXT PRIMARY KEY NOT NULL,
    source_url TEXT NOT NULL,
    resolved_url TEXT,
    filename TEXT,
    destination_path TEXT,
    temp_path TEXT,
    mime_type TEXT,
    total_bytes INTEGER,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    etag TEXT,
    last_modified TEXT,
    range_supported INTEGER
        CHECK (range_supported IS NULL OR range_supported IN (0, 1)),
    status TEXT NOT NULL DEFAULT 'created'
        CHECK (
            status IN (
                'created',
                'probing',
                'queued',
                'downloading',
                'paused',
                'retrying',
                'finalizing',
                'completed',
                'failed',
                'cancelled'
            )
        ),
    queue_position INTEGER,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    error_code TEXT,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_downloads_status
    ON downloads(status);

CREATE INDEX IF NOT EXISTS idx_downloads_queue_position
    ON downloads(queue_position);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

PRAGMA user_version = 1;

COMMIT;
"#;

const MIGRATION_V2: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE queues (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1
        CHECK (enabled IN (0, 1)),
    state TEXT NOT NULL DEFAULT 'stopped'
        CHECK (state IN ('running', 'stopped')),
    sort_order INTEGER NOT NULL,
    max_concurrent INTEGER NOT NULL DEFAULT 3
        CHECK (max_concurrent > 0),
    max_concurrent_per_host INTEGER
        CHECK (max_concurrent_per_host IS NULL OR max_concurrent_per_host > 0),
    default_priority TEXT NOT NULL DEFAULT 'normal'
        CHECK (default_priority IN ('low', 'normal', 'high', 'very_high')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO queues (
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
)
VALUES (
    'default',
    'Default Queue',
    1,
    'stopped',
    0,
    3,
    2,
    'normal',
    unixepoch(),
    unixepoch()
);

ALTER TABLE downloads ADD COLUMN queue_id TEXT
    REFERENCES queues(id) ON DELETE SET NULL;

ALTER TABLE downloads ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal'
    CHECK (priority IN ('low', 'normal', 'high', 'very_high'));

CREATE INDEX idx_downloads_queue
    ON downloads(queue_id, queue_position);

PRAGMA user_version = 2;

COMMIT;
"#;

const MIGRATION_V3: &str = r#"
BEGIN IMMEDIATE;

ALTER TABLE downloads ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0
    CHECK (attempts >= 0);

ALTER TABLE downloads ADD COLUMN retry_at INTEGER;

CREATE INDEX idx_downloads_retry_at
    ON downloads(retry_at);

PRAGMA user_version = 3;

COMMIT;
"#;

const MIGRATION_V4: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_segments (
    download_id TEXT NOT NULL
        REFERENCES downloads(id) ON DELETE CASCADE,
    segment_index INTEGER NOT NULL
        CHECK (segment_index >= 0),
    start_byte INTEGER NOT NULL
        CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL
        CHECK (end_byte >= start_byte),
    downloaded_bytes INTEGER NOT NULL DEFAULT 0
        CHECK (downloaded_bytes >= 0
            AND downloaded_bytes <= end_byte - start_byte + 1),
    temp_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'downloading', 'completed')),
    PRIMARY KEY (download_id, segment_index),
    UNIQUE (download_id, temp_path)
);

CREATE INDEX idx_download_segments_status
    ON download_segments(download_id, status, segment_index);

PRAGMA user_version = 4;

COMMIT;
"#;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create database directory: {0}")]
    CreateDirectory(#[source] std::io::Error),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("database mutex is poisoned")]
    LockPoisoned,

    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchemaVersion { found: i32, supported: i32 },

    #[error("download not found: {0}")]
    DownloadNotFound(String),

    #[error("invalid download status stored in database: {0}")]
    InvalidDownloadStatus(String),

    #[error("invalid download state transition for {id}: {from} -> {to}")]
    InvalidStatusTransition {
        id: String,
        from: DownloadStatus,
        to: DownloadStatus,
    },

    #[error("download {id} cannot be removed while its status is {status}")]
    DownloadNotRemovable { id: String, status: DownloadStatus },

    #[error("invalid queue state stored in database: {0}")]
    InvalidQueueState(String),

    #[error("invalid download priority stored in database: {0}")]
    InvalidDownloadPriority(String),

    #[error("invalid queue concurrency stored in database: {0}")]
    InvalidQueueConcurrency(i64),

    #[error("queue not found: {0}")]
    QueueNotFound(String),

    #[error("invalid queue configuration: {0}")]
    InvalidQueueConfiguration(String),

    #[error("queue order does not contain exactly the queued tasks for queue {0}")]
    QueueOrderMismatch(String),

    #[error("download segment not found: {download_id}/{segment_index}")]
    SegmentNotFound {
        download_id: String,
        segment_index: u32,
    },

    #[error("invalid download segment: {0}")]
    InvalidSegment(String),

    #[error("value for {field} is too large for SQLite INTEGER: {value}")]
    IntegerTooLarge { field: &'static str, value: u64 },

    #[error("negative SQLite INTEGER for unsigned field {field}: {value}")]
    NegativeInteger { field: &'static str, value: i64 },
}

pub type Result<T> = std::result::Result<T, StorageError>;

pub struct Storage {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StorageError::CreateDirectory)?;
        }

        let connection = Connection::open(&path)?;
        configure_connection(&connection)?;
        run_migrations(&connection)?;

        Ok(Self {
            path,
            connection: Mutex::new(connection),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> Result<i32> {
        let connection = self.connection()?;

        let version = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

        Ok(version)
    }

    pub fn health_check(&self) -> Result<()> {
        let connection = self.connection()?;

        let value: i32 = connection.query_row("SELECT 1;", [], |row| row.get(0))?;

        if value == 1 {
            Ok(())
        } else {
            Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery))
        }
    }

    pub fn table_exists(&self, name: &str) -> Result<bool> {
        let connection = self.connection()?;

        let count: i64 = connection.query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
              AND name = ?1;
            ",
            [name],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)
    }
}

fn configure_connection(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        "#,
    )?;

    Ok(())
}

fn run_migrations(connection: &Connection) -> Result<()> {
    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version > LATEST_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion {
            found: version,
            supported: LATEST_SCHEMA_VERSION,
        });
    }

    if version == 0 {
        connection.execute_batch(MIGRATION_V1)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 1 {
        connection.execute_batch(MIGRATION_V2)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 2 {
        connection.execute_batch(MIGRATION_V3)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 3 {
        connection.execute_batch(MIGRATION_V4)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{LATEST_SCHEMA_VERSION, MIGRATION_V1, MIGRATION_V3, Storage};
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn initializes_database_and_applies_v1_migration() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        let storage = Storage::open(&database_path).unwrap();

        assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        assert!(storage.table_exists("downloads").unwrap());
        assert!(storage.table_exists("settings").unwrap());
        assert!(storage.table_exists("queues").unwrap());
        assert!(storage.table_exists("download_segments").unwrap());
        assert!(storage.health_check().is_ok());
    }

    #[test]
    fn reopening_existing_database_is_safe() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        {
            let storage = Storage::open(&database_path).unwrap();
            assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        }

        {
            let storage = Storage::open(&database_path).unwrap();
            assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
            assert!(storage.health_check().is_ok());
        }
    }

    #[test]
    fn migrates_v1_database_without_losing_download_history() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        {
            let connection = Connection::open(&database_path).unwrap();
            connection.execute_batch(MIGRATION_V1).unwrap();
            connection
                .execute(
                    "INSERT INTO downloads (id, source_url, status, created_at) VALUES (?1, ?2, 'created', ?3)",
                    ("existing-id", "https://example.com/existing.bin", 1_000_i64),
                )
                .unwrap();
        }

        let storage = Storage::open(&database_path).unwrap();

        assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        assert_eq!(
            storage
                .get_download("existing-id")
                .unwrap()
                .unwrap()
                .source_url,
            "https://example.com/existing.bin"
        );

        let queues = storage.list_queues().unwrap();
        assert_eq!(queues.len(), 1);
        assert_eq!(queues[0].id, "default");
        assert_eq!(queues[0].name, "Default Queue");
    }

    #[test]
    fn migrates_v3_database_to_segment_schema_without_losing_history() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        {
            let connection = Connection::open(&database_path).unwrap();
            connection.execute_batch(MIGRATION_V1).unwrap();
            connection.execute_batch(
                "ALTER TABLE downloads ADD COLUMN queue_id TEXT;\n                 ALTER TABLE downloads ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal';\n                 CREATE TABLE queues (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL UNIQUE, enabled INTEGER NOT NULL DEFAULT 1, state TEXT NOT NULL DEFAULT 'stopped', sort_order INTEGER NOT NULL, max_concurrent INTEGER NOT NULL DEFAULT 3, max_concurrent_per_host INTEGER, default_priority TEXT NOT NULL DEFAULT 'normal', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);\n                 INSERT INTO queues (id, name, sort_order, created_at, updated_at) VALUES ('default', 'Default Queue', 0, 1, 1);\n                 PRAGMA user_version = 2;",
            )
            .unwrap();
            connection.execute_batch(MIGRATION_V3).unwrap();
            connection
                .execute(
                    "INSERT INTO downloads (id, source_url, status, created_at) VALUES (?1, ?2, 'created', ?3)",
                    ("v3-id", "https://example.com/v3.bin", 2_000_i64),
                )
                .unwrap();
        }

        let storage = Storage::open(&database_path).unwrap();

        assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        assert!(storage.table_exists("download_segments").unwrap());
        assert_eq!(
            storage.get_download("v3-id").unwrap().unwrap().source_url,
            "https://example.com/v3.bin"
        );
    }
}
