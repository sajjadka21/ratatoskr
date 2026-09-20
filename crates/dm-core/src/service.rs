use crate::{DownloadError, DownloadOutcome, Downloader};
use dm_common::{DownloadCompletion, DownloadRecord};
use dm_storage::{Storage, StorageError};
use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadServiceError {
    #[error("download engine error: {0}")]
    Download(#[from] DownloadError),

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("system clock is before Unix epoch")]
    ClockBeforeUnixEpoch,

    #[error("timestamp does not fit in SQLite INTEGER")]
    TimestampOverflow,
}

pub type Result<T> = std::result::Result<T, DownloadServiceError>;

#[derive(Clone)]
pub struct DownloadService {
    downloader: Downloader,
    storage: Arc<Storage>,
}

impl DownloadService {
    pub fn new(storage: Arc<Storage>) -> Result<Self> {
        Ok(Self {
            downloader: Downloader::new()?,
            storage,
        })
    }

    pub async fn start_download(
        &self,
        source_url: &str,
        destination_directory: impl AsRef<Path>,
    ) -> Result<DownloadRecord> {
        let created_at = unix_timestamp_seconds()?;

        let created = self.storage.create_download(source_url, created_at)?;

        let download_id = created.id.clone();

        self.storage
            .mark_downloading(&download_id, unix_timestamp_seconds()?)?;

        let storage = Arc::clone(&self.storage);
        let progress_id = download_id.clone();

        let result = self
            .downloader
            .download(source_url, destination_directory, move |progress| {
                storage
                    .update_progress(
                        &progress_id,
                        progress.downloaded_bytes,
                        progress.total_bytes,
                    )
                    .map_err(|error| DownloadError::ProgressCallback(error.to_string()))
            })
            .await;

        match result {
            Ok(outcome) => self.complete_download(&download_id, outcome),
            Err(error) => {
                let message = error.to_string();

                self.storage
                    .mark_failed(&download_id, "download_error", &message)?;

                Err(DownloadServiceError::Download(error))
            }
        }
    }

    fn complete_download(
        &self,
        download_id: &str,
        outcome: DownloadOutcome,
    ) -> Result<DownloadRecord> {
        let completion = DownloadCompletion {
            resolved_url: outcome.metadata.final_url,
            filename: outcome.metadata.filename,
            destination_path: outcome.final_path.to_string_lossy().into_owned(),
            mime_type: outcome.metadata.content_type,
            total_bytes: outcome.metadata.total_bytes,
            downloaded_bytes: outcome.downloaded_bytes,
        };

        self.storage
            .mark_completed(download_id, &completion, unix_timestamp_seconds()?)?;

        self.storage
            .get_download(download_id)?
            .ok_or_else(|| StorageError::DownloadNotFound(download_id.to_owned()))
            .map_err(DownloadServiceError::Storage)
    }
}

fn unix_timestamp_seconds() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DownloadServiceError::ClockBeforeUnixEpoch)?;

    i64::try_from(duration.as_secs()).map_err(|_| DownloadServiceError::TimestampOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dm_common::DownloadStatus;
    use tempfile::tempdir;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn downloads_file_and_persists_completed_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();

            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request).await.unwrap();

            let body = b"persistent download test";

            let headers = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Length: {}\r\n\
                 Content-Type: application/octet-stream\r\n\
                 Content-Disposition: attachment; filename=\"persist.bin\"\r\n\
                 Connection: close\r\n\
                 \r\n",
                body.len()
            );

            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
            socket.shutdown().await.unwrap();
        });

        let root = tempdir().unwrap();
        let database_path = root.path().join("downloads.db");
        let downloads_path = root.path().join("files");

        let storage = Arc::new(Storage::open(&database_path).unwrap());

        let service = DownloadService::new(Arc::clone(&storage)).unwrap();

        let record = service
            .start_download(&format!("http://{address}/download"), &downloads_path)
            .await
            .unwrap();

        server.await.unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(record.filename.as_deref(), Some("persist.bin"));
        assert_eq!(
            record.downloaded_bytes,
            b"persistent download test".len() as u64
        );

        let final_path = record.destination_path.as_ref().unwrap();

        let bytes = tokio::fs::read(final_path).await.unwrap();

        assert_eq!(bytes, b"persistent download test");

        let persisted = storage.get_download(&record.id).unwrap().unwrap();

        assert_eq!(persisted.status, DownloadStatus::Completed);
        assert_eq!(persisted.downloaded_bytes, record.downloaded_bytes);
        assert!(persisted.started_at.is_some());
        assert!(persisted.completed_at.is_some());
    }
}
