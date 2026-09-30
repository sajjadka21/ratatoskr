use dm_common::DownloadStatus;
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use thiserror::Error;

mod backup;
mod categories;
mod checks;
mod downloads;
mod host_profiles;
mod limits;
mod mirrors;
mod queues;
mod request_context;
mod schedules;
mod segments;
mod settings;
mod traffic;

pub use backup::{
    BackupInfo, RestoreOutcome, apply_pending_restore, cancel_pending_restore, inspect_backup,
    pending_restore, stage_restore,
};
pub use checks::DownloadChecks;
pub use traffic::{TrafficScope, TrafficTotals};

const LATEST_SCHEMA_VERSION: i32 = 15;

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

const MIGRATION_V5: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE host_profiles (
    host TEXT PRIMARY KEY NOT NULL
        CHECK (length(host) > 0),
    preferred_max_connections INTEGER NOT NULL DEFAULT 1
        CHECK (preferred_max_connections > 0),
    rate_limited_count INTEGER NOT NULL DEFAULT 0
        CHECK (rate_limited_count >= 0),
    busy_count INTEGER NOT NULL DEFAULT 0
        CHECK (busy_count >= 0),
    last_status INTEGER
        CHECK (last_status IS NULL OR last_status IN (429, 503)),
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_host_profiles_updated_at
    ON host_profiles(updated_at);

PRAGMA user_version = 5;

COMMIT;
"#;

const MIGRATION_V6: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE categories (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    extensions_json TEXT NOT NULL DEFAULT '[]',
    mime_patterns_json TEXT NOT NULL DEFAULT '[]',
    default_directory TEXT,
    host_patterns_json TEXT NOT NULL DEFAULT '[]',
    priority TEXT NOT NULL DEFAULT 'normal'
        CHECK (priority IN ('low', 'normal', 'high', 'very_high')),
    queue_id TEXT REFERENCES queues(id) ON DELETE SET NULL
);

CREATE TABLE download_rules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    sort_order INTEGER NOT NULL,
    domain TEXT,
    url_pattern TEXT,
    extension TEXT,
    mime_pattern TEXT,
    min_size INTEGER CHECK (min_size IS NULL OR min_size >= 0),
    max_size INTEGER CHECK (max_size IS NULL OR max_size >= 0),
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    destination_directory TEXT,
    queue_id TEXT REFERENCES queues(id) ON DELETE SET NULL,
    priority TEXT CHECK (priority IS NULL OR priority IN ('low', 'normal', 'high', 'very_high')),
    max_connections INTEGER CHECK (max_connections IS NULL OR max_connections > 0),
    max_host_concurrency INTEGER CHECK (max_host_concurrency IS NULL OR max_host_concurrency > 0),
    speed_cap INTEGER CHECK (speed_cap IS NULL OR speed_cap > 0),
    browser_takeover_allowed INTEGER
        CHECK (browser_takeover_allowed IS NULL OR browser_takeover_allowed IN (0, 1))
);

CREATE INDEX idx_download_rules_order ON download_rules(enabled, sort_order, id);

INSERT INTO categories (id, name, extensions_json, mime_patterns_json, priority)
VALUES
    ('applications', 'Applications', '["exe","msi","msix","appx"]', '["application/*"]', 'normal'),
    ('archives', 'Archives', '["zip","rar","7z","tar","gz","bz2","xz"]', '["application/zip","application/x-7z-compressed","application/x-rar-compressed"]', 'normal'),
    ('documents', 'Documents', '["pdf","doc","docx","xls","xlsx","ppt","pptx","txt","csv"]', '["application/pdf","text/*"]', 'normal'),
    ('video', 'Video', '["mp4","mkv","mov","avi","webm"]', '["video/*"]', 'normal'),
    ('audio', 'Audio', '["mp3","wav","flac","aac","m4a","ogg"]', '["audio/*"]', 'normal'),
    ('images', 'Images', '["png","jpg","jpeg","gif","webp","svg"]', '["image/*"]', 'normal'),
    ('other', 'Other', '[]', '[]', 'normal');

PRAGMA user_version = 6;

COMMIT;
"#;

