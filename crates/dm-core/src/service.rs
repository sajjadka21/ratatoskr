use crate::{
    DownloadError, Downloader, TransferProgress, TransferRequest,
    control::{StopReason, TaskControl},
    resume::{ResumePlan, StoredTransfer, plan_resume},
    retry::{FailureClass, RetryPolicy, classify_failure},
    throughput::ThroughputMeter,
    validate_source_url,
};
use dm_common::{DownloadCompletion, DownloadRecord, TransferPlan};
use dm_storage::{Storage, StorageError};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
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
const INTERRUPTED_ERROR_MESSAGE: &str =
    "Interrupted when the app closed. It will continue from where it stopped.";

/// Recorded when partial bytes had to be thrown away, so a transfer that
/// restarts from zero says why rather than appearing to lose progress.
const RESTARTED_NOTICE_CODE: &str = "restarted";

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

    #[error("download control registry is unavailable")]
    ControlRegistryUnavailable,

    #[error("download {0} is not running")]
    NotRunning(String),
}

pub type Result<T> = std::result::Result<T, DownloadServiceError>;

#[derive(Clone)]
pub struct DownloadService {
    downloader: Downloader,
    storage: Arc<Storage>,
    execution_slots: Arc<Semaphore>,
    /// Handles for transfers running in this process, so pause and cancel
    /// reach the transfer itself instead of only changing a row.
    controls: Arc<Mutex<HashMap<String, Arc<TaskControl>>>>,
    retry_policy: RetryPolicy,
}

