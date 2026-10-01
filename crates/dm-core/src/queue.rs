use crate::{
    DownloadError, TransferProgress,
    service::{DownloadService, DownloadServiceError},
    validate_source_url,
};
use dm_common::{DownloadPriority, DownloadRecord, QueueRecord, QueueState};
use dm_storage::{Storage, StorageError};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::{sync::Notify, task::JoinSet};

#[derive(Debug, Clone)]
pub enum QueueRunnerEvent {
    QueueUpdated(QueueRecord),
    TaskProgress {
        download_id: String,
        progress: TransferProgress,
    },
    TaskUpdated(Box<DownloadRecord>),
}

#[derive(Debug, Error)]
pub enum QueueServiceError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("download service error: {0}")]
    Download(#[from] DownloadServiceError),

    #[error("invalid queued source: {0}")]
    InvalidSource(#[from] DownloadError),

    #[error("queue runner is already active: {0}")]
    AlreadyRunning(String),

    #[error("queue is disabled: {0}")]
    QueueDisabled(String),

    #[error("queue runner registry is unavailable")]
    RunnerRegistryUnavailable,

    #[error("queue task failed to join: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("system clock is before Unix epoch")]
    ClockBeforeUnixEpoch,

    #[error("timestamp does not fit in SQLite INTEGER")]
    TimestampOverflow,
}

pub type Result<T> = std::result::Result<T, QueueServiceError>;

#[derive(Clone)]
pub struct QueueService {
    storage: Arc<Storage>,
    downloads: DownloadService,
    active_runners: Arc<Mutex<HashSet<String>>>,
    wakeups: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl QueueService {
    pub fn new(storage: Arc<Storage>, downloads: DownloadService) -> Self {
        Self {
            storage,
            downloads,
            active_runners: Arc::new(Mutex::new(HashSet::new())),
            wakeups: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// True while this process is running the queue. Starting an already
    /// running queue must not spawn a second runner, and the caller needs to
    /// be able to tell the difference rather than discovering it through a
    /// failure inside a detached task.
    pub fn is_running(&self, queue_id: &str) -> Result<bool> {
        Ok(self
            .active_runners
            .lock()
            .map_err(|_| QueueServiceError::RunnerRegistryUnavailable)?
            .contains(queue_id))
    }

    /// Wakes a running runner so it can fill free slots immediately. Without
    /// this the runner would only look for new work when a transfer finished,
    /// so a queue with free slots would run newly added tasks one at a time.
    fn wake(&self, queue_id: &str) -> Result<()> {
        let wakeups = self
            .wakeups
            .lock()
            .map_err(|_| QueueServiceError::RunnerRegistryUnavailable)?;

        if let Some(notify) = wakeups.get(queue_id) {
            notify.notify_one();
        }

        Ok(())
    }

    fn wakeup_handle(&self, queue_id: &str) -> Result<Arc<Notify>> {
        let mut wakeups = self
            .wakeups
            .lock()
            .map_err(|_| QueueServiceError::RunnerRegistryUnavailable)?;

        Ok(Arc::clone(
            wakeups
                .entry(queue_id.to_owned())
                .or_insert_with(|| Arc::new(Notify::new())),
        ))
    }

    pub fn list_queues(&self) -> Result<Vec<QueueRecord>> {
        Ok(self.storage.list_queues()?)
    }

    pub fn create_queue(
        &self,
        name: &str,
        max_concurrent: u32,
        max_concurrent_per_host: Option<u32>,
        default_priority: DownloadPriority,
    ) -> Result<QueueRecord> {
        Ok(self.storage.create_queue(
            name,
            max_concurrent,
            max_concurrent_per_host,
            default_priority,
            unix_timestamp_seconds()?,
        )?)
    }

    pub fn enqueue_task(
        &self,
        download_id: &str,
        queue_id: &str,
        priority: Option<DownloadPriority>,
    ) -> Result<DownloadRecord> {
        let record = self
            .storage
            .enqueue_download(download_id, queue_id, priority)?;
        self.wake(queue_id)?;

        Ok(record)
    }

    pub fn move_task(&self, download_id: &str, queue_id: &str) -> Result<DownloadRecord> {
        let record = self.storage.move_queued_download(download_id, queue_id)?;
        self.wake(queue_id)?;

        Ok(record)
    }

    pub fn remove_task(&self, download_id: &str) -> Result<DownloadRecord> {
        Ok(self.storage.remove_download_from_queue(download_id)?)
    }

    pub fn set_task_priority(
        &self,
        download_id: &str,
        priority: DownloadPriority,
    ) -> Result<DownloadRecord> {
        let record = self.storage.set_download_priority(download_id, priority)?;

        if let Some(queue_id) = record.queue_id.as_deref() {
            self.wake(queue_id)?;
        }

        Ok(record)
    }

    pub fn reorder_tasks(&self, queue_id: &str, ordered_ids: &[String]) -> Result<()> {
        self.storage
            .reorder_queue_downloads(queue_id, ordered_ids)?;
        self.wake(queue_id)
    }

    pub fn set_queue_enabled(&self, queue_id: &str, enabled: bool) -> Result<QueueRecord> {
        let queue = self
            .storage
            .set_queue_enabled(queue_id, enabled, unix_timestamp_seconds()?)?;
        self.wake(queue_id)?;

        Ok(queue)
    }

    pub fn stop_queue(&self, queue_id: &str) -> Result<QueueRecord> {
        let queue = self.storage.set_queue_state(
            queue_id,
            QueueState::Stopped,
            unix_timestamp_seconds()?,
        )?;
        // Wake the runner so an idle queue exits immediately instead of
        // waiting for a transfer that may never come.
        self.wake(queue_id)?;

        Ok(queue)
    }

    pub fn start_queue(&self, queue_id: &str) -> Result<QueueRecord> {
        let queue = self
            .storage
            .get_queue(queue_id)?
            .ok_or_else(|| StorageError::QueueNotFound(queue_id.to_owned()))?;

        if !queue.enabled {
            return Err(QueueServiceError::QueueDisabled(queue_id.to_owned()));
        }

        let queue = self.storage.set_queue_state(
            queue_id,
            QueueState::Running,
            unix_timestamp_seconds()?,
        )?;
        self.wake(queue_id)?;

        Ok(queue)
    }

    pub async fn run_queue<F>(
        &self,
        queue_id: &str,
        destination_directory: impl AsRef<Path>,
        on_event: F,
    ) -> Result<QueueRecord>
    where
        F: Fn(QueueRunnerEvent) + Send + Sync + 'static,
    {
        let _runner_guard =
            ActiveRunnerGuard::acquire(Arc::clone(&self.active_runners), queue_id.to_owned())?;
        let destination_directory = destination_directory.as_ref().to_path_buf();
        let on_event = Arc::new(on_event);
        let wakeup = self.wakeup_handle(queue_id)?;
        let mut queue = self.storage.set_queue_state(
            queue_id,
            QueueState::Running,
            unix_timestamp_seconds()?,
        )?;
        on_event(QueueRunnerEvent::QueueUpdated(queue.clone()));

        let mut tasks = JoinSet::new();
        let mut active_hosts: HashMap<String, usize> = HashMap::new();

        loop {
            queue = self
                .storage
                .get_queue(queue_id)?
                .ok_or_else(|| StorageError::QueueNotFound(queue_id.to_owned()))?;

            if queue.is_schedulable() {
                self.fill_available_slots(
                    &queue,
                    &destination_directory,
                    &on_event,
                    &mut tasks,
                    &mut active_hosts,
                )?;
            }

            if tasks.is_empty() {
                if queue.state == QueueState::Stopped {
                    return Ok(queue);
                }

                if !queue.is_schedulable()
                    || self.storage.list_queued_downloads(queue_id)?.is_empty()
                {
                    queue = self.storage.set_queue_state(
                        queue_id,
                        QueueState::Stopped,
                        unix_timestamp_seconds()?,
                    )?;
                    on_event(QueueRunnerEvent::QueueUpdated(queue.clone()));
                    return Ok(queue);
                }
            }

            // Two things can free or create work: a transfer finishing, and a
            // task being added, moved or reprioritised while other transfers
            // are still running. Waiting on both keeps the configured
            // concurrency real instead of only refilling slots on completion.
            let finished = tokio::select! {
                result = tasks.join_next(), if !tasks.is_empty() => result,
                () = wakeup.notified() => None,
            };

            if let Some(result) = finished {
                let (download_id, host, execution_result) = result?;
                decrement_host_count(&mut active_hosts, &host);

                let record = match execution_result {
                    Ok(record) => record,
                    Err(_) => self
                        .storage
                        .get_download(&download_id)?
                        .ok_or_else(|| StorageError::DownloadNotFound(download_id.clone()))?,
                };
                on_event(QueueRunnerEvent::TaskUpdated(Box::new(record)));
            }
        }
    }

    fn fill_available_slots<F>(
        &self,
        queue: &QueueRecord,
        destination_directory: &Path,
        on_event: &Arc<F>,
        tasks: &mut JoinSet<(
            String,
            String,
            std::result::Result<DownloadRecord, DownloadServiceError>,
        )>,
        active_hosts: &mut HashMap<String, usize>,
    ) -> Result<()>
    where
        F: Fn(QueueRunnerEvent) + Send + Sync + 'static,
    {
        while tasks.len() < queue.max_concurrent as usize {
            let queued = self.storage.list_queued_downloads(&queue.id)?;
            let candidate = queued
                .into_iter()
                .map(|record| {
                    let host = source_host(&record.source_url)?;
                    Ok((record, host))
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .find(|(_, host)| {
                    queue.max_concurrent_per_host.is_none_or(|cap| {
                        active_hosts.get(host).copied().unwrap_or(0) < cap as usize
                    })
                });

            let Some((record, host)) = candidate else {
                break;
            };

            let claimed = self.storage.claim_queued_download(&record.id, &queue.id)?;
            on_event(QueueRunnerEvent::TaskUpdated(Box::new(claimed)));
            *active_hosts.entry(host.clone()).or_default() += 1;

            let downloads = self.downloads.clone();
            let destination = PathBuf::from(destination_directory);
            let callback = Arc::clone(on_event);
            let download_id = record.id;

            tasks.spawn(async move {
                let result = downloads
                    .execute_claimed_task_with_progress(
                        &download_id,
                        destination,
                        |id, progress| {
                            callback(QueueRunnerEvent::TaskProgress {
                                download_id: id.to_owned(),
                                progress,
                            });
                        },
                    )
                    .await;

                (download_id, host, result)
            });
        }

        Ok(())
    }
}

fn source_host(source_url: &str) -> Result<String> {
    let parsed = validate_source_url(source_url)?;
    Ok(parsed
        .host_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "unknown-host".to_owned()))
}

fn decrement_host_count(active_hosts: &mut HashMap<String, usize>, host: &str) {
    if let Some(count) = active_hosts.get_mut(host) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            active_hosts.remove(host);
        }
    }
}

fn unix_timestamp_seconds() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| QueueServiceError::ClockBeforeUnixEpoch)?;
    i64::try_from(duration.as_secs()).map_err(|_| QueueServiceError::TimestampOverflow)
}

struct ActiveRunnerGuard {
    active_runners: Arc<Mutex<HashSet<String>>>,
    queue_id: String,
}

impl ActiveRunnerGuard {
    fn acquire(active_runners: Arc<Mutex<HashSet<String>>>, queue_id: String) -> Result<Self> {
        let inserted = active_runners
            .lock()
            .map_err(|_| QueueServiceError::RunnerRegistryUnavailable)?
            .insert(queue_id.clone());

        if !inserted {
            return Err(QueueServiceError::AlreadyRunning(queue_id));
        }

        Ok(Self {
            active_runners,
            queue_id,
        })
    }
}

impl Drop for ActiveRunnerGuard {
    fn drop(&mut self) {
        if let Ok(mut active_runners) = self.active_runners.lock() {
            active_runners.remove(&self.queue_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ServerBehaviour, TestServer};
    use dm_common::{DownloadPriority, DownloadStatus};
    use tempfile::tempdir;
    use tokio::time::{Duration, sleep};

    /// Long enough that transfers overlap observably, short enough to keep the
    /// suite quick.
    fn slow_server() -> ServerBehaviour {
        ServerBehaviour {
            chunk_size: 4,
            chunk_delay: Some(Duration::from_millis(25)),
            ..ServerBehaviour::default()
        }
    }

    struct Harness {
        _root: tempfile::TempDir,
        destination: PathBuf,
        storage: Arc<Storage>,
        downloads: DownloadService,
        queues: QueueService,
    }

    fn harness() -> Harness {
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let downloads = DownloadService::new(Arc::clone(&storage)).unwrap();
        let queues = QueueService::new(Arc::clone(&storage), downloads.clone());
        let destination = root.path().join("files");

        Harness {
            _root: root,
            destination,
            storage,
            downloads,
            queues,
        }
    }

    async fn await_status(storage: &Storage, download_id: &str, expected: DownloadStatus) {
        for _ in 0..400 {
            if storage.get_download(download_id).unwrap().unwrap().status == expected {
                return;
            }

            sleep(Duration::from_millis(10)).await;
        }

        let actual = storage.get_download(download_id).unwrap().unwrap().status;
        panic!("{download_id} never reached {expected}; it is {actual}");
    }

    /// Waits until a queued task has actually been claimed by its runner.
    async fn await_started(storage: &Storage, download_id: &str) {
        for _ in 0..400 {
            if storage.get_download(download_id).unwrap().unwrap().status != DownloadStatus::Queued
            {
                return;
            }

            sleep(Duration::from_millis(10)).await;
        }

        panic!("{download_id} was never claimed by its queue runner");
    }

    #[tokio::test]
    async fn queue_runner_preserves_ids_and_honors_per_host_limit() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let queue = harness
            .queues
            .create_queue("Host limited", 2, Some(1), DownloadPriority::Normal)
            .unwrap();

        let first = harness
            .downloads
            .create_task(&server.url("first.bin"))
            .unwrap();
        let second = harness
            .downloads
            .create_task(&server.url("second.bin"))
            .unwrap();

        harness
            .queues
            .enqueue_task(&first.id, &queue.id, None)
            .unwrap();
        harness
            .queues
            .enqueue_task(&second.id, &queue.id, None)
            .unwrap();

        assert!(
            harness
                .storage
                .list_downloads()
                .unwrap()
                .iter()
                .all(|task| task.status == DownloadStatus::Queued)
        );

        assert_eq!(
            harness
                .storage
                .get_queue(&queue.id)
                .unwrap()
                .unwrap()
                .max_concurrent_per_host,
            Some(1),
            "the per-host cap must be persisted"
        );

        let stopped = harness
            .queues
            .run_queue(&queue.id, &harness.destination, |_| {})
            .await
            .unwrap();

        assert_eq!(stopped.state, QueueState::Stopped);
        assert_eq!(
            server.peak_concurrent_bodies(),
            1,
            "the per-host cap must hold even though the queue allows two (requests: {}, ranged: {})",
            server.request_count(),
            server.ranged_request_count()
        );

        let records = harness.storage.list_downloads().unwrap();
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|task| task.status == DownloadStatus::Completed)
        );
        assert!(records.iter().any(|task| task.id == first.id));
        assert!(records.iter().any(|task| task.id == second.id));
    }

    #[tokio::test]
    async fn queue_runner_honors_queue_concurrency() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let queue = harness
            .queues
            .create_queue("Serial", 1, None, DownloadPriority::Normal)
            .unwrap();

        for path in ["first.bin", "second.bin"] {
            let task = harness.downloads.create_task(&server.url(path)).unwrap();
            harness
                .queues
                .enqueue_task(&task.id, &queue.id, None)
                .unwrap();
        }

        harness
            .queues
            .run_queue(&queue.id, &harness.destination, |_| {})
            .await
            .unwrap();

        assert_eq!(server.peak_concurrent_bodies(), 1);
        assert!(
            harness
                .storage
                .list_downloads()
                .unwrap()
                .iter()
                .all(|task| task.status == DownloadStatus::Completed)
        );
    }

    #[tokio::test]
    async fn stopping_queue_prevents_the_next_task_from_starting() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let queue = harness
            .queues
            .create_queue("Stoppable", 1, None, DownloadPriority::Normal)
            .unwrap();

        let mut ids = Vec::new();

        for path in ["first.bin", "second.bin"] {
            let task = harness.downloads.create_task(&server.url(path)).unwrap();
            harness
                .queues
                .enqueue_task(&task.id, &queue.id, None)
                .unwrap();
            ids.push(task.id);
        }

        let runner_service = harness.queues.clone();
        let queue_id = queue.id.clone();
        let destination = harness.destination.clone();
        let runner = tokio::spawn(async move {
            runner_service
                .run_queue(&queue_id, destination, |_| {})
                .await
                .unwrap()
        });

        await_status(&harness.storage, &ids[0], DownloadStatus::Downloading).await;

        let stopped = harness.queues.stop_queue(&queue.id).unwrap();
        assert_eq!(stopped.state, QueueState::Stopped);
        assert_eq!(runner.await.unwrap().state, QueueState::Stopped);

        let records = harness.storage.list_downloads().unwrap();

        assert_eq!(
            records
                .iter()
                .filter(|task| task.status == DownloadStatus::Completed)
                .count(),
            1,
            "the transfer already running is allowed to finish"
        );
        assert_eq!(
            records
                .iter()
                .filter(|task| task.status == DownloadStatus::Queued)
                .count(),
            1,
            "a stopped queue starts nothing new"
        );
    }

    #[tokio::test]
    async fn concurrent_queues_share_the_global_download_limit() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let first_queue = harness
            .queues
            .create_queue("First", 3, None, DownloadPriority::Normal)
            .unwrap();
        let second_queue = harness
            .queues
            .create_queue("Second", 3, None, DownloadPriority::Normal)
            .unwrap();

        for (index, queue_id) in [
            &first_queue.id,
            &first_queue.id,
            &second_queue.id,
            &second_queue.id,
        ]
        .into_iter()
        .enumerate()
        {
            let task = harness
                .downloads
                .create_task(&server.url(&format!("{index}.bin")))
                .unwrap();
            harness
                .queues
                .enqueue_task(&task.id, queue_id, None)
                .unwrap();
        }

        let first_runner = harness.queues.clone();
        let second_runner = harness.queues.clone();
        let first_destination = harness.destination.join("first");
        let second_destination = harness.destination.join("second");

        let (first_result, second_result) = tokio::join!(
            first_runner.run_queue(&first_queue.id, first_destination, |_| {}),
            second_runner.run_queue(&second_queue.id, second_destination, |_| {})
        );
        first_result.unwrap();
        second_result.unwrap();

        assert_eq!(
            server.peak_concurrent_bodies(),
            3,
            "two queues of three must still share the process-wide limit"
        );

        let records = harness.storage.list_downloads().unwrap();
        assert!(
            records
                .iter()
                .all(|task| task.status == DownloadStatus::Completed),
            "not every task completed: {:?}",
            records
                .iter()
                .map(|task| (task.status, task.error_message.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn a_running_queue_fills_free_slots_with_newly_added_work() {
        let body_gate = Arc::new(crate::testing::BodyGate::default());
        let server = TestServer::start(ServerBehaviour {
            body_gate: Some(Arc::clone(&body_gate)),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let queue = harness
            .queues
            .create_queue("Refilled", 2, None, DownloadPriority::Normal)
            .unwrap();

        let first = harness
            .downloads
            .create_task(&server.url("first.bin"))
            .unwrap();
        harness
            .queues
            .enqueue_task(&first.id, &queue.id, None)
            .unwrap();

        let runner_service = harness.queues.clone();
        let queue_id = queue.id.clone();
        let destination = harness.destination.clone();
        let runner = tokio::spawn(async move {
            runner_service
                .run_queue(&queue_id, destination, |_| {})
                .await
                .unwrap()
        });

        await_status(&harness.storage, &first.id, DownloadStatus::Downloading).await;
        tokio::time::timeout(Duration::from_secs(10), body_gate.wait_for_body())
            .await
            .expect("the first download never began its response body");

        let second = harness
            .downloads
            .create_task(&server.url("second.bin"))
            .unwrap();
        harness
            .queues
            .enqueue_task(&second.id, &queue.id, None)
            .unwrap();

        // The runner must pick the new task up while the first one is still
        // being transferred, not after it finishes.
        await_started(&harness.storage, &second.id).await;
        tokio::time::timeout(Duration::from_secs(10), body_gate.wait_for_body())
            .await
            .expect("the queue did not refill its free slot while the first body was held");

        assert_ne!(
            harness
                .storage
                .get_download(&first.id)
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Completed,
            "the second task must start while the first is still transferring"
        );

        // Both actual bodies are held here, so task finalization and CI load
        // cannot shorten the overlap window before this assertion.
        assert_eq!(server.peak_concurrent_bodies(), 2);
        body_gate.release();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), runner)
                .await
                .expect("the queue did not finish after both bodies were released")
                .unwrap()
                .state,
            QueueState::Stopped
        );
        assert!(
            harness
                .storage
                .list_downloads()
                .unwrap()
                .iter()
                .all(|task| task.status == DownloadStatus::Completed)
        );
    }

    #[tokio::test]
    async fn a_disabled_queue_neither_starts_nor_schedules() {
        let harness = harness();
        let queue = harness
            .queues
            .create_queue("Disabled", 2, None, DownloadPriority::Normal)
            .unwrap();
        let task = harness
            .downloads
            .create_task("http://127.0.0.1:1/never-requested.bin")
            .unwrap();
        harness
            .queues
            .enqueue_task(&task.id, &queue.id, None)
            .unwrap();
        harness.queues.set_queue_enabled(&queue.id, false).unwrap();

        let error = harness.queues.start_queue(&queue.id).unwrap_err();
        assert!(matches!(error, QueueServiceError::QueueDisabled(_)));

        let stopped = harness
            .queues
            .run_queue(&queue.id, &harness.destination, |_| {})
            .await
            .unwrap();

        assert_eq!(stopped.state, QueueState::Stopped);
        assert_eq!(
            harness
                .storage
                .get_download(&task.id)
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Queued,
            "a disabled queue must not hand work to its runner"
        );
    }
}