const MIGRATION_V7: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE queue_schedules (
    queue_id TEXT PRIMARY KEY NOT NULL REFERENCES queues(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    kind TEXT NOT NULL CHECK (kind IN ('once', 'daily', 'weekdays', 'repeating')),
    start_at INTEGER NOT NULL,
    stop_at INTEGER,
    weekdays_mask INTEGER NOT NULL DEFAULT 0 CHECK (weekdays_mask >= 0 AND weekdays_mask <= 127),
    interval_seconds INTEGER CHECK (interval_seconds IS NULL OR interval_seconds > 0),
    completion_action TEXT NOT NULL DEFAULT 'none'
        CHECK (completion_action IN ('none', 'notify', 'exit_app', 'sleep', 'hibernate', 'shutdown')),
    prevent_sleep INTEGER NOT NULL DEFAULT 0 CHECK (prevent_sleep IN (0, 1)),
    updated_at INTEGER NOT NULL
);

PRAGMA user_version = 7;

COMMIT;
"#;

/// Browser-supplied request context that a server may require before it will
/// serve a file (for example a Referer check or a User-Agent allowlist). Kept
/// out of `downloads` so history rows stay unchanged, and removed with its
/// task. Cookies and credentials are never stored here.
const MIGRATION_V8: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_request_context (
    download_id TEXT PRIMARY KEY NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    referrer TEXT,
    user_agent TEXT
);

PRAGMA user_version = 8;

COMMIT;
"#;

/// Daily and weekday schedules become wall-clock windows ("02:00 to 07:00")
/// in minutes after local midnight. Existing rows keep working: without a
/// window the old start/stop times of day are used.
const MIGRATION_V9: &str = r#"
BEGIN IMMEDIATE;

ALTER TABLE queue_schedules ADD COLUMN window_start_minute INTEGER
    CHECK (window_start_minute IS NULL OR (window_start_minute >= 0 AND window_start_minute < 1440));
ALTER TABLE queue_schedules ADD COLUMN window_end_minute INTEGER
    CHECK (window_end_minute IS NULL OR (window_end_minute >= 0 AND window_end_minute < 1440));

PRAGMA user_version = 9;

COMMIT;
"#;

/// Segments of one download now share a single preallocated partial file
/// and can be split while they run, so several rows name the same path. The
/// table is rebuilt without the one-file-per-segment constraint; rows are
/// kept. Daily traffic totals, split into domestic and international, feed
/// the usage meter and the optional international quota.
const MIGRATION_V10: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_segments_v10 (
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
    PRIMARY KEY (download_id, segment_index)
);

INSERT INTO download_segments_v10
    SELECT download_id, segment_index, start_byte, end_byte,
           downloaded_bytes, temp_path, status
    FROM download_segments;

DROP TABLE download_segments;
ALTER TABLE download_segments_v10 RENAME TO download_segments;

CREATE INDEX idx_download_segments_status
    ON download_segments(download_id, status, segment_index);

CREATE TABLE traffic_usage (
    day TEXT NOT NULL
        CHECK (length(day) = 10),
    scope TEXT NOT NULL
        CHECK (scope IN ('domestic', 'international')),
    bytes INTEGER NOT NULL DEFAULT 0
        CHECK (bytes >= 0),
    PRIMARY KEY (day, scope)
);

PRAGMA user_version = 10;

COMMIT;
"#;

/// Other addresses of the same file. A segmented download spreads its
/// ranges over every mirror that serves exactly the same bytes.
const MIGRATION_V11: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_mirrors (
    download_id TEXT NOT NULL
        REFERENCES downloads(id) ON DELETE CASCADE,
    url TEXT NOT NULL
        CHECK (length(url) > 0),
    position INTEGER NOT NULL
        CHECK (position >= 0),
    PRIMARY KEY (download_id, url)
);

PRAGMA user_version = 11;

COMMIT;
"#;

/// What was checked or done after a download finished: its checksum, a
/// virus scan, archive extraction and the user's command.
/// A speed limit chosen for one download, apart from the global one.
const MIGRATION_V15: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_names (
    download_id TEXT PRIMARY KEY NOT NULL
        REFERENCES downloads(id) ON DELETE CASCADE,
    filename TEXT NOT NULL CHECK (length(filename) > 0)
);

PRAGMA user_version = 15;

COMMIT;
"#;

const MIGRATION_V14: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_folders (
    download_id TEXT PRIMARY KEY NOT NULL
        REFERENCES downloads(id) ON DELETE CASCADE,
    directory TEXT NOT NULL CHECK (length(directory) > 0)
);

PRAGMA user_version = 14;

COMMIT;
"#;

const MIGRATION_V13: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_limits (
    download_id TEXT PRIMARY KEY NOT NULL
        REFERENCES downloads(id) ON DELETE CASCADE,
    bytes_per_second INTEGER NOT NULL CHECK (bytes_per_second > 0)
);

PRAGMA user_version = 13;

COMMIT;
"#;

const MIGRATION_V12: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE download_checks (
    download_id TEXT PRIMARY KEY NOT NULL
        REFERENCES downloads(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'idle'
        CHECK (state IN ('idle', 'running', 'done')),
    expected_checksum TEXT,
    algorithm TEXT
        CHECK (algorithm IS NULL OR algorithm IN ('md5', 'sha1', 'sha256')),
    actual_checksum TEXT,
    integrity TEXT
        CHECK (integrity IS NULL OR integrity IN ('verified', 'mismatch', 'error')),
    scan TEXT
        CHECK (scan IS NULL OR scan IN ('clean', 'threat', 'unavailable')),
    scan_detail TEXT,
    extracted_to TEXT,
    extract_error TEXT,
    command_error TEXT,
    updated_at INTEGER NOT NULL DEFAULT 0
);

PRAGMA user_version = 12;

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

    #[error("not a usable backup: {0}")]
    InvalidBackup(String),

    #[error("file error: {0}")]
    File(#[source] std::io::Error),

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

    #[error("invalid category configuration: {0}")]
    InvalidCategoryConfiguration(String),

    #[error("invalid rule configuration: {0}")]
    InvalidRuleConfiguration(String),

    #[error("invalid schedule configuration: {0}")]
    InvalidScheduleConfiguration(String),

    #[error("queue order does not contain exactly the queued tasks for queue {0}")]
    QueueOrderMismatch(String),

    #[error("download segment not found: {download_id}/{segment_index}")]
    SegmentNotFound {
        download_id: String,
        segment_index: u32,
    },

    #[error("invalid download segment: {0}")]
    InvalidSegment(String),

    #[error("invalid mirror: {0}")]
    InvalidMirror(String),

    #[error("invalid traffic record: {0}")]
    InvalidTraffic(String),

    #[error("invalid host profile key: {0}")]
    InvalidHostProfile(String),

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

    // WAL lets the browser native host insert a task while the application
    // holds the same database open, without either side seeing SQLITE_BUSY
    // for ordinary reads.
    let _mode: String = connection.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;

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

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 4 {
        connection.execute_batch(MIGRATION_V5)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 5 {
        connection.execute_batch(MIGRATION_V6)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 6 {
        connection.execute_batch(MIGRATION_V7)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 7 {
        connection.execute_batch(MIGRATION_V8)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 8 {
        connection.execute_batch(MIGRATION_V9)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 9 {
        connection.execute_batch(MIGRATION_V10)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 10 {
        connection.execute_batch(MIGRATION_V11)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 11 {
        connection.execute_batch(MIGRATION_V12)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 12 {
        connection.execute_batch(MIGRATION_V13)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 13 {
        connection.execute_batch(MIGRATION_V14)?;
    }

    let version: i32 = connection.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    if version == 14 {
        connection.execute_batch(MIGRATION_V15)?;
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
        assert!(storage.table_exists("host_profiles").unwrap());
        assert!(storage.table_exists("categories").unwrap());
        assert!(storage.table_exists("download_rules").unwrap());
        assert!(storage.table_exists("queue_schedules").unwrap());
        assert!(storage.health_check().is_ok());
    }

    #[test]
    fn v10_keeps_segment_rows_and_lets_segments_share_one_file() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");

        {
            let connection = Connection::open(&database_path).unwrap();
            for migration in [
                MIGRATION_V1,
                super::MIGRATION_V2,
                MIGRATION_V3,
                super::MIGRATION_V4,
                super::MIGRATION_V5,
                super::MIGRATION_V6,
                super::MIGRATION_V7,
                super::MIGRATION_V8,
                super::MIGRATION_V9,
            ] {
                connection.execute_batch(migration).unwrap();
            }
            connection
                .execute(
                    "INSERT INTO downloads (id, source_url, status, created_at) VALUES ('kept', 'https://example.com/a.bin', 'paused', 1)",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO download_segments (download_id, segment_index, start_byte, end_byte, downloaded_bytes, temp_path, status) VALUES ('kept', 0, 0, 9, 4, 'a.part.segment-0000.part', 'pending')",
                    [],
                )
                .unwrap();
        }

        let storage = Storage::open(&database_path).unwrap();
        assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        let segments = storage.list_download_segments("kept").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].downloaded_bytes, 4);
        assert!(storage.table_exists("traffic_usage").unwrap());

        // Two rows may now name the same partial file.
        let shared = |index: u32, start: u64, end: u64| dm_common::DownloadSegment {
            download_id: "kept".to_owned(),
            segment_index: index,
            start_byte: start,
            end_byte: end,
            downloaded_bytes: 0,
            temp_path: "a.part".to_owned(),
            status: dm_common::SegmentStatus::Pending,
        };
        storage
            .replace_download_segments("kept", &[shared(0, 0, 4), shared(1, 5, 9)])
            .unwrap();
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
        assert!(storage.table_exists("host_profiles").unwrap());
        assert_eq!(
            storage.get_download("v3-id").unwrap().unwrap().source_url,
            "https://example.com/v3.bin"
        );
    }
}