impl DownloadService {
    pub fn new(storage: Arc<Storage>) -> Result<Self> {
        Ok(Self {
            downloader: Downloader::new()?,
            storage,
            execution_slots: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_DOWNLOADS)),
            controls: Arc::new(Mutex::new(HashMap::new())),
            retry_policy: RetryPolicy::default(),
        })
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
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

    /// Returns tasks that a previous process left mid-transfer to a state the
    /// user or a queue runner can act on again. Called once during startup,
    /// before any runner is spawned, so no row is ever presented as active
    /// with nobody driving it.
    pub fn recover_orphaned_tasks(&self) -> Result<Vec<DownloadRecord>> {
        self.storage
            .recover_orphaned_downloads(INTERRUPTED_ERROR_CODE, INTERRUPTED_ERROR_MESSAGE)
            .map_err(DownloadServiceError::Storage)
    }

    /// Stops a running transfer and keeps everything it has already written.
    ///
    /// The running transfer persists the pause itself, so the byte count that
    /// is stored is the one actually on disk.
    pub fn pause_task(&self, download_id: &str) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        match self.control_for(download_id)? {
            Some(control) => {
                control.request(StopReason::Pause);
                Ok(task)
            }
            None if task.status.is_executing() => {
                // The row claims to be running but nothing in this process is
                // driving it; persist the pause directly rather than leaving
                // a task nobody can stop.
                self.storage
                    .mark_paused(download_id, task.downloaded_bytes)?;
                self.get_task(download_id)
            }
            None => Err(DownloadServiceError::NotRunning(download_id.to_owned())),
        }
    }

    /// Ends a task and discards its partial transfer. A running transfer
    /// persists the cancellation itself; anything else is cancelled in place.
    pub async fn cancel_task(&self, download_id: &str) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        if let Some(control) = self.control_for(download_id)? {
            control.request(StopReason::Cancel);
            return Ok(task);
        }

        remove_partial_file(task.temp_path.as_deref()).await;
        self.storage.mark_cancelled(download_id)?;
        self.get_task(download_id)
    }

    /// True while this process is transferring the task.
    pub fn is_running(&self, download_id: &str) -> Result<bool> {
        Ok(self.control_for(download_id)?.is_some())
    }

    /// Retrying tasks whose backoff has elapsed.
    pub fn due_retries(&self) -> Result<Vec<DownloadRecord>> {
        Ok(self.storage.list_due_retries(unix_timestamp_seconds()?)?)
    }

    pub async fn start_task(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
    ) -> Result<DownloadRecord> {
        self.start_task_with_progress(download_id, destination_directory, |_, _| {})
            .await
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

        let control = self.register_control(download_id)?;
        let _guard = ControlGuard {
            controls: Arc::clone(&self.controls),
            download_id: download_id.to_owned(),
        };

        let result = self
            .run_transfer(
                &task,
                destination_directory.as_ref(),
                &control,
                &mut on_progress,
            )
            .await;

        match result {
            Ok(outcome) => {
                self.storage.mark_finalizing(download_id)?;
                self.complete_download(download_id, outcome)
            }
            Err(DownloadServiceError::Download(DownloadError::Stopped(reason))) => {
                self.persist_stop(download_id, reason).await
            }
            Err(DownloadServiceError::Download(error)) => {
                self.persist_failure(download_id, &task, error)
            }
            Err(error) => Err(error),
        }
    }

    /// Probes the source, decides what to do with any partial file, and moves
    /// the remaining bytes.
    async fn run_transfer<F>(
        &self,
        task: &DownloadRecord,
        destination_directory: &Path,
        control: &TaskControl,
        on_progress: &mut F,
    ) -> Result<crate::TransferOutcome>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let probe = self.downloader.probe(&task.source_url).await?;

        // Paths are reserved once and then kept, so a resumed task writes to
        // the same partial file rather than starting a second one.
        let (destination_path, temp_path) = match (&task.destination_path, &task.temp_path) {
            (Some(destination), Some(temp)) => (PathBuf::from(destination), PathBuf::from(temp)),
            _ => {
                self.downloader
                    .plan_paths(destination_directory, &probe.filename)
                    .await?
            }
        };

        let stored = StoredTransfer {
            downloaded_bytes: task.downloaded_bytes,
            total_bytes: task.total_bytes,
            etag: task.etag.clone(),
            last_modified: task.last_modified.clone(),
            partial_bytes_on_disk: partial_file_size(&temp_path).await,
        };

        self.storage.set_transfer_plan(
            &task.id,
            &TransferPlan {
                resolved_url: probe.final_url.clone(),
                filename: probe.filename.clone(),
                destination_path: destination_path.to_string_lossy().into_owned(),
                temp_path: temp_path.to_string_lossy().into_owned(),
                mime_type: probe.content_type.clone(),
                total_bytes: probe.total_bytes,
                etag: probe.etag.clone(),
                last_modified: probe.last_modified.clone(),
                range_supported: probe.range_supported,
            },
        )?;

        let start_offset = match plan_resume(&stored, &probe) {
            ResumePlan::ContinueFrom(offset) | ResumePlan::AlreadyComplete(offset) => offset,
            ResumePlan::StartFromZero(reason) => {
                self.storage.reset_transfer_progress(&task.id)?;

                // Only worth explaining when there was something to lose.
                if stored.downloaded_bytes > 0 {
                    self.storage.record_notice(
                        &task.id,
                        RESTARTED_NOTICE_CODE,
                        reason.explanation(),
                    )?;
                }

                0
            }
        };

        self.storage
            .mark_downloading(&task.id, unix_timestamp_seconds()?)?;

        let storage = Arc::clone(&self.storage);
        let progress_id = task.id.clone();
        let reached = Arc::new(AtomicU64::new(start_offset));
        let reached_writer = Arc::clone(&reached);
        let mut last_persisted_at = Instant::now();
        let mut last_persisted_bytes = start_offset;
        let mut has_persisted_progress = false;
        let mut meter = ThroughputMeter::new(Instant::now(), start_offset);

        let outcome = self
            .downloader
            .transfer(
                TransferRequest {
                    source_url: &task.source_url,
                    temp_path: &temp_path,
                    destination_path: &destination_path,
                    start_offset,
                    total_bytes: probe.total_bytes,
                },
                control,
                |progress| {
                    reached_writer.store(progress.downloaded_bytes, Ordering::Relaxed);

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
                },
            )
            .await;

        // Whatever happened, the byte count the transfer actually reached is
        // the one worth keeping.
        let reached_bytes = reached.load(Ordering::Relaxed);

        if outcome.is_err() {
            let _ = self
                .storage
                .update_progress(&task.id, reached_bytes, probe.total_bytes);
        }

        Ok(outcome?)
    }

    /// Persists a stop the user asked for. A pause keeps the partial file; a
    /// cancellation deletes it, because cancelling is terminal.
    async fn persist_stop(&self, download_id: &str, reason: StopReason) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        match reason {
            StopReason::Pause => {
                self.storage
                    .mark_paused(download_id, task.downloaded_bytes)?;
            }
            StopReason::Cancel => {
                remove_partial_file(task.temp_path.as_deref()).await;
                self.storage.mark_cancelled(download_id)?;
            }
        }

        self.get_task(download_id)
    }

    /// Fails the task, or schedules another attempt when the failure is the
    /// kind that tends to pass and the attempt budget still allows it.
    fn persist_failure(
        &self,
        download_id: &str,
        task: &DownloadRecord,
        error: DownloadError,
    ) -> Result<DownloadRecord> {
        let message = error.redacted_message();
        let class = classify_failure(&error);
        let attempts = task.attempts.saturating_add(1);

        if class == FailureClass::Retryable
            && let Some(delay) = self.retry_policy.delay_for(attempts)
        {
            let retry_at = unix_timestamp_seconds()?.saturating_add(delay.as_secs() as i64);

            self.storage.mark_retrying(
                download_id,
                attempts,
                retry_at,
                error_code_for(&error),
                &message,
            )?;

            return Err(DownloadServiceError::Download(error));
        }

        self.storage.record_attempt(download_id, attempts)?;
        self.storage
            .mark_failed(download_id, error_code_for(&error), &message)?;

        Err(DownloadServiceError::Download(error))
    }

    fn register_control(&self, download_id: &str) -> Result<Arc<TaskControl>> {
        let control = Arc::new(TaskControl::new());

        self.controls
            .lock()
            .map_err(|_| DownloadServiceError::ControlRegistryUnavailable)?
            .insert(download_id.to_owned(), Arc::clone(&control));

        Ok(control)
    }

    fn control_for(&self, download_id: &str) -> Result<Option<Arc<TaskControl>>> {
        Ok(self
            .controls
            .lock()
            .map_err(|_| DownloadServiceError::ControlRegistryUnavailable)?
            .get(download_id)
            .map(Arc::clone))
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
        outcome: crate::TransferOutcome,
    ) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        let completion = DownloadCompletion {
            resolved_url: task
                .resolved_url
                .clone()
                .unwrap_or_else(|| task.source_url.clone()),
            filename: task
                .filename
                .clone()
                .or_else(|| {
                    outcome
                        .final_path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "download.bin".to_owned()),
            destination_path: outcome.final_path.to_string_lossy().into_owned(),
            mime_type: task.mime_type.clone(),
            total_bytes: task.total_bytes.or(Some(outcome.downloaded_bytes)),
            downloaded_bytes: outcome.downloaded_bytes,
        };

        self.storage
            .mark_completed(download_id, &completion, unix_timestamp_seconds()?)?;

        self.get_task(download_id)
    }
}

