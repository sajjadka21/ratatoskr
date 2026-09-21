use crate::{
    DownloadError, DownloadProgress,
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
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub enum QueueRunnerEvent {
    QueueUpdated(QueueRecord),
    TaskProgress {
        download_id: String,
        progress: DownloadProgress,
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
}

impl QueueService {
    pub fn new(storage: Arc<Storage>, downloads: DownloadService) -> Self {
        Self {
            storage,
            downloads,
            active_runners: Arc::new(Mutex::new(HashSet::new())),
        }
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
        Ok(self
            .storage
            .enqueue_download(download_id, queue_id, priority)?)
    }

    pub fn move_task(&self, download_id: &str, queue_id: &str) -> Result<DownloadRecord> {
        Ok(self.storage.move_queued_download(download_id, queue_id)?)
    }

    pub fn remove_task(&self, download_id: &str) -> Result<DownloadRecord> {
        Ok(self.storage.remove_download_from_queue(download_id)?)
    }

    pub fn set_task_priority(
        &self,
        download_id: &str,
        priority: DownloadPriority,
    ) -> Result<DownloadRecord> {
        Ok(self.storage.set_download_priority(download_id, priority)?)
    }

    pub fn reorder_tasks(&self, queue_id: &str, ordered_ids: &[String]) -> Result<()> {
        Ok(self
            .storage
            .reorder_queue_downloads(queue_id, ordered_ids)?)
    }

    pub fn stop_queue(&self, queue_id: &str) -> Result<QueueRecord> {
        Ok(self.storage.set_queue_state(
            queue_id,
            QueueState::Stopped,
            unix_timestamp_seconds()?,
        )?)
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

            if queue.state == QueueState::Running {
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

                if self.storage.list_queued_downloads(queue_id)?.is_empty() {
                    queue = self.storage.set_queue_state(
                        queue_id,
                        QueueState::Stopped,
                        unix_timestamp_seconds()?,
                    )?;
                    on_event(QueueRunnerEvent::QueueUpdated(queue.clone()));
                    return Ok(queue);
                }
            }

            if let Some(result) = tasks.join_next().await {
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
    use dm_common::{DownloadPriority, DownloadStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
        time::{Duration, sleep},
    };

    #[tokio::test]
    async fn queue_runner_preserves_ids_and_honors_per_host_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let server = spawn_test_server(listener, 2, Arc::clone(&active), Arc::clone(&maximum));
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let downloads = DownloadService::new(Arc::clone(&storage)).unwrap();
        let queues = QueueService::new(Arc::clone(&storage), downloads.clone());
        let queue = queues
            .create_queue("Host limited", 2, Some(1), DownloadPriority::Normal)
            .unwrap();
        let first = downloads
            .create_task(&format!("http://{address}/first.bin"))
            .unwrap();
        let second = downloads
            .create_task(&format!("http://{address}/second.bin"))
            .unwrap();
        queues.enqueue_task(&first.id, &queue.id, None).unwrap();
        queues.enqueue_task(&second.id, &queue.id, None).unwrap();

        assert!(
            storage
                .list_downloads()
                .unwrap()
                .iter()
                .all(|task| task.status == DownloadStatus::Queued)
        );

        let stopped = queues
            .run_queue(&queue.id, root.path().join("files"), |_| {})
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(stopped.state, QueueState::Stopped);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
        let records = storage.list_downloads().unwrap();
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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let server = spawn_test_server(listener, 2, Arc::clone(&active), Arc::clone(&maximum));
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let downloads = DownloadService::new(Arc::clone(&storage)).unwrap();
        let queues = QueueService::new(Arc::clone(&storage), downloads.clone());
        let queue = queues
            .create_queue("Serial", 1, None, DownloadPriority::Normal)
            .unwrap();

        for path in ["first.bin", "second.bin"] {
            let task = downloads
                .create_task(&format!("http://{address}/{path}"))
                .unwrap();
            queues.enqueue_task(&task.id, &queue.id, None).unwrap();
        }

        queues
            .run_queue(&queue.id, root.path().join("files"), |_| {})
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stopping_queue_prevents_the_next_task_from_starting() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request).await.unwrap();
            accepted_tx.send(()).unwrap();
            release_rx.await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndone")
                .await
                .unwrap();
            socket.shutdown().await.unwrap();
        });
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let downloads = DownloadService::new(Arc::clone(&storage)).unwrap();
        let queues = QueueService::new(Arc::clone(&storage), downloads.clone());
        let queue = queues
            .create_queue("Stoppable", 1, None, DownloadPriority::Normal)
            .unwrap();

        for path in ["first.bin", "second.bin"] {
            let task = downloads
                .create_task(&format!("http://{address}/{path}"))
                .unwrap();
            queues.enqueue_task(&task.id, &queue.id, None).unwrap();
        }

        let runner_service = queues.clone();
        let queue_id = queue.id.clone();
        let destination = root.path().join("files");
        let runner = tokio::spawn(async move {
            runner_service
                .run_queue(&queue_id, destination, |_| {})
                .await
                .unwrap()
        });

        accepted_rx.await.unwrap();
        let stopped = queues.stop_queue(&queue.id).unwrap();
        assert_eq!(stopped.state, QueueState::Stopped);
        release_tx.send(()).unwrap();
        server.await.unwrap();
        assert_eq!(runner.await.unwrap().state, QueueState::Stopped);

        let records = storage.list_downloads().unwrap();
        assert_eq!(
            records
                .iter()
                .filter(|task| task.status == DownloadStatus::Completed)
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|task| task.status == DownloadStatus::Queued)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn concurrent_queues_share_the_global_download_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let server = spawn_test_server(listener, 4, Arc::clone(&active), Arc::clone(&maximum));
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let downloads = DownloadService::new(Arc::clone(&storage)).unwrap();
        let queues = QueueService::new(Arc::clone(&storage), downloads.clone());
        let first_queue = queues
            .create_queue("First", 3, None, DownloadPriority::Normal)
            .unwrap();
        let second_queue = queues
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
            let task = downloads
                .create_task(&format!("http://{address}/{index}.bin"))
                .unwrap();
            queues.enqueue_task(&task.id, queue_id, None).unwrap();
        }

        let first_runner = queues.clone();
        let second_runner = queues.clone();
        let first_destination = root.path().join("first-files");
        let second_destination = root.path().join("second-files");
        let (first_result, second_result) = tokio::join!(
            first_runner.run_queue(&first_queue.id, first_destination, |_| {}),
            second_runner.run_queue(&second_queue.id, second_destination, |_| {})
        );
        first_result.unwrap();
        second_result.unwrap();
        server.await.unwrap();

        assert_eq!(maximum.load(Ordering::SeqCst), 3);
        assert!(
            storage
                .list_downloads()
                .unwrap()
                .iter()
                .all(|task| task.status == DownloadStatus::Completed)
        );
    }

    fn spawn_test_server(
        listener: TcpListener,
        request_count: usize,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut handlers = Vec::new();
            for _ in 0..request_count {
                let (mut socket, _) = listener.accept().await.unwrap();
                let active = Arc::clone(&active);
                let maximum = Arc::clone(&maximum);
                handlers.push(tokio::spawn(async move {
                    let mut request = [0_u8; 2048];
                    let _ = socket.read(&mut request).await.unwrap();
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(current, Ordering::SeqCst);
                    sleep(Duration::from_millis(75)).await;
                    let body = b"queue test";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.write_all(body).await.unwrap();
                    socket.shutdown().await.unwrap();
                    active.fetch_sub(1, Ordering::SeqCst);
                }));
            }
            for handler in handlers {
                handler.await.unwrap();
            }
        })
    }
}
