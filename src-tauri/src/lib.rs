use dm_common::{
    CategoryRecord, CompletionAction, DownloadPriority, DownloadRecord, DownloadRule, QueueRecord,
    QueueSchedule, QueueState, ScheduleKind,
};
use dm_core::{
    queue::{QueueRunnerEvent, QueueService},
    service::DownloadService,
    CoreService, TransferProgress,
};
mod automation;

use automation::Automation;
use dm_ipc::{
    AppInfoResponse, CategoryResponse, ComponentHealth, DownloadListItemResponse,
    DownloadRuleResponse, DownloadSettingsResponse, DownloadTaskEvent, HealthCheckResponse,
    LinkCandidateResponse, MediaClassificationResponse, MediaVariantResponse, QueueResponse,
    QueueRunnerEventResponse, QueueScheduleResponse, TransferProgressResponse,
};
use dm_storage::Storage;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(100);

/// How often pending retries are checked. Retry delays are measured in
/// seconds, so a coarse poll costs nothing and keeps the app idle.
const RETRY_POLL_INTERVAL: Duration = Duration::from_secs(5);
const SCHEDULE_POLL_INTERVAL: Duration = Duration::from_secs(15);

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
    /// Links a browser sent before the window could receive them.
    pending_link_intake: Mutex<Vec<String>>,
    automation: Automation,
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
        let app = publisher.app.clone();
        let events_queue_id = queue_id.clone();
        // Set once the runner hands a task over, so a queue that started and
        // found nothing to do never triggers its completion action.
        let processed_work = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let processed = Arc::clone(&processed_work);
        let result = queues
            .run_queue(&queue_id, destination_directory, move |event| {
                if matches!(event, QueueRunnerEvent::TaskUpdated(_)) {
                    processed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                publisher.queue_event(&events_queue_id, event);
            })
            .await;

        match result {
            Ok(_) => {
                info!(queue_id = %queue_id, "queue runner stopped");
                automation::queue_finished(
                    &app,
                    &queue_id,
                    processed_work.load(std::sync::atomic::Ordering::Relaxed),
                );
            }
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
fn list_queue_schedules(state: State<'_, AppState>) -> Result<Vec<QueueScheduleResponse>, String> {
    state
        .storage
        .list_queue_schedules()
        .map(|schedules| schedules.into_iter().map(queue_schedule_response).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn set_queue_schedule(
    state: State<'_, AppState>,
    queue_id: String,
    enabled: bool,
    kind: String,
    start_at: i64,
    stop_at: Option<i64>,
    weekdays_mask: u8,
    interval_seconds: Option<u64>,
    completion_action: String,
    prevent_sleep: bool,
    updated_at: i64,
    window_start_minute: Option<u16>,
    window_end_minute: Option<u16>,
) -> Result<QueueScheduleResponse, String> {
    let kind = kind
        .parse::<ScheduleKind>()
        .map_err(|_| "unknown schedule kind".to_owned())?;
    let completion_action = completion_action
        .parse::<CompletionAction>()
        .map_err(|_| "unknown completion action".to_owned())?;
    state
        .storage
        .upsert_queue_schedule(&QueueSchedule {
            queue_id,
            enabled,
            kind,
            start_at,
            stop_at,
            weekdays_mask,
            interval_seconds,
            completion_action,
            prevent_sleep,
            updated_at,
            window_start_minute,
            window_end_minute,
        })
        .map(queue_schedule_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_categories(state: State<'_, AppState>) -> Result<Vec<CategoryResponse>, String> {
    state
        .storage
        .list_categories()
        .map(|categories| categories.into_iter().map(category_response).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_download_rules(state: State<'_, AppState>) -> Result<Vec<DownloadRuleResponse>, String> {
    state
        .storage
        .list_rules()
        .map(|rules| rules.into_iter().map(rule_response).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_download_rule_explanation(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, String> {
    let record = state
        .storage
        .get_download(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    state
        .downloads
        .rule_decision_for_url(&record.source_url)
        .map(|decision| decision.map(|value| value.explanation))
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
fn handoff_browser_download(
    state: State<'_, AppState>,
    url: String,
    filename_hint: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
) -> Result<DownloadListItemResponse, String> {
    let handoff = dm_core::browser::BrowserHandoff {
        url,
        filename_hint,
        referrer,
        user_agent,
    }
    .validate()
    .map_err(|error| error.to_string())?;
    let context = handoff.request_context();
    let record = create_download_task(state.clone(), handoff.url)?;
    state
        .storage
        .set_request_context(&record.id, &context)
        .map_err(|error| error.to_string())?;
    Ok(record)
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

/// Starts queues whose persisted schedule is currently active. Queues started
/// manually are never stopped by this loop; only runners started here are
/// stopped when their window closes.
async fn run_queue_scheduler(
    app: AppHandle,
    storage: Arc<Storage>,
    queues: QueueService,
    publisher: EventPublisher,
    destination_directory: std::path::PathBuf,
) {
    let mut scheduler_owned = std::collections::HashSet::new();
    loop {
        tokio::time::sleep(SCHEDULE_POLL_INTERVAL).await;
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
            Err(_) => continue,
        };
        let schedules = match storage.list_queue_schedules() {
            Ok(schedules) => schedules,
            Err(error) => {
                warn!(error = %error, "could not read queue schedules");
                continue;
            }
        };
        for schedule in schedules {
            if schedule.is_active_at(now, automation::local_utc_offset_seconds()) {
                let already_running = queues.is_running(&schedule.queue_id).unwrap_or(false);
                if !already_running {
                    match queues.start_queue(&schedule.queue_id) {
                        Ok(_) => {
                            scheduler_owned.insert(schedule.queue_id.clone());
                            spawn_queue_runner(
                                queues.clone(),
                                publisher.clone(),
                                schedule.queue_id.clone(),
                                destination_directory.clone(),
                            );
                        }
                        Err(error) => {
                            warn!(queue_id = %schedule.queue_id, error = %error, "scheduled queue could not start")
                        }
                    }
                }
            } else if scheduler_owned.remove(&schedule.queue_id) {
                if let Err(error) = queues.stop_queue(&schedule.queue_id) {
                    warn!(queue_id = %schedule.queue_id, error = %error, "scheduled queue could not stop");
                }
            }
        }
        if app.webview_windows().is_empty() {
            return;
        }
    }
}

fn queue_schedule_response(schedule: QueueSchedule) -> QueueScheduleResponse {
    QueueScheduleResponse {
        queue_id: schedule.queue_id,
        enabled: schedule.enabled,
        kind: schedule.kind.as_str().to_owned(),
        start_at: schedule.start_at,
        stop_at: schedule.stop_at,
        weekdays_mask: schedule.weekdays_mask,
        interval_seconds: schedule.interval_seconds,
        completion_action: schedule.completion_action.as_str().to_owned(),
        prevent_sleep: schedule.prevent_sleep,
        updated_at: schedule.updated_at,
        window_start_minute: schedule.window_start_minute,
        window_end_minute: schedule.window_end_minute,
    }
}

fn category_response(category: CategoryRecord) -> CategoryResponse {
    CategoryResponse {
        id: category.id,
        name: category.name,
        extensions: category.extensions,
        mime_patterns: category.mime_patterns,
        default_directory: category.default_directory,
        host_patterns: category.host_patterns,
        priority: category.priority.to_string(),
        queue_id: category.queue_id,
    }
}

fn rule_response(rule: DownloadRule) -> DownloadRuleResponse {
    DownloadRuleResponse {
        id: rule.id,
        name: rule.name,
        enabled: rule.enabled,
        sort_order: rule.sort_order,
        domain: rule.domain,
        url_pattern: rule.url_pattern,
        extension: rule.extension,
        mime_pattern: rule.mime_pattern,
        min_size: rule.min_size,
        max_size: rule.max_size,
        category_id: rule.category_id,
        destination_directory: rule.destination_directory,
        queue_id: rule.queue_id,
        priority: rule.priority.map(|value| value.to_string()),
        max_connections: rule.max_connections,
        max_host_concurrency: rule.max_host_concurrency,
        speed_cap: rule.speed_cap,
        browser_takeover_allowed: rule.browser_takeover_allowed,
    }
}

fn download_settings_response(
    app: &AppHandle,
    state: &AppState,
) -> Result<DownloadSettingsResponse, String> {
    Ok(DownloadSettingsResponse {
        default_directory: state
            .downloads
            .default_directory()
            .map_err(|error| error.to_string())?
            .map(|path| path.to_string_lossy().into_owned()),
        system_directory: app
            .path()
            .download_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned()),
        global_speed_limit: state.downloads.global_speed_limit(),
        prevent_sleep: automation::prevent_sleep_enabled(state),
    })
}

#[tauri::command]
fn get_download_settings(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DownloadSettingsResponse, String> {
    download_settings_response(&app, &state)
}

/// `None` returns new downloads to the system Downloads folder.
#[tauri::command]
fn set_default_download_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    directory: Option<String>,
) -> Result<DownloadSettingsResponse, String> {
    let directory = directory
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    state
        .downloads
        .set_default_directory(directory.as_deref())
        .map_err(|error| error.to_string())?;
    download_settings_response(&app, &state)
}

/// Bytes per second for every download together; `None` or 0 is unlimited.
#[tauri::command]
fn set_global_speed_limit(
    app: AppHandle,
    state: State<'_, AppState>,
    bytes_per_second: Option<u64>,
) -> Result<DownloadSettingsResponse, String> {
    state
        .downloads
        .set_global_speed_limit(bytes_per_second)
        .map_err(|error| error.to_string())?;
    download_settings_response(&app, &state)
}

#[tauri::command]
fn set_prevent_sleep(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<DownloadSettingsResponse, String> {
    state
        .storage
        .set_setting(
            automation::SETTING_PREVENT_SLEEP,
            if enabled { "true" } else { "false" },
        )
        .map_err(|error| error.to_string())?;
    download_settings_response(&app, &state)
}

#[tauri::command]
fn set_category_directory(
    state: State<'_, AppState>,
    id: String,
    directory: Option<String>,
) -> Result<CategoryResponse, String> {
    if !is_blank_or_absolute(directory.as_deref()) {
        return Err("the folder must be an absolute path".to_owned());
    }
    state
        .storage
        .set_category_directory(&id, directory.as_deref())
        .map(category_response)
        .map_err(|error| error.to_string())
}

/// Folders are stored only as absolute paths; blank means "not set".
fn is_blank_or_absolute(directory: Option<&str>) -> bool {
    directory
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|value| std::path::Path::new(value).is_absolute())
}

/// Creates the rule when `rule.id` is empty, otherwise replaces it.
#[tauri::command]
fn save_download_rule(
    state: State<'_, AppState>,
    rule: DownloadRuleResponse,
) -> Result<DownloadRuleResponse, String> {
    let priority = rule
        .priority
        .as_deref()
        .map(DownloadPriority::from_str)
        .transpose()
        .map_err(|_| "unknown priority".to_owned())?;
    if !is_blank_or_absolute(rule.destination_directory.as_deref()) {
        return Err("the folder must be an absolute path".to_owned());
    }
    let record = DownloadRule {
        id: rule.id.trim().to_owned(),
        name: rule.name,
        enabled: rule.enabled,
        sort_order: rule.sort_order,
        domain: rule.domain,
        url_pattern: rule.url_pattern,
        extension: rule.extension,
        mime_pattern: rule.mime_pattern,
        min_size: rule.min_size,
        max_size: rule.max_size,
        category_id: rule.category_id,
        destination_directory: rule.destination_directory,
        queue_id: rule.queue_id,
        priority,
        max_connections: rule.max_connections,
        max_host_concurrency: rule.max_host_concurrency,
        speed_cap: rule.speed_cap.filter(|value| *value > 0),
        browser_takeover_allowed: rule.browser_takeover_allowed,
    };
    let saved = if record.id.is_empty() {
        state.storage.create_rule(&record)
    } else {
        state.storage.update_rule(&record)
    };
    saved.map(rule_response).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_download_rule(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .storage
        .delete_rule(&id)
        .map_err(|error| error.to_string())
}

/// Stops the power or exit action a finished queue scheduled.
#[tauri::command]
fn cancel_completion_action(app: AppHandle, state: State<'_, AppState>) -> bool {
    match state.automation.cancel_pending() {
        Some(id) => {
            automation::publish_cancelled(&app, id);
            true
        }
        None => false,
    }
}

/// The file of a completed download. Opening goes through this lookup so the
/// window can only ever open files the engine itself wrote.
fn completed_file(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    let record = state
        .storage
        .get_download(id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    if record.status != dm_common::DownloadStatus::Completed {
        return Err("the download has not finished yet".to_owned());
    }
    let path = record
        .destination_path
        .map(std::path::PathBuf::from)
        .ok_or_else(|| "the download has no file".to_owned())?;
    if !path.is_file() {
        return Err("the file was moved or deleted".to_owned());
    }
    Ok(path)
}

#[tauri::command]
fn open_download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let path = completed_file(&state, &id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reveal_download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let path = completed_file(&state, &id)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn inspect_links(input: String) -> Vec<LinkCandidateResponse> {
    dm_core::linkgrabber::extract_links(&input)
        .into_iter()
        .map(|candidate| LinkCandidateResponse {
            url: candidate.url,
            host: candidate.host,
            extension: candidate.extension,
        })
        .collect()
}

#[tauri::command]
fn classify_media_source(
    url: String,
    mime_type: Option<String>,
) -> Result<MediaClassificationResponse, String> {
    let kind = dm_core::media::classify_source(&url, mime_type.as_deref())
        .ok_or_else(|| "invalid media URL".to_owned())?;
    let kind = match kind {
        dm_core::media::MediaKind::Direct => "direct",
        dm_core::media::MediaKind::Hls => "hls",
        dm_core::media::MediaKind::Dash => "dash",
        dm_core::media::MediaKind::UnsupportedProtected => "unsupported_protected",
    };
    Ok(MediaClassificationResponse {
        kind: kind.to_owned(),
    })
}

#[tauri::command]
fn parse_hls_manifest(base_url: String, content: String) -> Vec<MediaVariantResponse> {
    dm_core::media::parse_hls_master_playlist(&base_url, &content)
        .into_iter()
        .map(|variant| MediaVariantResponse {
            uri: variant.uri,
            bandwidth: variant.bandwidth,
            width: variant.width,
            height: variant.height,
        })
        .collect()
}

/// What a launch of the executable asks the running application to do. The
/// browser native host starts the executable with one of these switches; the
/// single-instance plugin forwards a second launch's arguments here instead
/// of opening another window with a second engine on the same database.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchRequest {
    /// Tasks the native host already persisted; start or queue them.
    HandoffTasks(Vec<String>),
    /// A bare URL from an older native host; create a task, then as above.
    BrowserUrl(String),
    /// Links from a text selection, for review in LinkGrabber.
    GrabLinks(Vec<String>),
}

const ARG_HANDOFF_TASK: &str = "--handoff-task";
const ARG_BROWSER_HANDOFF: &str = "--browser-handoff";
const ARG_GRAB_LINKS: &str = "--grab-links";
const LINK_INTAKE_EVENT: &str = "link-intake";

fn parse_launch_args(args: &[String]) -> Vec<LaunchRequest> {
    let mut requests = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let values = args[index + 1..]
            .iter()
            .take_while(|value| !value.starts_with("--"))
            .cloned()
            .collect::<Vec<_>>();
        let consumed = values.len();

        match args[index].as_str() {
            ARG_HANDOFF_TASK if !values.is_empty() => {
                requests.push(LaunchRequest::HandoffTasks(values))
            }
            ARG_BROWSER_HANDOFF => {
                if let Some(url) = values.into_iter().next() {
                    requests.push(LaunchRequest::BrowserUrl(url));
                }
            }
            ARG_GRAB_LINKS if !values.is_empty() => requests.push(LaunchRequest::GrabLinks(values)),
            _ => {}
        }

        index += 1 + consumed;
    }

    requests
}

/// Brings the main window forward so the user sees what the browser sent.
fn reveal_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn handle_launch_requests(app: &AppHandle, requests: Vec<LaunchRequest>) {
    if requests.is_empty() {
        return;
    }

    let Some(state) = app.try_state::<AppState>() else {
        warn!("launch request arrived before the application state was ready");
        return;
    };

    for request in requests {
        match request {
            LaunchRequest::HandoffTasks(ids) => {
                for id in ids {
                    if let Err(error) = start_handoff_task(app, &state, &id) {
                        warn!(download_id = %id, error = %error, "browser handoff could not start");
                    }
                }
            }
            LaunchRequest::BrowserUrl(url) => {
                let started = dm_core::browser::BrowserHandoff {
                    url,
                    filename_hint: None,
                    referrer: None,
                    user_agent: None,
                }
                .validate()
                .map_err(|error| error.to_string())
                .and_then(|handoff| {
                    state
                        .downloads
                        .create_task(&handoff.url)
                        .map_err(|error| error.to_string())
                })
                .and_then(|task| start_handoff_task(app, &state, &task.id));
                if let Err(error) = started {
                    warn!(error = %error, "browser handoff could not start");
                }
            }
            LaunchRequest::GrabLinks(urls) => {
                if let Ok(mut pending) = state.pending_link_intake.lock() {
                    pending.extend(urls.iter().cloned());
                }
                // A window that is already listening takes the links now; one
                // still loading collects them with `take_pending_link_intake`.
                let _ = app.emit(LINK_INTAKE_EVENT, ());
            }
        }
    }

    reveal_main_window(app);
}

/// Starts or queues a task that arrived from the browser, applying the same
/// intake rules as Add Download, and shows the row immediately.
fn start_handoff_task(app: &AppHandle, state: &AppState, task_id: &str) -> Result<(), String> {
    let task = state
        .storage
        .get_download(task_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {task_id}"))?;
    let publisher = EventPublisher::new(app.clone());

    let decision = state
        .downloads
        .rule_decision_for_url(&task.source_url)
        .map_err(|error| error.to_string())?;
    if let Some(decision) = decision {
        if let Some(priority) = decision.priority {
            state
                .queues
                .set_task_priority(&task.id, priority)
                .map_err(|error| error.to_string())?;
        }
        if let Some(queue_id) = decision.queue_id {
            let queued = state
                .queues
                .enqueue_task(&task.id, &queue_id, decision.priority)
                .map_err(|error| error.to_string())?;
            publisher.download_updated(queued);
            return Ok(());
        }
    }

    let claimed = state
        .downloads
        .claim_task(&task.id)
        .map_err(|error| error.to_string())?;
    publisher.download_updated(claimed);

    let destination_directory = app
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;
    spawn_transfer(
        state.downloads.clone(),
        publisher,
        task.id,
        destination_directory,
    );
    Ok(())
}

/// Hands LinkGrabber the links a browser sent while the window was not yet
/// listening. Each link is returned once.
#[tauri::command]
fn take_pending_link_intake(state: State<'_, AppState>) -> Vec<String> {
    state
        .pending_link_intake
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default()
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

    let launch_requests = parse_launch_args(&std::env::args().skip(1).collect::<Vec<_>>());

    info!("starting Download Manager");

    tauri::Builder::default()
        // Registered first so a second launch exits before it opens a window
        // or the database; its arguments are handled by this process instead.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let requests = parse_launch_args(args.get(1..).unwrap_or_default());
            if requests.is_empty() {
                reveal_main_window(app);
            } else {
                handle_launch_requests(app, requests);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
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
                storage: Arc::clone(&storage),
                downloads: downloads.clone(),
                queues: queues.clone(),
                pending_link_intake: Mutex::new(Vec::new()),
                automation: Automation::new(),
            });

            tauri::async_runtime::spawn(automation::run_keep_awake(app.handle().clone()));

            handle_launch_requests(app.handle(), launch_requests.clone());

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

            tauri::async_runtime::spawn(run_queue_scheduler(
                app.handle().clone(),
                Arc::clone(&storage),
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
            handoff_browser_download,
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
            set_queue_enabled,
            list_queue_schedules,
            set_queue_schedule,
            list_categories,
            list_download_rules,
            get_download_rule_explanation,
            inspect_links,
            take_pending_link_intake,
            get_download_settings,
            set_default_download_directory,
            set_global_speed_limit,
            set_prevent_sleep,
            set_category_directory,
            save_download_rule,
            delete_download_rule,
            cancel_completion_action,
            open_download_file,
            reveal_download_file,
            classify_media_source,
            parse_hls_manifest
        ])
        .run(tauri::generate_context!())
        .expect("error while running Download Manager");
}

#[cfg(test)]
mod tests {
    use super::{parse_launch_args, LaunchRequest};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_every_launch_switch_the_native_host_sends() {
        assert_eq!(
            parse_launch_args(&args(&["--handoff-task", "a", "b"])),
            vec![LaunchRequest::HandoffTasks(args(&["a", "b"]))]
        );
        assert_eq!(
            parse_launch_args(&args(&["--browser-handoff", "https://x.test/f"])),
            vec![LaunchRequest::BrowserUrl("https://x.test/f".to_owned())]
        );
        assert_eq!(
            parse_launch_args(&args(&[
                "--grab-links",
                "https://x.test/1",
                "https://x.test/2",
                "--handoff-task",
                "t1",
            ])),
            vec![
                LaunchRequest::GrabLinks(args(&["https://x.test/1", "https://x.test/2"])),
                LaunchRequest::HandoffTasks(args(&["t1"])),
            ]
        );
    }

    #[test]
    fn ignores_unknown_and_empty_switches() {
        assert!(parse_launch_args(&args(&[])).is_empty());
        assert!(parse_launch_args(&args(&["--handoff-task"])).is_empty());
        assert!(parse_launch_args(&args(&["--unknown", "x", "stray"])).is_empty());
    }
}