/// Unregisters the control handle when a transfer ends, however it ends.
struct ControlGuard {
    controls: Arc<Mutex<HashMap<String, Arc<TaskControl>>>>,
    download_id: String,
}

impl Drop for ControlGuard {
    fn drop(&mut self) {
        if let Ok(mut controls) = self.controls.lock() {
            controls.remove(&self.download_id);
        }
    }
}

async fn partial_file_size(temp_path: &Path) -> Option<u64> {
    tokio::fs::metadata(temp_path)
        .await
        .ok()
        .map(|metadata| metadata.len())
}

async fn remove_partial_file(temp_path: Option<&str>) {
    if let Some(path) = temp_path {
        let _ = tokio::fs::remove_file(path).await;
    }
}

fn error_code_for(error: &DownloadError) -> &'static str {
    match error {
        DownloadError::InvalidUrl(_) | DownloadError::UnsupportedScheme(_) => "invalid_source",
        DownloadError::Http(_) => "download_error",
        DownloadError::Io(_) => "filesystem_error",
        DownloadError::ProgressCallback(_) => "storage_error",
        DownloadError::IncompleteTransfer { .. } => "incomplete_transfer",
        DownloadError::Stopped(_) => "stopped",
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
    use crate::testing::{DEFAULT_BODY, ServerBehaviour, TestServer};
    use dm_common::DownloadStatus;
    use tempfile::tempdir;
    use tokio::time::sleep;

    struct Harness {
        _root: tempfile::TempDir,
        destination: PathBuf,
        storage: Arc<Storage>,
        service: DownloadService,
    }

    fn harness() -> Harness {
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();
        let destination = root.path().join("files");

        Harness {
            _root: root,
            destination,
            storage,
            service,
        }
    }

    /// A body slow enough to be paused or cancelled part-way through.
    fn slow_server() -> ServerBehaviour {
        ServerBehaviour {
            chunk_size: 4,
            chunk_delay: Some(Duration::from_millis(25)),
            ..ServerBehaviour::default()
        }
    }

    /// Waits until the partial file on disk actually holds bytes, which is
    /// the only way to know a transfer has moved data rather than merely
    /// having been claimed.
    async fn await_partial_bytes(storage: &Storage, id: &str) -> u64 {
        for _ in 0..400 {
            if let Some(temp) = storage.get_download(id).unwrap().unwrap().temp_path
                && let Ok(metadata) = tokio::fs::metadata(&temp).await
                && metadata.len() > 0
            {
                return metadata.len();
            }

            sleep(Duration::from_millis(5)).await;
        }

        panic!("{id} never wrote anything to its partial file");
    }

    async fn await_status(storage: &Storage, id: &str, expected: DownloadStatus) {
        for _ in 0..400 {
            if storage.get_download(id).unwrap().unwrap().status == expected {
                return;
            }

            sleep(Duration::from_millis(10)).await;
        }

        let actual = storage.get_download(id).unwrap().unwrap().status;
        panic!("{id} never reached {expected}; it is {actual}");
    }

    #[tokio::test]
    async fn creates_task_without_contacting_the_source() {
        let harness = harness();

        let record = harness
            .service
            .create_task("http://127.0.0.1:1/source-is-offline")
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Created);
        assert_eq!(record.downloaded_bytes, 0);
        assert!(record.started_at.is_none());
        assert_eq!(harness.storage.list_downloads().unwrap().len(), 1);
    }

    #[test]
    fn rejects_non_http_task_without_persisting_it() {
        let harness = harness();

        let error = harness
            .service
            .create_task("file:///private/file.bin")
            .unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::Download(DownloadError::UnsupportedScheme(_))
        ));
        assert!(harness.storage.list_downloads().unwrap().is_empty());
    }

    #[tokio::test]
    async fn starts_existing_task_with_stable_id_and_no_duplicate_record() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("download.bin"))
            .unwrap();

        let record = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(record.id, created.id);
        assert_eq!(record.filename.as_deref(), Some("payload.bin"));
        assert_eq!(record.downloaded_bytes, DEFAULT_BODY.len() as u64);

        let bytes = tokio::fs::read(record.destination_path.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(bytes, DEFAULT_BODY);

        let persisted = harness.storage.get_download(&record.id).unwrap().unwrap();
        assert_eq!(persisted.status, DownloadStatus::Completed);
        assert!(persisted.started_at.is_some());
        assert!(persisted.completed_at.is_some());
        assert!(
            persisted.temp_path.is_none(),
            "a finished download no longer has a partial file"
        );

        let downloads = harness.storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
    }

    #[test]
    fn rejects_duplicate_start_claim_with_typed_transition_error() {
        let harness = harness();

        let created = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();

        let claimed = harness.service.claim_task(&created.id).unwrap();
        assert_eq!(claimed.status, DownloadStatus::Probing);

        let error = harness.service.claim_task(&created.id).unwrap_err();

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
    async fn a_permanent_failure_keeps_the_task_id_and_is_not_retried() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((404, "Not Found")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("missing.bin"))
            .unwrap();

        let error = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadServiceError::Download(_)));

        let downloads = harness.storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
        assert_eq!(
            downloads[0].status,
            DownloadStatus::Failed,
            "a 404 will not become a 200 by trying again"
        );
        assert_eq!(downloads[0].error_code.as_deref(), Some("download_error"));
        assert!(
            !downloads[0]
                .error_message
                .as_deref()
                .unwrap()
                .contains(&server.url("missing.bin")),
            "the persisted message must not carry the source URL"
        );
    }

    #[tokio::test]
    async fn a_temporary_failure_schedules_another_attempt() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((503, "Service Unavailable")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("busy.bin"))
            .unwrap();

        let _ = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await;

        let record = harness.storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(record.status, DownloadStatus::Retrying);
        assert_eq!(record.attempts, 1);
        assert!(
            record.retry_at.is_some(),
            "a retrying task must know when it becomes eligible again"
        );
    }

    #[tokio::test]
    async fn the_retry_budget_is_bounded() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((503, "Service Unavailable")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = harness.service.clone().with_retry_policy(RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            maximum_delay: Duration::from_millis(1),
        });

        let created = service.create_task(&server.url("busy.bin")).unwrap();

        let _ = service.start_task(&created.id, &harness.destination).await;
        assert_eq!(
            harness
                .storage
                .get_download(&created.id)
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Retrying
        );

        let _ = service.start_task(&created.id, &harness.destination).await;

        let record = harness.storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(
            record.status,
            DownloadStatus::Failed,
            "the last attempt in the budget fails for good"
        );
        assert_eq!(record.attempts, 2);
    }

    #[tokio::test]
    async fn pausing_keeps_the_partial_file_and_resuming_finishes_the_download() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("slow.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();

        let paused = transfer.await.unwrap().unwrap();

        assert_eq!(paused.status, DownloadStatus::Paused);
        assert!(
            paused.downloaded_bytes > 0,
            "pausing must keep what was already transferred"
        );
        assert!(paused.downloaded_bytes < DEFAULT_BODY.len() as u64);

        let partial = paused.temp_path.clone().unwrap();
        assert!(tokio::fs::try_exists(&partial).await.unwrap());

        let resumed = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(resumed.id, created.id);
        assert_eq!(resumed.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(resumed.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY,
            "a resumed download must produce exactly the original file"
        );
        assert!(
            server.ranged_request_count() >= 2,
            "resuming must ask the server for a range"
        );
    }

    #[tokio::test]
    async fn cancelling_a_running_transfer_discards_its_partial_file() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("slow.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;

        let partial = harness
            .storage
            .get_download(&created.id)
            .unwrap()
            .unwrap()
            .temp_path
            .unwrap();

        harness.service.cancel_task(&created.id).await.unwrap();

        let cancelled = transfer.await.unwrap().unwrap();

        assert_eq!(cancelled.status, DownloadStatus::Cancelled);
        assert_eq!(cancelled.downloaded_bytes, 0);
        assert!(
            !tokio::fs::try_exists(&partial).await.unwrap(),
            "cancelling is terminal, so the partial file goes with it"
        );
    }

    #[tokio::test]
    async fn a_task_that_is_not_running_cannot_be_paused() {
        let harness = harness();

        let created = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();

        let error = harness.service.pause_task(&created.id).unwrap_err();

        assert!(matches!(error, DownloadServiceError::NotRunning(_)));
    }

    #[tokio::test]
    async fn a_source_that_changed_is_downloaded_again_instead_of_appended_to() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("changing.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();
        transfer.await.unwrap().unwrap();

        // The file behind the URL is replaced while the task is paused.
        let replacement = b"a completely different payload of its own".to_vec();
        server.update(|behaviour| {
            behaviour.body = replacement.clone();
            behaviour.etag = Some("\"v2\"".to_owned());
            behaviour.chunk_delay = None;
        });

        let resumed = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(resumed.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(resumed.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            b"a completely different payload of its own",
            "stale bytes must never be spliced into the new content"
        );
        assert_eq!(
            resumed.error_code.as_deref(),
            None,
            "a finished download carries no leftover notice"
        );
    }

    #[tokio::test]
    async fn an_interrupted_task_resumes_from_its_partial_file_after_a_restart() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("interrupted.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();
        transfer.await.unwrap().unwrap();

        // Stand in for a process that died while downloading: the row is left
        // in an executing state with its partial file on disk.
        harness.storage.mark_probing(&created.id).unwrap();

        let recovered = harness.service.recover_orphaned_tasks().unwrap();

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].status, DownloadStatus::Paused);
        assert!(recovered[0].downloaded_bytes > 0);

        let finished = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
    }
}
