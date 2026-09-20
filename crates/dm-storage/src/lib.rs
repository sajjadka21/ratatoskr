use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use thiserror::Error;

const LATEST_SCHEMA_VERSION: i32 = 1;

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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Storage;
    use tempfile::tempdir;

    #[test]
    fn initializes_database_and_applies_v1_migration() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        let storage = Storage::open(&database_path).unwrap();

        assert_eq!(storage.schema_version().unwrap(), 1);
        assert!(storage.table_exists("downloads").unwrap());
        assert!(storage.table_exists("settings").unwrap());
        assert!(storage.health_check().is_ok());
    }

    #[test]
    fn reopening_existing_database_is_safe() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        {
            let storage = Storage::open(&database_path).unwrap();
            assert_eq!(storage.schema_version().unwrap(), 1);
        }

        {
            let storage = Storage::open(&database_path).unwrap();
            assert_eq!(storage.schema_version().unwrap(), 1);
            assert!(storage.health_check().is_ok());
        }
    }
}
