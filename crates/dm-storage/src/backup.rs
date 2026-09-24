//! Backups of the database and restoring one.
//!
//! A backup is a consistent copy made by SQLite itself (`VACUUM INTO`), so
//! it can be taken while downloads run. Restoring never replaces the open
//! database: the backup is checked, copied next to the database, and swapped
//! in on the next start, before anything opens the database. The database
//! it replaces is kept beside it, so a restore can always be undone by hand.
//!
//! The database holds no cookies, passwords or authorization headers (browser
//! sessions live only in memory), so a backup does not either.

use crate::{LATEST_SCHEMA_VERSION, Result, Storage, StorageError};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// What a backup file holds, as far as checking it tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    pub schema_version: i32,
    pub downloads: u64,
    pub queues: u64,
    pub bytes: u64,
}

/// What happened to a restore waiting from the previous run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// The backup is now the database; the one it replaced was kept here.
    Restored { kept_copy: PathBuf },
    /// The waiting backup could not be used and was set aside; the database
    /// was not touched.
    Refused { reason: String },
}

impl Storage {
    /// Writes a consistent copy of the database to `target`, replacing a
    /// file already there only once the copy is complete.
    pub fn backup_to(&self, target: &Path) -> Result<BackupInfo> {
        if target.is_dir() {
            return Err(StorageError::InvalidBackup(
                "the target is a folder".to_owned(),
            ));
        }
        let partial = sibling(target, ".partial");
        let _ = fs::remove_file(&partial);
        {
            let connection = self.connection()?;
            connection.execute("VACUUM INTO ?1;", [partial.to_string_lossy().as_ref()])?;
        }
        let info = match inspect_backup(&partial) {
            Ok(info) => info,
            Err(error) => {
                let _ = fs::remove_file(&partial);
                return Err(error);
            }
        };
        fs::rename(&partial, target).map_err(StorageError::File)?;
        Ok(info)
    }

    /// `ok` when SQLite finds nothing wrong, otherwise its first findings.
    pub fn integrity_check(&self) -> Result<String> {
        let connection = self.connection()?;
        integrity(&connection)
    }

    /// The size of the database file and its write-ahead log, in bytes.
    pub fn database_bytes(&self) -> u64 {
        let size = |path: &Path| fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        size(&self.path) + size(&sibling(&self.path, "-wal"))
    }
}

/// Opens a backup read-only and checks that this version can use it: an
/// intact SQLite file with the tables of a download database, from this
/// version or an older one.
pub fn inspect_backup(path: &Path) -> Result<BackupInfo> {
    let refuse = |reason: &str| StorageError::InvalidBackup(reason.to_owned());
    let bytes = fs::metadata(path)
        .map_err(|_| refuse("the file cannot be read"))?
        .len();
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| refuse("not a database file"))?;

    let version: i32 = connection
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .map_err(|_| refuse("not a database file"))?;
    if version < 1 {
        return Err(refuse("not a download database"));
    }
    if version > LATEST_SCHEMA_VERSION {
        return Err(refuse("made by a newer version of the application"));
    }
    match integrity(&connection) {
        Ok(result) if result == "ok" => {}
        _ => return Err(refuse("the file is damaged")),
    }
    let count = |table: &str| -> Result<u64> {
        let exists: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1;",
            [table],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(0);
        }
        let rows: i64 =
            connection.query_row(&format!("SELECT COUNT(*) FROM {table};"), [], |row| {
                row.get(0)
            })?;
        Ok(u64::try_from(rows).unwrap_or(0))
    };
    let has_downloads: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'downloads';",
        [],
        |row| row.get(0),
    )?;
    if has_downloads == 0 {
        return Err(refuse("not a download database"));
    }

    Ok(BackupInfo {
        schema_version: version,
        downloads: count("downloads")?,
        queues: count("queues")?,
        bytes,
    })
}

/// Where a restore waits for the next start.
pub fn pending_restore_path(database: &Path) -> PathBuf {
    sibling(database, ".restore")
}

/// Checks `backup` and places a copy where the next start will pick it up.
pub fn stage_restore(database: &Path, backup: &Path) -> Result<BackupInfo> {
    let info = inspect_backup(backup)?;
    let pending = pending_restore_path(database);
    let partial = sibling(&pending, ".partial");
    fs::copy(backup, &partial).map_err(StorageError::File)?;
    fs::rename(&partial, &pending).map_err(StorageError::File)?;
    Ok(info)
}

/// The restore waiting for the next start, if there is one.
pub fn pending_restore(database: &Path) -> Option<BackupInfo> {
    let pending = pending_restore_path(database);
    pending
        .is_file()
        .then(|| inspect_backup(&pending).ok())
        .flatten()
}

/// Drops a waiting restore.
pub fn cancel_pending_restore(database: &Path) -> Result<()> {
    match fs::remove_file(pending_restore_path(database)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::File(error)),
    }
}

