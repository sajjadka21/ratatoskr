use crate::{Result, Storage, StorageError};
use dm_common::{DownloadSegment, SegmentStatus};
use rusqlite::{OptionalExtension, Row, params};
use std::str::FromStr;

impl Storage {
    pub fn replace_download_segments(
        &self,
        download_id: &str,
        segments: &[DownloadSegment],
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        ensure_download_exists(&transaction, download_id)?;

        transaction.execute(
            "DELETE FROM download_segments WHERE download_id = ?1;",
            [download_id],
        )?;

        for segment in segments {
            validate_segment(segment, download_id)?;
            transaction.execute(
                r#"
                INSERT INTO download_segments (
                    download_id, segment_index, start_byte, end_byte,
                    downloaded_bytes, temp_path, status
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);
                "#,
                params![
                    &segment.download_id,
                    i64::from(segment.segment_index),
                    u64_to_i64(segment.start_byte, "segment_start_byte")?,
                    u64_to_i64(segment.end_byte, "segment_end_byte")?,
                    u64_to_i64(segment.downloaded_bytes, "segment_downloaded_bytes")?,
                    &segment.temp_path,
                    segment.status.as_str(),
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn list_download_segments(&self, download_id: &str) -> Result<Vec<DownloadSegment>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT download_id, segment_index, start_byte, end_byte,
                   downloaded_bytes, temp_path, status
            FROM download_segments
            WHERE download_id = ?1
            ORDER BY segment_index ASC;
            "#,
        )?;
        let rows = statement.query_map([download_id], StoredSegmentRow::from_row)?;
        rows.map(|row| row?.into_segment()).collect()
    }

    pub fn reset_incomplete_download_segments(&self, download_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE download_segments SET status = 'pending' WHERE download_id = ?1 AND status = 'downloading';",
            [download_id],
        )?;
        Ok(())
    }

    pub fn claim_download_segment(&self, download_id: &str, segment_index: u32) -> Result<()> {
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE download_segments SET status = 'downloading' WHERE download_id = ?1 AND segment_index = ?2 AND status = 'pending';",
            params![download_id, i64::from(segment_index)],
        )?;
        if changed == 0 {
            ensure_segment_exists(&connection, download_id, segment_index)?;
            return Err(StorageError::InvalidSegment(
                "segment is already claimed or completed".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn update_download_segment_progress(
        &self,
        download_id: &str,
        segment_index: u32,
        downloaded_bytes: u64,
    ) -> Result<()> {
        let connection = self.connection()?;
        let downloaded_bytes = u64_to_i64(downloaded_bytes, "segment_downloaded_bytes")?;
        let changed = connection.execute(
            "UPDATE download_segments SET downloaded_bytes = ?3 WHERE download_id = ?1 AND segment_index = ?2 AND status = 'downloading';",
            params![download_id, i64::from(segment_index), downloaded_bytes],
        )?;
        if changed == 0 {
            ensure_segment_exists(&connection, download_id, segment_index)?;
            return Err(StorageError::InvalidSegment(
                "segment is not currently downloading".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn complete_download_segment(&self, download_id: &str, segment_index: u32) -> Result<()> {
        let connection = self.connection()?;
        let changed = connection.execute(
            r#"
            UPDATE download_segments
            SET status = 'completed'
            WHERE download_id = ?1
              AND segment_index = ?2
              AND status = 'downloading'
              AND downloaded_bytes = end_byte - start_byte + 1;
            "#,
            params![download_id, i64::from(segment_index)],
        )?;
        if changed == 0 {
            ensure_segment_exists(&connection, download_id, segment_index)?;
            return Err(StorageError::InvalidSegment(
                "segment did not reach its expected byte count".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn clear_download_segments(&self, download_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM download_segments WHERE download_id = ?1;",
            [download_id],
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct StoredSegmentRow {
    download_id: String,
    segment_index: i64,
    start_byte: i64,
    end_byte: i64,
    downloaded_bytes: i64,
    temp_path: String,
    status: String,
}

impl StoredSegmentRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            download_id: row.get(0)?,
            segment_index: row.get(1)?,
            start_byte: row.get(2)?,
            end_byte: row.get(3)?,
            downloaded_bytes: row.get(4)?,
            temp_path: row.get(5)?,
            status: row.get(6)?,
        })
    }

    fn into_segment(self) -> Result<DownloadSegment> {
        let status = SegmentStatus::from_str(&self.status)
            .map_err(|_| StorageError::InvalidSegment(self.status.clone()))?;
        let segment = DownloadSegment {
            download_id: self.download_id,
            segment_index: u32::try_from(self.segment_index)
                .map_err(|_| StorageError::InvalidSegment("segment index overflow".to_owned()))?,
            start_byte: i64_to_u64(self.start_byte, "segment_start_byte")?,
            end_byte: i64_to_u64(self.end_byte, "segment_end_byte")?,
            downloaded_bytes: i64_to_u64(self.downloaded_bytes, "segment_downloaded_bytes")?,
            temp_path: self.temp_path,
            status,
        };
        validate_segment(&segment, &segment.download_id)?;
        Ok(segment)
    }
}

fn validate_segment(segment: &DownloadSegment, download_id: &str) -> Result<()> {
    if segment.download_id != download_id {
        return Err(StorageError::InvalidSegment(
            "segment belongs to another download".to_owned(),
        ));
    }
    let Some(expected) = segment.expected_bytes() else {
        return Err(StorageError::InvalidSegment(
            "segment range overflows".to_owned(),
        ));
    };
    if segment.downloaded_bytes > expected || segment.temp_path.trim().is_empty() {
        return Err(StorageError::InvalidSegment(
            "segment progress or path is invalid".to_owned(),
        ));
    }
    if segment.status == SegmentStatus::Completed && segment.downloaded_bytes != expected {
        return Err(StorageError::InvalidSegment(
            "completed segment is short".to_owned(),
        ));
    }
    Ok(())
}

fn ensure_download_exists(connection: &rusqlite::Connection, download_id: &str) -> Result<()> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM downloads WHERE id = ?1);",
        [download_id],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(StorageError::DownloadNotFound(download_id.to_owned()))
    }
}

fn ensure_segment_exists(
    connection: &rusqlite::Connection,
    download_id: &str,
    segment_index: u32,
) -> Result<()> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM download_segments WHERE download_id = ?1 AND segment_index = ?2;",
            params![download_id, i64::from(segment_index)],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_some() {
        Ok(())
    } else {
        Err(StorageError::SegmentNotFound {
            download_id: download_id.to_owned(),
            segment_index,
        })
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
    use dm_common::SegmentStatus;
    use tempfile::tempdir;

    #[test]
    fn persists_and_reopens_segment_map() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("downloads.db");
        let task_id;
        {
            let storage = Storage::open(&database_path).unwrap();
            let task = storage
                .create_download("https://example.com/file.bin", 1_000)
                .unwrap();
            task_id = task.id.clone();
            let segments = vec![
                DownloadSegment {
                    download_id: task.id.clone(),
                    segment_index: 0,
                    start_byte: 0,
                    end_byte: 4,
                    downloaded_bytes: 0,
                    temp_path: "file.0.part".to_owned(),
                    status: SegmentStatus::Pending,
                },
                DownloadSegment {
                    download_id: task.id,
                    segment_index: 1,
                    start_byte: 5,
                    end_byte: 9,
                    downloaded_bytes: 5,
                    temp_path: "file.1.part".to_owned(),
                    status: SegmentStatus::Completed,
                },
            ];
            storage
                .replace_download_segments(&task_id, &segments)
                .unwrap();
            storage.claim_download_segment(&task_id, 0).unwrap();
            storage
                .update_download_segment_progress(&task_id, 0, 5)
                .unwrap();
            storage.complete_download_segment(&task_id, 0).unwrap();
        }

        let storage = Storage::open(&database_path).unwrap();
        let segments = storage.list_download_segments(&task_id).unwrap();
        assert_eq!(segments.len(), 2);
        assert!(segments.iter().all(DownloadSegment::is_complete));
    }

    #[test]
    fn resetting_incomplete_segments_makes_them_claimable() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/file.bin", 1_000)
            .unwrap();
        storage
            .replace_download_segments(
                &task.id,
                &[DownloadSegment {
                    download_id: task.id.clone(),
                    segment_index: 0,
                    start_byte: 0,
                    end_byte: 9,
                    downloaded_bytes: 0,
                    temp_path: "file.0.part".to_owned(),
                    status: SegmentStatus::Pending,
                }],
            )
            .unwrap();
        storage.claim_download_segment(&task.id, 0).unwrap();
        storage
            .reset_incomplete_download_segments(&task.id)
            .unwrap();
        storage.claim_download_segment(&task.id, 0).unwrap();
    }
}
