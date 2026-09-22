use dm_common::{DownloadPriority, DownloadRecord, QueueRecord, QueueState};
use dm_core::{
    queue::{QueueRunnerEvent, QueueService},
    service::DownloadService,
    CoreService, TransferProgress,
};
use dm_ipc::{
    AppInfoResponse, ComponentHealth, DownloadListItemResponse, DownloadTaskEvent,
    HealthCheckResponse, QueueResponse, QueueRunnerEventResponse, TransferProgressResponse,
};
use dm_storage::Storage;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(100);

/// How often pending retries are checked. Retry delays are measured in
/// seconds, so a coarse poll costs nothing and keeps the app idle.
const RETRY_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Application-level event names. Runner output is published to the whole
/// window rather than through a per-invoke channel, so work that a command did
/// not start — a queue resumed during startup, for example — still reaches the
/// UI instead of running invisibly.
const DOWNLOAD_TASK_EVENT: &str = "download-task-event";
const QUEUE_RUNNER_EVENT: &str = "queue-runner-event";

pub struct AppState {
    core: CoreService,
    storage: Arc<Storage>,
    downloads: DownloadService,
    queues: QueueService,
}

/// Publishes engine events to the UI and enforces the per-task event rate.
/// Throttling lives here, not in the engine, because it is a presentation
/// concern: the engine measures every chunk, the window only needs ten updates
/// a second.
#[derive(Clone)]
struct EventPublisher {
    app: AppHandle,
    last_sent: Arc<Mutex<HashMap<String, Instant>>>,
}

