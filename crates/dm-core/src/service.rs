use crate::{
    DownloadError, DownloadOutcome, Downloader, TransferProgress, throughput::ThroughputMeter,
    validate_source_url,
};
use dm_common::{DownloadCompletion, DownloadRecord};
use dm_storage::{Storage, StorageError};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::sync::Semaphore;

const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 3;
const PROGRESS_PERSIST_INTERVAL: Duration = Duration::from_millis(500);
const PROGRESS_PERSIST_BYTES: u64 = 1024 * 1024;

/// Recorded on tasks that a process restart orphaned, so the row explains
/// itself instead of silently reappearing as unstarted work.
const INTERRUPTED_ERROR_CODE: &str = "interrupted";
const INTERRUPTED_ERROR_MESSAGE: &str = "Interrupted when the app closed. This transfer cannot be resumed yet and will restart from the beginning.";

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

    #[error("download execution is unavailable")]
    ExecutionUnavailable,
}

pub type Result<T> = std::result::Result<T, DownloadServiceError>;

#[derive(Clone)]
pub struct DownloadService {
    downloader: Downloader,
    storage: Arc<Storage>,
    execution_slots: Arc<Semaphore>,
}

impl DownloadService {
    pub fn new(storage: Arc<Storage>) -> Result<Self> {
        Ok(Self {
            downloader: Downloader::new()?,
            storage,
            execution_slots: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_DOWNLOADS)),
        })
    }

    pub fn create_task(&self, source_url: &str) -> Result<DownloadRecord> {
        validate_source_url(source_url)?;

        self.storage
            .create_download(source_url, unix_timestamp_seconds()?)
            .map_err(DownloadServiceError::Storage)
    }

    pub fn claim_task(&self, download_id: &str) -> Result<DownloadRecord> {
        self.storage.mark_probing(download_id)?;
        self.get_task(download_id)
    }

    pub async fn start_task(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
    ) -> Result<DownloadRecord> {
        self.start_task_with_progress(download_id, destination_directory, |_, _| {})
            .await
    }

    /// Returns tasks that a previous process left mid-transfer to a state the
    /// user or a queue runner can act on again. Called once during startup,
    /// before any runner is spawned, so no row is ever presented as active
    /// with nobody driving it.
    pub fn recover_orphaned_tasks(&self) -> Result<Vec<DownloadRecord>> {
        self.storage
            .recover_orphaned_downloads(INTERRUPTED_ERROR_CODE, INTERRUPTED_ERROR_MESSAGE)
            .map_err(DownloadServiceError::Storage)
    }

    pub async fn start_task_with_progress<F>(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
        on_progress: F,
    ) -> Result<DownloadRecord>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        self.claim_task(download_id)?;
        self.execute_claimed_task_with_progress(download_id, destination_directory, on_progress)
            .await
    }

    pub async fn execute_claimed_task_with_progress<F>(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
        mut on_progress: F,
    ) -> Result<DownloadRecord>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let task = self.get_task(download_id)?;
        let _execution_slot = Arc::clone(&self.execution_slots)
            .acquire_owned()
            .await
            .map_err(|_| DownloadServiceError::ExecutionUnavailable)?;

        self.storage
            .mark_downloading(download_id, unix_timestamp_seconds()?)?;

        let storage = Arc::clone(&self.storage);
        let progress_id = download_id.to_owned();
        let mut last_persisted_at = Instant::now();
        let mut last_persisted_bytes = 0_u64;
        let mut has_persisted_progress = false;
        let mut meter = ThroughputMeter::new(Instant::now(), 0);

        let result = self
            .downloader
            .download(&task.source_url, destination_directory, move |progress| {
                let reached_known_end = progress.total_bytes == Some(progress.downloaded_bytes);
                let bytes_since_persist = progress
                    .downloaded_bytes
                    .saturating_sub(last_persisted_bytes);
                let should_persist = !has_persisted_progress
                    || reached_known_end
                    || bytes_since_persist >= PROGRESS_PERSIST_BYTES
                    || last_persisted_at.elapsed() >= PROGRESS_PERSIST_INTERVAL;

                if should_persist {
                    storage
                        .update_progress(
                            &progress_id,
                            progress.downloaded_bytes,
                            progress.total_bytes,
                        )
                        .map_err(|error| DownloadError::ProgressCallback(error.to_string()))?;

                    has_persisted_progress = true;
                    last_persisted_at = Instant::now();
                    last_persisted_bytes = progress.downloaded_bytes;
                }

                let bytes_per_second = meter.sample(progress.downloaded_bytes, Instant::now());

                on_progress(
                    &progress_id,
                    TransferProgress {
                        downloaded_bytes: progress.downloaded_bytes,
                        total_bytes: progress.total_bytes,
                        bytes_per_second,
                        eta_seconds: meter
                            .eta_seconds(progress.downloaded_bytes, progress.total_bytes),
                    },
                );

                Ok(())
            })
            .await;

        match result {
            Ok(outcome) => {
                self.storage.mark_finalizing(download_id)?;
                self.complete_download(download_id, outcome)
            }
            Err(error) => {
                let message = error.redacted_message();

                self.storage
                    .mark_failed(download_id, "download_error", &message)?;

                Err(DownloadServiceError::Download(error))
            }
        }
    }

    fn get_task(&self, download_id: &str) -> Result<DownloadRecord> {
        self.storage
            .get_download(download_id)?
            .ok_or_else(|| StorageError::DownloadNotFound(download_id.to_owned()))
            .map_err(DownloadServiceError::Storage)
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
    async fn creates_task_without_contacting_the_source() {
        let root = tempdir().unwrap();
        let database_path = root.path().join("downloads.db");
        let storage = Arc::new(Storage::open(&database_path).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();

        let record = service
            .create_task("http://127.0.0.1:1/source-is-offline")
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Created);
        assert_eq!(record.downloaded_bytes, 0);
        assert!(record.started_at.is_none());
        assert_eq!(storage.list_downloads().unwrap().len(), 1);
    }

    #[test]
    fn rejects_non_http_task_without_persisting_it() {
        let root = tempdir().unwrap();
        let database_path = root.path().join("downloads.db");
        let storage = Arc::new(Storage::open(&database_path).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();

        let error = service.create_task("file:///private/file.bin").unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::Download(DownloadError::UnsupportedScheme(_))
        ));
        assert!(storage.list_downloads().unwrap().is_empty());
    }

    #[tokio::test]
    async fn starts_existing_task_with_stable_id_and_no_duplicate_record() {
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

        let created = service
            .create_task(&format!("http://{address}/download"))
            .unwrap();

        let record = service
            .start_task(&created.id, &downloads_path)
            .await
            .unwrap();

        server.await.unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(record.id, created.id);
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

        let downloads = storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
    }

    #[test]
    fn rejects_duplicate_start_claim_with_typed_transition_error() {
        let root = tempdir().unwrap();
        let database_path = root.path().join("downloads.db");
        let storage = Arc::new(Storage::open(&database_path).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();

        let created = service.create_task("https://example.com/file.bin").unwrap();

        let claimed = service.claim_task(&created.id).unwrap();
        assert_eq!(claimed.status, DownloadStatus::Probing);

        let error = service.claim_task(&created.id).unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::Storage(StorageError::InvalidStatusTransition {
                from: DownloadStatus::Probing,
                to: DownloadStatus::Probing,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn failed_transfer_keeps_the_created_task_id() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request).await.unwrap();

            socket
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            socket.shutdown().await.unwrap();
        });

        let root = tempdir().unwrap();
        let database_path = root.path().join("downloads.db");
        let downloads_path = root.path().join("files");
        let storage = Arc::new(Storage::open(&database_path).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();
        let created = service
            .create_task(&format!("http://{address}/missing"))
            .unwrap();

        let error = service
            .start_task(&created.id, &downloads_path)
            .await
            .unwrap_err();

        server.await.unwrap();

        assert!(matches!(error, DownloadServiceError::Download(_)));

        let downloads = storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
        assert_eq!(downloads[0].status, DownloadStatus::Failed);
        assert_eq!(downloads[0].error_code.as_deref(), Some("download_error"));
        assert_eq!(
            downloads[0].error_message.as_deref(),
            Some("HTTP request failed with status 404 Not Found")
        );
        assert!(
            !downloads[0]
                .error_message
                .as_deref()
                .unwrap()
                .contains(&address.to_string())
        );
    }
}