/// Swaps a waiting restore in. Must run before the database is opened.
/// The replaced database (with its write-ahead log) is renamed, not
/// deleted.
pub fn apply_pending_restore(database: &Path) -> Result<Option<RestoreOutcome>> {
    let pending = pending_restore_path(database);
    if !pending.is_file() {
        return Ok(None);
    }
    if let Err(error) = inspect_backup(&pending) {
        let _ = fs::rename(&pending, sibling(&pending, ".refused"));
        return Ok(Some(RestoreOutcome::Refused {
            reason: error.to_string(),
        }));
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let stem = database
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "downloads".to_owned());
    let kept = database.with_file_name(format!("{stem}.before-restore-{stamp}.db"));

    if database.exists() {
        fs::rename(database, &kept).map_err(StorageError::File)?;
        for suffix in ["-wal", "-shm"] {
            let companion = sibling(database, suffix);
            if companion.exists() {
                fs::rename(&companion, sibling(&kept, suffix)).map_err(StorageError::File)?;
            }
        }
    }
    fs::rename(&pending, database).map_err(StorageError::File)?;
    Ok(Some(RestoreOutcome::Restored { kept_copy: kept }))
}

fn integrity(connection: &Connection) -> Result<String> {
    let mut statement = connection.prepare("PRAGMA integrity_check(5);")?;
    let findings = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(findings.join("; "))
}

/// `path` with `suffix` appended to its file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn storage_with_download(path: &Path) -> Storage {
        let storage = Storage::open(path).unwrap();
        storage
            .create_download("https://example.com/a.iso", 1)
            .unwrap();
        storage
    }

    #[test]
    fn a_backup_is_a_complete_checked_copy() {
        let directory = tempdir().unwrap();
        let storage = storage_with_download(&directory.path().join("downloads.db"));
        let target = directory.path().join("backup.rudbackup");
        std::fs::write(&target, b"an older backup").unwrap();

        let info = storage.backup_to(&target).unwrap();

        assert_eq!(info.downloads, 1);
        assert_eq!(info.schema_version, LATEST_SCHEMA_VERSION);
        assert_eq!(inspect_backup(&target).unwrap(), info);
        assert!(!directory.path().join("backup.rudbackup.partial").exists());
        assert_eq!(storage.integrity_check().unwrap(), "ok");
        assert!(storage.database_bytes() > 0);
    }

    #[test]
    fn files_that_are_not_backups_are_refused() {
        let directory = tempdir().unwrap();
        let text = directory.path().join("notes.txt");
        std::fs::write(&text, b"hello").unwrap();
        assert!(matches!(
            inspect_backup(&text),
            Err(StorageError::InvalidBackup(_))
        ));

        let other = directory.path().join("other.db");
        Connection::open(&other)
            .unwrap()
            .execute_batch("CREATE TABLE notes (x); PRAGMA user_version = 3;")
            .unwrap();
        assert!(matches!(
            inspect_backup(&other),
            Err(StorageError::InvalidBackup(_))
        ));

        let newer = directory.path().join("newer.db");
        storage_with_download(&newer);
        Connection::open(&newer)
            .unwrap()
            .execute_batch(&format!(
                "PRAGMA user_version = {};",
                LATEST_SCHEMA_VERSION + 1
            ))
            .unwrap();
        assert!(matches!(
            inspect_backup(&newer),
            Err(StorageError::InvalidBackup(_))
        ));

        assert!(matches!(
            stage_restore(&directory.path().join("downloads.db"), &text),
            Err(StorageError::InvalidBackup(_))
        ));
        assert!(!pending_restore_path(&directory.path().join("downloads.db")).exists());
    }

    #[test]
    fn a_restore_waits_for_the_next_start_and_keeps_the_old_database() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");
        let backup = directory.path().join("backup.rudbackup");
        {
            let storage = storage_with_download(&database);
            storage.backup_to(&backup).unwrap();
            // Work done after the backup, which the restore rolls back.
            storage
                .create_download("https://example.com/b.iso", 2)
                .unwrap();
        }

        assert_eq!(apply_pending_restore(&database).unwrap(), None);
        let staged = stage_restore(&database, &backup).unwrap();
        assert_eq!(pending_restore(&database), Some(staged));

        let outcome = apply_pending_restore(&database).unwrap().unwrap();
        let RestoreOutcome::Restored { kept_copy } = outcome else {
            panic!("not restored");
        };
        assert_eq!(
            Storage::open(&database)
                .unwrap()
                .list_downloads()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            Storage::open(&kept_copy)
                .unwrap()
                .list_downloads()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(pending_restore(&database), None);
    }

    #[test]
    fn a_waiting_restore_can_be_cancelled_or_is_set_aside_when_damaged() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");
        let backup = directory.path().join("backup.rudbackup");
        storage_with_download(&database).backup_to(&backup).unwrap();

        stage_restore(&database, &backup).unwrap();
        cancel_pending_restore(&database).unwrap();
        cancel_pending_restore(&database).unwrap();
        assert_eq!(apply_pending_restore(&database).unwrap(), None);

        std::fs::write(pending_restore_path(&database), b"damaged").unwrap();
        assert!(matches!(
            apply_pending_restore(&database).unwrap(),
            Some(RestoreOutcome::Refused { .. })
        ));
        assert_eq!(
            Storage::open(&database)
                .unwrap()
                .list_downloads()
                .unwrap()
                .len(),
            1
        );
    }
}