impl EventPublisher {
    fn new(app: AppHandle) -> Self {
        Self {
            app,
            last_sent: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// True when a progress update for this task is due. A transfer that has
    /// reached its known total always reports, so the final byte count is
    /// never dropped by the rate limit.
    fn should_publish_progress(&self, download_id: &str, progress: &TransferProgress) -> bool {
        let reached_known_end = progress.total_bytes == Some(progress.downloaded_bytes);
        let Ok(mut last_sent) = self.last_sent.lock() else {
            return false;
        };

        match last_sent.get(download_id) {
            Some(sent_at) if !reached_known_end && sent_at.elapsed() < PROGRESS_EVENT_INTERVAL => {
                false
            }
            _ => {
                last_sent.insert(download_id.to_owned(), Instant::now());
                true
            }
        }
    }

    fn forget(&self, download_id: &str) {
        if let Ok(mut last_sent) = self.last_sent.lock() {
            last_sent.remove(download_id);
        }
    }

    fn emit<T: serde::Serialize + Clone>(&self, name: &str, payload: T) {
        if let Err(error) = self.app.emit(name, payload) {
            warn!(event = name, error = %error, "failed to publish engine event");
        }
    }

    fn download_progress(&self, download_id: &str, progress: TransferProgress) {
        if !self.should_publish_progress(download_id, &progress) {
            return;
        }

        self.emit(
            DOWNLOAD_TASK_EVENT,
            DownloadTaskEvent::progress(download_id, transfer_progress_response(progress)),
        );
    }

    fn download_updated(&self, record: DownloadRecord) {
        self.forget(&record.id);
        self.emit(
            DOWNLOAD_TASK_EVENT,
            DownloadTaskEvent::updated(download_list_item_response(record)),
        );
    }

    /// Publishes whatever the row says now. Used when a transfer ended in a
    /// state the caller does not hold, such as a failure that scheduled a
    /// retry.
    fn download_refreshed(&self, download_id: &str) {
        let Some(state) = self.app.try_state::<AppState>() else {
            return;
        };

        if let Ok(Some(record)) = state.storage.get_download(download_id) {
            self.download_updated(record);
        }
    }

    fn queue_event(&self, queue_id: &str, event: QueueRunnerEvent) {
        let response = match event {
            QueueRunnerEvent::QueueUpdated(queue) => {
                QueueRunnerEventResponse::queue_updated(queue_response(queue))
            }
            QueueRunnerEvent::TaskUpdated(download) => {
                self.forget(&download.id);
                QueueRunnerEventResponse::task_updated(
                    queue_id,
                    download_list_item_response(*download),
                )
            }
            QueueRunnerEvent::TaskProgress {
                download_id,
                progress,
            } => {
                if !self.should_publish_progress(&download_id, &progress) {
                    return;
                }

                QueueRunnerEventResponse::task_progress(
                    queue_id,
                    download_id,
                    transfer_progress_response(progress),
                )
            }
        };

        self.emit(QUEUE_RUNNER_EVENT, response);
    }
}

fn transfer_progress_response(progress: TransferProgress) -> TransferProgressResponse {
    TransferProgressResponse {
        downloaded_bytes: progress.downloaded_bytes,
        total_bytes: progress.total_bytes,
        bytes_per_second: progress.bytes_per_second,
        eta_seconds: progress.eta_seconds,
        active_connections: progress.active_connections,
        max_connections: progress.max_connections,
        adaptive_reason: progress.adaptive_reason.map(str::to_owned),
    }
}

/// Starts a queue runner in the background and publishes everything it does.
/// Used both by the start command and by startup resume, so neither path can
/// end up running silently.
fn spawn_queue_runner(
    queues: QueueService,
    publisher: EventPublisher,
    queue_id: String,
    destination_directory: std::path::PathBuf,
) {
    tauri::async_runtime::spawn(async move {
        let events_queue_id = queue_id.clone();
        let result = queues
            .run_queue(&queue_id, destination_directory, move |event| {
                publisher.queue_event(&events_queue_id, event);
            })
            .await;

        match result {
            Ok(_) => info!(queue_id = %queue_id, "queue runner stopped"),
            Err(error) => warn!(queue_id = %queue_id, error = %error, "queue runner failed"),
        }
    });
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .try_init();
}

#[tauri::command]
fn get_app_info() -> AppInfoResponse {
    AppInfoResponse {
        name: "Download Manager".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
fn health_check(state: State<'_, AppState>) -> HealthCheckResponse {
    let core = if state.core.health_check() {
        ComponentHealth::ready()
    } else {
        ComponentHealth::error("core unavailable")
    };

    let storage = ComponentHealth::ready();

    let database = match state.storage.health_check() {
        Ok(()) => ComponentHealth::ready(),
        Err(error) => ComponentHealth::error(error.to_string()),
    };

    HealthCheckResponse {
        core,
        storage,
        database,
    }
}

#[tauri::command]
fn list_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadListItemResponse>, String> {
    let downloads = state
        .storage
        .list_downloads()
        .map_err(|error| error.to_string())?;

    Ok(downloads
        .into_iter()
        .map(download_list_item_response)
        .collect())
}

fn download_list_item_response(record: DownloadRecord) -> DownloadListItemResponse {
    DownloadListItemResponse {
        id: record.id,
        source_url: record.source_url,
        resolved_url: record.resolved_url,
        filename: record.filename,
        destination_path: record.destination_path,
        mime_type: record.mime_type,
        etag: record.etag,
        last_modified: record.last_modified,
        range_supported: record.range_supported,
        total_bytes: record.total_bytes,
        downloaded_bytes: record.downloaded_bytes,
        status: record.status.to_string(),
        queue_id: record.queue_id,
        priority: record.priority.to_string(),
        queue_position: record.queue_position,
        created_at: record.created_at,
        started_at: record.started_at,
        completed_at: record.completed_at,
        attempts: record.attempts,
        retry_at: record.retry_at,
        error_code: record.error_code,
        error_message: record.error_message,
    }
}

fn queue_response(record: QueueRecord) -> QueueResponse {
    QueueResponse {
        id: record.id,
        name: record.name,
        enabled: record.enabled,
        state: record.state.to_string(),
        sort_order: record.sort_order,
        max_concurrent: record.max_concurrent,
        max_concurrent_per_host: record.max_concurrent_per_host,
        default_priority: record.default_priority.to_string(),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn parse_priority(value: &str) -> Result<DownloadPriority, String> {
    DownloadPriority::from_str(value).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_queues(state: State<'_, AppState>) -> Result<Vec<QueueResponse>, String> {
    state
        .queues
        .list_queues()
        .map(|queues| queues.into_iter().map(queue_response).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_queue(
    state: State<'_, AppState>,
    name: String,
    max_concurrent: u32,
    max_concurrent_per_host: Option<u32>,
    default_priority: String,
) -> Result<QueueResponse, String> {
    state
        .queues
        .create_queue(
            &name,
            max_concurrent,
            max_concurrent_per_host,
            parse_priority(&default_priority)?,
        )
        .map(queue_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn enqueue_download_task(
    state: State<'_, AppState>,
    id: String,
    queue_id: String,
    priority: Option<String>,
) -> Result<DownloadListItemResponse, String> {
    let priority = priority.as_deref().map(parse_priority).transpose()?;
    state
        .queues
        .enqueue_task(&id, &queue_id, priority)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn move_queued_download(
    state: State<'_, AppState>,
    id: String,
    queue_id: String,
) -> Result<DownloadListItemResponse, String> {
    state
        .queues
        .move_task(&id, &queue_id)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_download_from_queue(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadListItemResponse, String> {
    state
        .queues
        .remove_task(&id)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_download_priority(
    state: State<'_, AppState>,
    id: String,
    priority: String,
) -> Result<DownloadListItemResponse, String> {
    state
        .queues
        .set_task_priority(&id, parse_priority(&priority)?)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reorder_queue_downloads(
    state: State<'_, AppState>,
    queue_id: String,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    state
        .queues
        .reorder_tasks(&queue_id, &ordered_ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn stop_queue(state: State<'_, AppState>, queue_id: String) -> Result<QueueResponse, String> {
    state
        .queues
        .stop_queue(&queue_id)
        .map(queue_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn start_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    queue_id: String,
) -> Result<QueueResponse, String> {
    let destination_directory = app
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;
    let queue = state
        .queues
        .start_queue(&queue_id)
        .map_err(|error| error.to_string())?;

    // A queue resumed at startup is already running; starting it again must
    // reuse that runner rather than spawn a second one that immediately fails.
    if state
        .queues
        .is_running(&queue_id)
        .map_err(|error| error.to_string())?
    {
        info!(queue_id = %queue_id, "queue runner already active");
        return Ok(queue_response(queue));
    }

    info!(queue_id = %queue_id, "starting queue runner");

    spawn_queue_runner(
        state.queues.clone(),
        EventPublisher::new(app),
        queue_id,
        destination_directory,
    );

    Ok(queue_response(queue))
}

#[tauri::command]
fn set_queue_enabled(
    state: State<'_, AppState>,
    queue_id: String,
    enabled: bool,
) -> Result<QueueResponse, String> {
    state
        .queues
        .set_queue_enabled(&queue_id, enabled)
        .map(queue_response)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn remove_download(
    state: State<'_, AppState>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    let record = state
        .storage
        .get_download(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;

    // Reported early so the user gets a clear reason; the delete itself is
    // guarded in SQL as well, so a runner claiming the task at the same
    // moment still cannot lose its row mid-transfer.
    if !record.status.is_removable() {
        return Err(format!(
            "cannot remove download while status is '{}'",
            record.status
        ));
    }

    if delete_file {
        let destination_path = record
            .destination_path
            .as_deref()
            .ok_or_else(|| "download has no destination file to delete".to_owned())?;

        let path = std::path::Path::new(destination_path);

        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // The file was already removed outside the app.
                // History can still be removed safely.
            }
            Err(error) => {
                return Err(format!("failed to delete downloaded file: {error}"));
            }
        }
    }

    // A paused or cancelled task can still own a partial file; removing the
    // row must not leave it behind.
    if let Some(temp_path) = record.temp_path.as_deref() {
        let _ = std::fs::remove_file(temp_path);
    }

    state
        .storage
        .remove_download_record(&id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn get_add_download_input_mode(state: State<'_, AppState>) -> Result<String, String> {
    let value = state
        .storage
        .get_setting("add_download_input_mode")
        .map_err(|error| error.to_string())?;

    match value.as_deref() {
        Some("manual") => Ok("manual".to_owned()),
        Some("clipboard") | None => Ok("clipboard".to_owned()),
        Some(_) => Ok("clipboard".to_owned()),
    }
}

#[tauri::command]
fn set_add_download_input_mode(state: State<'_, AppState>, mode: String) -> Result<(), String> {
    if !matches!(mode.as_str(), "clipboard" | "manual") {
        return Err(format!("invalid add download input mode: {mode}"));
    }

    state
        .storage
        .set_setting("add_download_input_mode", &mode)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn create_download_task(
    state: State<'_, AppState>,
    url: String,
) -> Result<DownloadListItemResponse, String> {
    let task = state
        .downloads
        .create_task(&url)
        .map_err(|error| error.to_string())?;

    let decision = state
        .downloads
        .rule_decision_for_url(&url)
        .map_err(|error| error.to_string())?;

    if let Some(decision) = decision {
        if let Some(priority) = decision.priority {
            state
                .queues
                .set_task_priority(&task.id, priority)
                .map_err(|error| error.to_string())?;
        }
        if let Some(queue_id) = decision.queue_id {
            return state
                .queues
                .enqueue_task(&task.id, &queue_id, decision.priority)
                .map(download_list_item_response)
                .map_err(|error| error.to_string());
        }
    }

    state
        .storage
        .get_download(&task.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {}", task.id))
        .map(download_list_item_response)
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadListItemResponse, String> {
    let destination_directory = app
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;

    let claimed = state
        .downloads
        .claim_task(&id)
        .map_err(|error| error.to_string())?;
    let response = download_list_item_response(claimed);

    info!(download_id = %id, "starting background download");

    spawn_transfer(
        state.downloads.clone(),
        EventPublisher::new(app),
        id,
        destination_directory,
    );

    Ok(response)
}

/// Stops a running transfer and keeps what it has already written.
#[tauri::command]
fn pause_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadListItemResponse, String> {
    info!(download_id = %id, "pausing download");

    state
        .downloads
        .pause_task(&id)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

/// Continues a task from whatever it already transferred.
///
/// A task that belongs to a queue goes back to its queue so the runner keeps
/// owning it; anything else starts straight away.
#[tauri::command]
fn resume_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadListItemResponse, String> {
    let record = state
        .storage
        .get_download(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;

    if let Some(queue_id) = record.queue_id.clone() {
        info!(download_id = %id, "returning download to its queue");

        return state
            .queues
            .enqueue_task(&id, &queue_id, Some(record.priority))
            .map(download_list_item_response)
            .map_err(|error| error.to_string());
    }

    start_download(app, state, id)
}

/// Ends a task and discards its partial transfer.
#[tauri::command]
async fn cancel_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadListItemResponse, String> {
    info!(download_id = %id, "cancelling download");

    state
        .downloads
        .cancel_task(&id)
        .await
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

/// Explicitly discards all transfer bytes and starts the task from zero. A
/// task that belongs to a queue is returned to that queue rather than
/// bypassing its runner.
#[tauri::command]
async fn restart_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadListItemResponse, String> {
    info!(download_id = %id, "restarting download from zero");

    let restarted = state
        .downloads
        .restart_task(&id)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(queue_id) = restarted.queue_id.clone() {
        return state
            .queues
            .enqueue_task(&id, &queue_id, Some(restarted.priority))
            .map(download_list_item_response)
            .map_err(|error| error.to_string());
    }

    Ok(download_list_item_response(restarted))
}

/// Replaces an expired or corrected source URL while keeping the task ID.
#[tauri::command]
fn refresh_download_source(
    state: State<'_, AppState>,
    id: String,
    source_url: String,
) -> Result<DownloadListItemResponse, String> {
    info!(download_id = %id, "refreshing download source");

    state
        .downloads
        .refresh_source_url(&id, &source_url)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

/// Restarts tasks whose retry backoff has elapsed.
///
/// Queued work is handed back to its queue so per-queue concurrency still
/// applies; everything else is started directly.
async fn run_retry_scheduler(
    app: AppHandle,
    downloads: DownloadService,
    queues: QueueService,
    publisher: EventPublisher,
    destination_directory: std::path::PathBuf,
) {
    loop {
        tokio::time::sleep(RETRY_POLL_INTERVAL).await;

        let due = match downloads.due_retries() {
            Ok(due) => due,
            Err(error) => {
                warn!(error = %error, "could not read pending retries");
                continue;
            }
        };

        for task in due {
            if let Some(queue_id) = task.queue_id.clone() {
                match queues.enqueue_task(&task.id, &queue_id, Some(task.priority)) {
                    Ok(record) => publisher.download_updated(record),
                    Err(error) => {
                        warn!(download_id = %task.id, error = %error, "could not requeue a retry")
                    }
                }

                continue;
            }

            if let Err(error) = downloads.claim_task(&task.id) {
                warn!(download_id = %task.id, error = %error, "could not claim a retry");
                continue;
            }

            info!(download_id = %task.id, attempt = task.attempts, "retrying download");

            spawn_transfer(
                downloads.clone(),
                publisher.clone(),
                task.id,
                destination_directory.clone(),
            );
        }

        // Nothing else keeps the handle alive; drop out if the app is gone.
        if app.webview_windows().is_empty() {
            return;
        }
    }
}

/// Runs one task's transfer in the background and publishes what happens.
fn spawn_transfer(
    downloads: DownloadService,
    publisher: EventPublisher,
    download_id: String,
    destination_directory: std::path::PathBuf,
) {
    let progress_publisher = publisher.clone();

    tauri::async_runtime::spawn(async move {
        let result = downloads
            .execute_claimed_task_with_progress(
                &download_id,
                &destination_directory,
                move |id, progress| {
                    progress_publisher.download_progress(id, progress);
                },
            )
            .await;

        match result {
            Ok(record) => {
                info!(download_id = %download_id, status = %record.status, "transfer finished");
                publisher.download_updated(record);
            }
            Err(error) => {
                warn!(download_id = %download_id, error = %error, "transfer failed");
                publisher.download_refreshed(&download_id);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    info!("starting Download Manager");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database_path = app_data_dir.join("downloads.db");

            let storage = Arc::new(Storage::open(&database_path)?);

            let downloads = DownloadService::new(Arc::clone(&storage))?;

            // Recover before anything is listed or resumed, so a transfer the
            // previous process was in the middle of never appears as active
            // work that nothing is driving.
            let recovered = downloads.recover_orphaned_tasks()?;

            if !recovered.is_empty() {
                info!(
                    count = recovered.len(),
                    "recovered tasks interrupted by a previous run"
                );
            }

            let queues = QueueService::new(Arc::clone(&storage), downloads.clone());
            let running_queue_ids = queues
                .list_queues()?
                .into_iter()
                .filter(|queue| queue.state == QueueState::Running)
                .map(|queue| queue.id)
                .collect::<Vec<_>>();
            let destination_directory = app.path().download_dir()?;

            info!(
                database = %storage.path().display(),
                schema_version = storage.schema_version()?,
                "storage initialized"
            );

            app.manage(AppState {
                core: CoreService::new(),
                storage,
                downloads: downloads.clone(),
                queues: queues.clone(),
            });

            let publisher = EventPublisher::new(app.handle().clone());

            for queue_id in running_queue_ids {
                info!(queue_id = %queue_id, "resuming persisted queue runner");

                spawn_queue_runner(
                    queues.clone(),
                    publisher.clone(),
                    queue_id,
                    destination_directory.clone(),
                );
            }

            tauri::async_runtime::spawn(run_retry_scheduler(
                app.handle().clone(),
                downloads.clone(),
                queues.clone(),
                publisher.clone(),
                destination_directory.clone(),
            ));

            info!("application state initialized");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            health_check,
            list_downloads,
            get_add_download_input_mode,
            set_add_download_input_mode,
            remove_download,
            create_download_task,
            start_download,
            pause_download,
            resume_download,
            cancel_download,
            restart_download,
            refresh_download_source,
            list_queues,
            create_queue,
            enqueue_download_task,
            move_queued_download,
            remove_download_from_queue,
            set_download_priority,
            reorder_queue_downloads,
            start_queue,
            stop_queue,
            set_queue_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running Download Manager");
}
