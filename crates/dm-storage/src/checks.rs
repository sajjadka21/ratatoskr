use crate::{Result, Storage, StorageError};
use rusqlite::{OptionalExtension, params};

/// What was checked or done after a download finished. Empty fields mean
/// the step did not run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadChecks {
    pub download_id: String,
    /// `idle`, `running` or `done`.
    pub state: String,
    pub expected_checksum: Option<String>,
    /// `md5`, `sha1` or `sha256`.
    pub algorithm: Option<String>,
    pub actual_checksum: Option<String>,
    /// `verified`, `mismatch` or `error`.
    pub integrity: Option<String>,
    /// `clean`, `threat` or `unavailable`.
    pub scan: Option<String>,
    pub scan_detail: Option<String>,
    pub extracted_to: Option<String>,
    pub extract_error: Option<String>,
    pub command_error: Option<String>,
    pub updated_at: i64,
}

impl Storage {
    pub fn get_download_checks(&self, download_id: &str) -> Result<Option<DownloadChecks>> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                r#"
                SELECT download_id, state, expected_checksum, algorithm, actual_checksum,
                       integrity, scan, scan_detail, extracted_to, extract_error,
                       command_error, updated_at
                FROM download_checks WHERE download_id = ?1;
                "#,
                [download_id],
                |row| {
                    Ok(DownloadChecks {
                        download_id: row.get(0)?,
                        state: row.get(1)?,
                        expected_checksum: row.get(2)?,
                        algorithm: row.get(3)?,
                        actual_checksum: row.get(4)?,
                        integrity: row.get(5)?,
                        scan: row.get(6)?,
                        scan_detail: row.get(7)?,
                        extracted_to: row.get(8)?,
                        extract_error: row.get(9)?,
                        command_error: row.get(10)?,
                        updated_at: row.get(11)?,
                    })
                },
            )
            .optional()?)
    }

    /// Stores every field of `checks`, creating the row when needed.
    pub fn save_download_checks(&self, checks: &DownloadChecks) -> Result<()> {
        let connection = self.connection()?;
        let exists = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM downloads WHERE id = ?1);",
            [&checks.download_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StorageError::DownloadNotFound(checks.download_id.clone()));
        }
        let state = if checks.state.is_empty() {
            "idle"
        } else {
            checks.state.as_str()
        };
        connection.execute(
            r#"
            INSERT INTO download_checks (
                download_id, state, expected_checksum, algorithm, actual_checksum,
                integrity, scan, scan_detail, extracted_to, extract_error,
                command_error, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT (download_id) DO UPDATE SET
                state = excluded.state,
                expected_checksum = excluded.expected_checksum,
                algorithm = excluded.algorithm,
                actual_checksum = excluded.actual_checksum,
                integrity = excluded.integrity,
                scan = excluded.scan,
                scan_detail = excluded.scan_detail,
                extracted_to = excluded.extracted_to,
                extract_error = excluded.extract_error,
                command_error = excluded.command_error,
                updated_at = excluded.updated_at;
            "#,
            params![
                &checks.download_id,
                state,
                &checks.expected_checksum,
                &checks.algorithm,
                &checks.actual_checksum,
                &checks.integrity,
                &checks.scan,
                &checks.scan_detail,
                &checks.extracted_to,
                &checks.extract_error,
                &checks.command_error,
                checks.updated_at,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn checks_round_trip_and_go_with_the_download() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/a.iso", 1)
            .unwrap();
        assert_eq!(storage.get_download_checks(&task.id).unwrap(), None);

        let checks = DownloadChecks {
            download_id: task.id.clone(),
            state: "done".to_owned(),
            expected_checksum: Some("abc".to_owned()),
            algorithm: Some("sha256".to_owned()),
            integrity: Some("mismatch".to_owned()),
            scan: Some("clean".to_owned()),
            updated_at: 5,
            ..DownloadChecks::default()
        };
        storage.save_download_checks(&checks).unwrap();
        assert_eq!(storage.get_download_checks(&task.id).unwrap(), Some(checks));

        storage.remove_download_record(&task.id).unwrap();
        assert_eq!(storage.get_download_checks(&task.id).unwrap(), None);
    }

    #[test]
    fn values_outside_the_known_sets_are_refused() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/a.iso", 1)
            .unwrap();
        let bad = DownloadChecks {
            download_id: task.id.clone(),
            integrity: Some("probably".to_owned()),
            ..DownloadChecks::default()
        };
        assert!(storage.save_download_checks(&bad).is_err());
        assert!(
            storage
                .save_download_checks(&DownloadChecks {
                    download_id: "missing".to_owned(),
                    ..DownloadChecks::default()
                })
                .is_err()
        );
    }
}
