use dm_common::{
    CategoryRecord, CompletionAction, DownloadPriority, DownloadRecord, DownloadRule,
    DownloadStatus, QueueRecord, QueueSchedule, QueueState, ScheduleKind,
};
use dm_core::{
    network::{NetworkSettings, ProxyMode},
    queue::{QueueRunnerEvent, QueueService},
    service::DownloadService,
    traffic::parse_host_list,
    CoreService, TransferProgress,
};
mod automation;
mod browser_setup;
mod clipboard_watch;
mod mini;
mod portable;
mod tools;
mod tray;
mod updates;

use automation::Automation;
use dm_ipc::UiPreferencesResponse;
use dm_ipc::{
    ActivityDayResponse, BackupInfoResponse, ConnectionCheckResponse, DownloadStatsResponse,
    NamedTotalResponse, RestoreOutcomeResponse,
};
use dm_ipc::{
    AppInfoResponse, CategoryResponse, ComponentHealth, DownloadListItemResponse,
    DownloadRuleResponse, DownloadSettingsResponse, DownloadTaskEvent, HealthCheckResponse,
    LinkCandidateResponse, MediaClassificationResponse, MediaVariantResponse,
    NetworkSettingsResponse, QueueResponse, QueueRunnerEventResponse, QueueScheduleResponse,
    RuleExplanationResponse, TrafficSummaryResponse, TransferProgressResponse,
};
use dm_ipc::{DownloadChecksResponse, PostProcessSettingsResponse};
use dm_ipc::{EngineSettingsResponse, FfmpegStatusResponse, StreamVariantResponse};
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
    /// Links waiting for the small add window to collect them.
    pending_mini_links: Mutex<Vec<String>>,
    automation: Automation,
    tray_menu: Mutex<Option<tray::TrayMenu>>,
    /// What happened to a restore that waited for this start; shown once.
    restore_outcome: Mutex<Option<RestoreOutcomeResponse>>,
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

/// Setting key: a sound when a download finishes or fails (on unless turned
/// off).
const SETTING_FINISH_SOUND: &str = "finish_sound";
/// When the last sound played; downloads ending together sound once.
static LAST_SOUND: Mutex<Option<Instant>> = Mutex::new(None);

fn finish_sound_enabled(app: &AppHandle) -> bool {
    app.try_state::<AppState>().is_none_or(|state| {
        state
            .storage
            .get_setting(SETTING_FINISH_SOUND)
            .ok()
            .flatten()
            .as_deref()
            != Some("false")
    })
}

fn play_finish_sound(app: &AppHandle, failed: bool) {
    if !finish_sound_enabled(app) {
        return;
    }
    if let Ok(mut last) = LAST_SOUND.lock() {
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(3)) {
            return;
        }
        *last = Some(Instant::now());
    }
    dm_system::sound::play(if failed {
        dm_system::sound::Chime::Failed
    } else {
        dm_system::sound::Chime::Finished
    });
}

#[tauri::command]
fn get_keep_server_time(state: State<'_, AppState>) -> bool {
    state.downloads.keep_server_time()
}

#[tauri::command]
fn set_keep_server_time(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state
        .downloads
        .set_keep_server_time(enabled)
        .map_err(|error| error.to_string())?;
    Ok(state.downloads.keep_server_time())
}

#[tauri::command]
fn get_finish_sound(app: AppHandle) -> bool {
    finish_sound_enabled(&app)
}

#[tauri::command]
fn set_finish_sound(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state
        .storage
        .set_setting(SETTING_FINISH_SOUND, if enabled { "true" } else { "false" })
        .map_err(|error| error.to_string())?;
    Ok(enabled)
}

/// Endings already announced by a system notification, as `id:status`, so a
/// row published again (a refresh) is not announced twice.
static ANNOUNCED: std::sync::LazyLock<Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

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
        self.announce(&record);
        self.emit(
            DOWNLOAD_TASK_EVENT,
            DownloadTaskEvent::updated(download_list_item_response(record)),
        );
    }

    /// A system notification when a download finishes or fails while the
    /// window is hidden or in the background, where its own message would
    /// go unseen.
    fn announce(&self, record: &DownloadRecord) {
        let ended = matches!(
            record.status,
            dm_common::DownloadStatus::Completed | dm_common::DownloadStatus::Failed
        );
        if !ended {
            // Running again (restarted, retried): its next ending is news.
            if let Ok(mut announced) = ANNOUNCED.lock() {
                let prefix = format!("{}:", record.id);
                announced.retain(|key| !key.starts_with(&prefix));
            }
            return;
        }
        automation::download_finished(&self.app, record);
        let key = format!("{}:{}", record.id, record.status);
        let first = ANNOUNCED
            .lock()
            .map(|mut announced| announced.insert(key))
            .unwrap_or(false);
        if !first {
            return;
        }
        play_finish_sound(&self.app, record.status == DownloadStatus::Failed);
        let in_view = self.app.get_webview_window("main").is_some_and(|window| {
            window.is_visible().unwrap_or(false)
                && window.is_focused().unwrap_or(false)
                && !window.is_minimized().unwrap_or(false)
        });
        if !in_view {
            automation::download_ended(&self.app, record);
        }
    }

    fn download_removed(&self, download_id: &str) {
        self.forget(download_id);
        self.emit(DOWNLOAD_TASK_EVENT, DownloadTaskEvent::removed(download_id));
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
                self.announce(&download);
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
        name: "Ratatoskr".to_owned(),
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
    start_queue_now(&app, &state, &queue_id).map(queue_response)
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
) -> Result<Option<RuleExplanationResponse>, String> {
    let record = state
        .storage
        .get_download(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    state
        .downloads
        .rule_decision_for_url(&record.source_url)
        .map(|decision| {
            decision.map(|value| RuleExplanationResponse {
                rule_name: value.rule_name,
                category_id: value.category_id,
                category_name: value.category_name,
            })
        })
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
        dm_core::service::discard_partial(temp_path);
    }

    state
        .storage
        .remove_download_record(&id)
        .map_err(|error| error.to_string())?;

    state.downloads.forget_browser_session(&id);
    Ok(())
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

/// Looks for a newer version now. `not_configured` when no release address
/// is set up.
#[tauri::command]
async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<updates::UpdateInfo>, String> {
    updates::check(&app, &state.storage).await
}

/// Installs the newer version and restarts into it.
#[tauri::command]
async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    updates::install(&app, &state.storage).await
}

#[tauri::command]
fn get_auto_update_check(state: State<'_, AppState>) -> bool {
    updates::auto_check_enabled(&state.storage)
}

#[tauri::command]
fn set_auto_update_check(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state
        .storage
        .set_setting(
            updates::SETTING_AUTO_UPDATE_CHECK,
            if enabled { "true" } else { "false" },
        )
        .map_err(|error| error.to_string())?;
    Ok(updates::auto_check_enabled(&state.storage))
}

/// What the browser extension needs and what is already done.
#[tauri::command]
fn get_browser_connection(app: AppHandle) -> browser_setup::BrowserConnection {
    browser_setup::connection(&app)
}

/// Registers the connector again, for when a browser was installed after
/// the app started.
#[tauri::command]
fn connect_browsers(app: AppHandle) -> Result<browser_setup::BrowserConnection, String> {
    browser_setup::register(&app)?;
    Ok(browser_setup::connection(&app))
}

/// Shows the extension folder, to load it in the browser.
#[tauri::command]
fn reveal_extension_folder(app: AppHandle) -> Result<(), String> {
    let folder = browser_setup::extension_folder(&app)
        .ok_or_else(|| "the extension folder is missing".to_owned())?;
    app.opener()
        .reveal_item_in_dir(folder.join("manifest.json"))
        .map_err(|error| error.to_string())
}

/// Asks Firefox to add the signed extension package that ships with the app.
#[tauri::command]
fn install_firefox_extension(app: AppHandle) -> Result<(), String> {
    browser_setup::install_in_firefox(&app)
}

#[tauri::command]
fn open_browser_extensions_page(browser: String) -> Result<(), String> {
    browser_setup::open_extensions_page(&browser)
}

/// Whether copied download links bring up the Add download dialog.
#[tauri::command]
fn get_clipboard_watch(state: State<'_, AppState>) -> bool {
    clipboard_watch::enabled(&state.storage)
}

/// Whether the app starts (in the tray) when the user signs in to Windows.
/// `None` where the app cannot start itself (outside Windows), so the
/// setting is not offered.
#[tauri::command]
fn get_start_with_windows() -> Option<bool> {
    if !cfg!(windows) {
        return None;
    }
    std::env::current_exe()
        .ok()
        .map(|application| dm_system::autostart::enabled(&application))
}

#[tauri::command]
fn set_start_with_windows(enabled: bool) -> Result<bool, String> {
    let application = std::env::current_exe().map_err(|error| error.to_string())?;
    dm_system::autostart::set(&application, enabled).map_err(|error| error.to_string())?;
    Ok(dm_system::autostart::enabled(&application))
}

#[tauri::command]
fn set_clipboard_watch(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state
        .storage
        .set_setting(
            clipboard_watch::SETTING_CLIPBOARD_WATCH,
            if enabled { "true" } else { "false" },
        )
        .map_err(|error| error.to_string())?;
    Ok(clipboard_watch::enabled(&state.storage))
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
/// Creates a download for a link typed or pasted in the window. A fresh
/// link for a download that stopped part-way continues that download
/// instead of starting a second copy.
#[tauri::command]
async fn create_download_task(
    state: State<'_, AppState>,
    url: String,
    directory: Option<String>,
    filename: Option<String>,
) -> Result<DownloadListItemResponse, String> {
    let created = create_download_record(&state, url, directory.as_deref())?;
    if let Some(filename) = filename.as_deref().filter(|name| !name.trim().is_empty()) {
        if let Err(error) = state
            .downloads
            .set_download_name(&created.id, Some(filename))
        {
            let _ = state.storage.remove_download_record(&created.id);
            return Err(error.to_string());
        }
    }
    let created = match state.storage.get_download(&created.id) {
        Ok(Some(record)) => download_list_item_response(record),
        _ => created,
    };
    // A folder or name chosen for this download belongs to a new one, not
    // to a stopped download the link would otherwise continue.
    if directory.is_none() && filename.is_none() && state.downloads.may_adopt(&created.id) {
        if let Ok(Some(adopted)) = state.downloads.adopt_fresh_link(&created.id).await {
            info!(download_id = %adopted.id, "fresh link continues a stopped download");
            return Ok(download_list_item_response(adopted));
        }
    }
    Ok(created)
}

fn create_download_record(
    state: &AppState,
    url: String,
    directory: Option<&str>,
) -> Result<DownloadListItemResponse, String> {
    let task = state
        .downloads
        .create_task(&url)
        .map_err(|error| error.to_string())?;

    // Saved before a rule can hand the download to a running queue.
    if let Some(directory) = directory.map(str::trim).filter(|value| !value.is_empty()) {
        if let Err(error) = state
            .downloads
            .set_download_folder(&task.id, Some(std::path::Path::new(directory)))
        {
            let _ = state.storage.remove_download_record(&task.id);
            return Err(error.to_string());
        }
    }

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
    let record = create_download_record(&state, handoff.url, None)?;
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
    info!(download_id = %id, "resuming download");
    resume_now(&app, &state, &id).map(download_list_item_response)
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

            match downloads.claim_task(&task.id) {
                // The row no longer says "retrying" or carries the old
                // error; show that now rather than when the transfer ends.
                Ok(record) => publisher.download_updated(record),
                Err(error) => {
                    warn!(download_id = %task.id, error = %error, "could not claim a retry");
                    continue;
                }
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
        max_connections: u32::try_from(state.downloads.max_connections()).unwrap_or(u32::MAX),
    })
}

#[tauri::command]
fn list_download_mirrors(state: State<'_, AppState>, id: String) -> Result<Vec<String>, String> {
    state
        .downloads
        .mirrors(&id)
        .map_err(|error| error.to_string())
}

/// Replaces a download's other addresses. They are checked against the file
/// when the download next runs; a mirror of a different file is never used.
#[tauri::command]
fn set_download_mirrors(
    state: State<'_, AppState>,
    id: String,
    urls: Vec<String>,
) -> Result<Vec<String>, String> {
    state
        .downloads
        .set_mirrors(&id, &urls)
        .map_err(|error| error.to_string())
}

/// Saves a download in `directory` instead of the usual folder; `None` goes
/// back to the usual one. Only before the download has reserved its file.
#[tauri::command]
fn set_download_folder(
    state: State<'_, AppState>,
    id: String,
    directory: Option<String>,
) -> Result<(), String> {
    state
        .downloads
        .set_download_folder(&id, directory.as_deref().map(std::path::Path::new))
        .map_err(|error| error.to_string())
}

/// Saves a download under `filename` instead of the name the server gives;
/// `None` goes back to that name. Only before the download reserved its file.
#[tauri::command]
fn set_download_name(
    state: State<'_, AppState>,
    id: String,
    filename: Option<String>,
) -> Result<(), String> {
    state
        .downloads
        .set_download_name(&id, filename.as_deref())
        .map_err(|error| error.to_string())
}

/// What a video page holds (its qualities and their sizes, or a playlist's
/// videos), for choosing before downloading. Fails with `needs_ytdlp` when
/// yt-dlp is not available.
#[tauri::command]
async fn probe_video(
    state: State<'_, AppState>,
    url: String,
) -> Result<dm_core::ytdlp::VideoProbe, String> {
    if !dm_core::ytdlp::handles(dm_core::ytdlp::without_fragment(&url)) {
        return Err("not_a_video_page".to_owned());
    }
    state
        .downloads
        .probe_video(&url)
        .await
        .map_err(|error| match error {
            dm_core::service::DownloadServiceError::Download(
                dm_core::DownloadError::NeedsYtDlp,
            ) => "needs_ytdlp".to_owned(),
            dm_core::service::DownloadServiceError::Download(error) => error.redacted_message(),
            other => other.to_string(),
        })
}

/// The qualities an HLS link offers, for choosing before downloading.
#[tauri::command]
async fn list_stream_variants(
    state: State<'_, AppState>,
    url: String,
) -> Result<Vec<StreamVariantResponse>, String> {
    let variants = state
        .downloads
        .stream_variants(&url)
        .await
        .map_err(|error| error.to_string())?;
    Ok(variants
        .into_iter()
        .map(|variant| StreamVariantResponse {
            needs_muxing: variant.audio_group.is_some(),
            uri: variant.uri,
            bandwidth: variant.bandwidth,
            width: variant.width,
            height: variant.height,
        })
        .collect())
}

fn engine_settings_response(state: &AppState) -> EngineSettingsResponse {
    EngineSettingsResponse {
        auto_adopt_links: state.downloads.auto_adopt_links(),
        polite_hosts: state.downloads.polite_hosts().join("\n"),
        stream_max_height: state.downloads.stream_max_height(),
        stream_prefer_mp4: state.downloads.stream_prefer_mp4(),
    }
}

async fn ffmpeg_status(state: &AppState) -> FfmpegStatusResponse {
    let found = state.downloads.ffmpeg();
    let version = match &found {
        Some(ffmpeg) => ffmpeg.version().await,
        None => None,
    };
    FfmpegStatusResponse {
        configured_path: state.downloads.ffmpeg_path_setting(),
        found_path: found.map(|ffmpeg| ffmpeg.path().to_string_lossy().into_owned()),
        version,
    }
}

#[tauri::command]
async fn get_ffmpeg_status(state: State<'_, AppState>) -> Result<FfmpegStatusResponse, String> {
    Ok(ffmpeg_status(&state).await)
}

/// `None` looks for FFmpeg next to the application and on `PATH`.
#[tauri::command]
async fn set_ffmpeg_path(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<FfmpegStatusResponse, String> {
    let path = path
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    state
        .downloads
        .set_ffmpeg_path(path.as_deref())
        .map_err(|_| "choose the FFmpeg program itself (ffmpeg.exe)".to_owned())?;
    Ok(ffmpeg_status(&state).await)
}

async fn ytdlp_status(state: &AppState) -> FfmpegStatusResponse {
    let found = state.downloads.ytdlp();
    let version = match &found {
        Some(ytdlp) => ytdlp.version().await,
        None => None,
    };
    FfmpegStatusResponse {
        configured_path: state.downloads.ytdlp_path_setting(),
        found_path: found.map(|ytdlp| ytdlp.path().to_string_lossy().into_owned()),
        version,
    }
}

/// Where yt-dlp is and which version, for Settings.
#[tauri::command]
async fn get_ytdlp_status(state: State<'_, AppState>) -> Result<FfmpegStatusResponse, String> {
    Ok(ytdlp_status(&state).await)
}

/// Downloads FFmpeg (checksum verified) into the app's own tools folder.
#[tauri::command]
async fn install_ffmpeg(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FfmpegStatusResponse, String> {
    let folder = tools::managed_dir(&app).ok_or_else(|| "no application data folder".to_owned())?;
    state.downloads.install_ffmpeg(&folder).await?;
    Ok(ffmpeg_status(&state).await)
}

/// Installs yt-dlp if it is missing, or updates the app's own copy.
#[tauri::command]
async fn update_ytdlp(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FfmpegStatusResponse, String> {
    tools::ensure_ytdlp(&app, &state.downloads, true).await?;
    Ok(ytdlp_status(&state).await)
}

/// `None` looks for yt-dlp next to the application and on `PATH`.
#[tauri::command]
async fn set_ytdlp_path(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<FfmpegStatusResponse, String> {
    let path = path
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    state
        .downloads
        .set_ytdlp_path(path.as_deref())
        .map_err(|_| "choose the yt-dlp program itself (yt-dlp.exe)".to_owned())?;
    Ok(ytdlp_status(&state).await)
}

// -- statistics, backup, diagnostics ------------------------------------------

fn named_total(total: dm_core::stats::NamedTotal) -> NamedTotalResponse {
    NamedTotalResponse {
        name: total.name,
        count: total.count,
        bytes: total.bytes,
    }
}

#[tauri::command]
fn get_download_stats(
    state: State<'_, AppState>,
    days: u32,
) -> Result<DownloadStatsResponse, String> {
    let stats = state
        .downloads
        .download_stats(days)
        .map_err(|error| error.to_string())?;
    Ok(DownloadStatsResponse {
        days: stats
            .days
            .into_iter()
            .map(|day| ActivityDayResponse {
                day: day.day,
                domestic_bytes: day.domestic_bytes,
                international_bytes: day.international_bytes,
                completed: day.completed,
            })
            .collect(),
        period_completed: stats.period_completed,
        period_domestic_bytes: stats.period_domestic_bytes,
        period_international_bytes: stats.period_international_bytes,
        all_completed: stats.all_completed,
        all_completed_bytes: stats.all_completed_bytes,
        failed: stats.failed,
        active: stats.active,
        top_hosts: stats.top_hosts.into_iter().map(named_total).collect(),
        extensions: stats.extensions.into_iter().map(named_total).collect(),
        largest: stats.largest.map(named_total),
    })
}

/// A path the user chose in a save or open dialog.
fn chosen_path(path: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(path.trim());
    if !path.is_absolute() || path.file_name().is_none() || path.is_dir() {
        return Err("choose a file".to_owned());
    }
    Ok(path)
}

fn backup_info(info: dm_storage::BackupInfo) -> BackupInfoResponse {
    BackupInfoResponse {
        schema_version: info.schema_version,
        downloads: info.downloads,
        queues: info.queues,
        bytes: info.bytes,
    }
}

#[tauri::command]
async fn backup_database(
    state: State<'_, AppState>,
    path: String,
) -> Result<BackupInfoResponse, String> {
    let target = chosen_path(&path)?;
    let storage = Arc::clone(&state.storage);
    tauri::async_runtime::spawn_blocking(move || storage.backup_to(&target))
        .await
        .map_err(|error| error.to_string())?
        .map(backup_info)
        .map_err(|error| error.to_string())
}

/// Checks a backup and sets it to replace the database on the next start.
#[tauri::command]
fn stage_restore(state: State<'_, AppState>, path: String) -> Result<BackupInfoResponse, String> {
    let backup = chosen_path(&path)?;
    dm_storage::stage_restore(state.storage.path(), &backup)
        .map(backup_info)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_pending_restore(state: State<'_, AppState>) -> Option<BackupInfoResponse> {
    dm_storage::pending_restore(state.storage.path()).map(backup_info)
}

#[tauri::command]
fn cancel_restore(state: State<'_, AppState>) -> Result<(), String> {
    dm_storage::cancel_pending_restore(state.storage.path()).map_err(|error| error.to_string())
}

#[tauri::command]
fn take_restore_outcome(state: State<'_, AppState>) -> Option<RestoreOutcomeResponse> {
    state
        .restore_outcome
        .lock()
        .ok()
        .and_then(|mut outcome| outcome.take())
}

/// Restarts the application so a waiting restore takes effect. Running
/// transfers are paused first, so their partial files stay usable.
#[tauri::command]
fn restart_app(app: AppHandle, state: State<'_, AppState>) {
    state.downloads.pause_all();
    app.restart();
}

#[tauri::command]
fn export_downloads(
    state: State<'_, AppState>,
    path: String,
    format: String,
    ids: Option<Vec<String>>,
) -> Result<(), String> {
    let target = chosen_path(&path)?;
    let format = match format.as_str() {
        "csv" => dm_core::service::ExportFormat::Csv,
        "links" => dm_core::service::ExportFormat::Links,
        _ => return Err("unknown export format".to_owned()),
    };
    let text = state
        .downloads
        .export_downloads(format, ids.as_deref())
        .map_err(|error| error.to_string())?;
    std::fs::write(target, text).map_err(|error| error.to_string())
}

async fn diagnostics_text(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let folder = state
        .downloads
        .default_directory()
        .ok()
        .flatten()
        .or_else(|| app.path().download_dir().ok())
        .unwrap_or_default();
    let home = app
        .path()
        .home_dir()
        .ok()
        .map(|home| home.to_string_lossy().into_owned());
    state
        .downloads
        .diagnostics_report(env!("CARGO_PKG_VERSION"), &folder, home.as_deref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_diagnostics_report(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    diagnostics_text(&app, &state).await
}

#[tauri::command]
async fn save_diagnostics_report(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let target = chosen_path(&path)?;
    let text = diagnostics_text(&app, &state).await?;
    std::fs::write(target, text).map_err(|error| error.to_string())
}

#[tauri::command]
fn check_database(state: State<'_, AppState>) -> Result<String, String> {
    state
        .storage
        .integrity_check()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn check_connection(
    state: State<'_, AppState>,
    url: String,
) -> Result<ConnectionCheckResponse, String> {
    let check = state.downloads.connection_check(&url).await;
    Ok(ConnectionCheckResponse {
        host: check.host,
        route: check.route,
        reachable: check.reachable,
        elapsed_ms: check.elapsed_ms,
        final_host: check.final_host,
        filename: check.filename,
        total_bytes: check.total_bytes,
        range_supported: check.range_supported,
        error: check.error,
    })
}

/// Tells the window that a download's after-download results changed.
const DOWNLOAD_CHECKS_EVENT: &str = "download-checks-changed";

fn download_checks_response(checks: Option<dm_storage::DownloadChecks>) -> DownloadChecksResponse {
    let Some(checks) = checks else {
        return DownloadChecksResponse {
            state: "idle".to_owned(),
            ..DownloadChecksResponse::default()
        };
    };
    DownloadChecksResponse {
        state: checks.state,
        expected_checksum: checks.expected_checksum,
        algorithm: checks.algorithm,
        actual_checksum: checks.actual_checksum,
        integrity: checks.integrity,
        scan: checks.scan,
        scan_detail: checks.scan_detail,
        extracted_to: checks.extracted_to,
        extract_error: checks.extract_error,
        command_error: checks.command_error,
    }
}

#[tauri::command]
fn get_download_checks(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<DownloadChecksResponse, String> {
    state
        .downloads
        .download_checks(&download_id)
        .map(download_checks_response)
        .map_err(|error| error.to_string())
}

/// Stores the checksum the download should have; an empty value forgets
/// it. A finished download is checked at once.
#[tauri::command]
async fn set_expected_checksum(
    state: State<'_, AppState>,
    download_id: String,
    checksum: Option<String>,
) -> Result<DownloadChecksResponse, String> {
    state
        .downloads
        .set_expected_checksum(&download_id, checksum.as_deref())
        .await
        .map(|checks| download_checks_response(Some(checks)))
        .map_err(|error| error.to_string())
}

/// Runs the after-download steps again for a finished download.
#[tauri::command]
async fn run_post_process(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<DownloadChecksResponse, String> {
    state
        .downloads
        .post_process(&download_id)
        .await
        .map(|checks| download_checks_response(Some(checks)))
        .map_err(|error| error.to_string())
}

fn post_process_settings_response(state: &AppState) -> PostProcessSettingsResponse {
    let settings = state.downloads.post_process_settings();
    PostProcessSettingsResponse {
        hash_always: settings.hash_always,
        extract_zip: settings.extract_zip,
        scan: settings.scan,
        command: settings.command.unwrap_or_default(),
        scan_available: dm_core::postprocess::defender_path().is_some(),
    }
}

#[tauri::command]
fn get_post_process_settings(state: State<'_, AppState>) -> PostProcessSettingsResponse {
    post_process_settings_response(&state)
}

#[tauri::command]
fn set_post_process_settings(
    state: State<'_, AppState>,
    settings: PostProcessSettingsResponse,
) -> Result<PostProcessSettingsResponse, String> {
    let command = settings.command.trim();
    state
        .downloads
        .set_post_process_settings(&dm_core::service::PostProcessSettings {
            hash_always: settings.hash_always,
            extract_zip: settings.extract_zip,
            scan: settings.scan,
            command: (!command.is_empty()).then(|| command.to_owned()),
        })
        .map_err(|error| error.to_string())?;
    Ok(post_process_settings_response(&state))
}

#[tauri::command]
fn get_engine_settings(state: State<'_, AppState>) -> EngineSettingsResponse {
    engine_settings_response(&state)
}

#[tauri::command]
fn set_engine_settings(
    state: State<'_, AppState>,
    settings: EngineSettingsResponse,
) -> Result<EngineSettingsResponse, String> {
    state
        .downloads
        .set_auto_adopt_links(settings.auto_adopt_links)
        .and_then(|()| {
            state
                .downloads
                .set_polite_hosts(&parse_host_list(&settings.polite_hosts))
        })
        .and_then(|()| {
            state
                .downloads
                .set_stream_max_height(settings.stream_max_height)
        })
        .and_then(|()| {
            state
                .downloads
                .set_stream_prefer_mp4(settings.stream_prefer_mp4)
        })
        .map_err(|error| error.to_string())?;
    Ok(engine_settings_response(&state))
}

/// Most connections one download may open (1 to 64).
#[tauri::command]
fn set_max_connections(
    app: AppHandle,
    state: State<'_, AppState>,
    connections: u32,
) -> Result<DownloadSettingsResponse, String> {
    state
        .downloads
        .set_max_connections(connections as usize)
        .map_err(|error| error.to_string())?;
    download_settings_response(&app, &state)
}

fn network_settings_response(settings: &NetworkSettings) -> NetworkSettingsResponse {
    NetworkSettingsResponse {
        mode: settings.mode.as_str().to_owned(),
        proxy_url: settings.proxy_url.clone(),
        pac_url: settings.pac_url.clone(),
        system_pac_url: dm_core::pac::system_script_url(),
        direct_hosts: settings.direct_hosts.join("\n"),
        domestic_direct: settings.domestic_direct,
        domestic_hosts: settings.domestic_hosts.join("\n"),
    }
}

#[tauri::command]
fn get_network_settings(state: State<'_, AppState>) -> NetworkSettingsResponse {
    network_settings_response(&state.downloads.network_settings())
}

/// Stores and applies the proxy route. A proxy address with a user name or
/// password is refused, so no credential is ever written.
#[tauri::command]
fn set_network_settings(
    state: State<'_, AppState>,
    settings: NetworkSettingsResponse,
) -> Result<NetworkSettingsResponse, String> {
    let mode = ProxyMode::parse(&settings.mode)
        .ok_or_else(|| format!("unknown proxy mode {:?}", settings.mode))?;
    let next = NetworkSettings {
        mode,
        proxy_url: settings
            .proxy_url
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        pac_url: settings
            .pac_url
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        direct_hosts: parse_host_list(&settings.direct_hosts),
        domestic_direct: settings.domestic_direct,
        domestic_hosts: parse_host_list(&settings.domestic_hosts),
    };
    state
        .downloads
        .set_network_settings(&next)
        .map_err(|error| error.to_string())?;
    Ok(network_settings_response(
        &state.downloads.network_settings(),
    ))
}

fn traffic_summary_response(state: &AppState) -> Result<TrafficSummaryResponse, String> {
    state
        .downloads
        .set_utc_offset_seconds(automation::local_utc_offset_seconds());
    let summary = state
        .downloads
        .traffic_summary()
        .map_err(|error| error.to_string())?;
    Ok(TrafficSummaryResponse {
        period_start: summary.period_start,
        explicit_period: summary.explicit_period,
        period_domestic_bytes: summary.period_domestic_bytes,
        period_international_bytes: summary.period_international_bytes,
        today_domestic_bytes: summary.today_domestic_bytes,
        today_international_bytes: summary.today_international_bytes,
        month_domestic_bytes: summary.month_domestic_bytes,
        month_international_bytes: summary.month_international_bytes,
        international_quota: summary.international_quota,
    })
}

#[tauri::command]
fn get_traffic_summary(state: State<'_, AppState>) -> Result<TrafficSummaryResponse, String> {
    traffic_summary_response(&state)
}

/// `quota` in bytes (`None` or 0 for none); `period_start` as `YYYY-MM-DD`,
/// or `None` to count the last 30 days.
#[tauri::command]
fn set_traffic_quota(
    state: State<'_, AppState>,
    quota: Option<u64>,
    period_start: Option<String>,
) -> Result<TrafficSummaryResponse, String> {
    let period_start = period_start
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    state
        .downloads
        .set_traffic_quota(quota, period_start.as_deref())
        .map_err(|error| error.to_string())?;
    traffic_summary_response(&state)
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
/// One download's own speed limit in bytes per second, if it has one.
#[tauri::command]
fn get_download_speed_limit(state: State<'_, AppState>, id: String) -> Result<Option<u64>, String> {
    state
        .downloads
        .task_speed_limit(&id)
        .map_err(|error| error.to_string())
}

/// Sets or removes one download's own speed limit; a running transfer
/// follows at once.
#[tauri::command]
fn set_download_speed_limit(
    state: State<'_, AppState>,
    id: String,
    bytes_per_second: Option<u64>,
) -> Result<Option<u64>, String> {
    state
        .downloads
        .set_task_speed_limit(&id, bytes_per_second)
        .map_err(|error| error.to_string())?;
    state
        .downloads
        .task_speed_limit(&id)
        .map_err(|error| error.to_string())
}

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
    if record.status != DownloadStatus::Completed {
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

/// Windows' "Open with" dialog for a finished download, to pick the program.
#[tauri::command]
fn open_download_with(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let path = completed_file(&state, &id)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // The path is one argument; rundll32 hands it to the shell as is.
        std::process::Command::new("rundll32.exe")
            .arg("shell32.dll,OpenAs_RunDLL")
            .arg(&path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("\"Open with\" is only available on Windows".to_owned())
    }
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

/// Opens the folder an archive was unpacked into. The path comes from the
/// engine's own record, never from the window.
#[tauri::command]
fn open_extracted_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let folder = state
        .downloads
        .download_checks(&id)
        .map_err(|error| error.to_string())?
        .and_then(|checks| checks.extracted_to)
        .map(std::path::PathBuf::from)
        .filter(|folder| folder.is_dir())
        .ok_or_else(|| "the unpacked folder was moved or deleted".to_owned())?;
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

/// Pauses every running transfer; each persists its own pause.
#[tauri::command]
fn pause_all_downloads(state: State<'_, AppState>) -> usize {
    state.downloads.pause_all().len()
}

pub(crate) const SETTING_UI_LANGUAGE: &str = "ui_language";
const SETTING_UI_THEME: &str = "ui_theme";

/// The plain themes and the four brand themes.
fn is_known_theme(value: &str) -> bool {
    matches!(
        value,
        "dark"
            | "light"
            | "system"
            | "ember-forge"
            | "midnight-arcane"
            | "forest-rune"
            | "frost-byte"
    )
}

fn ui_preferences(state: &AppState) -> Result<UiPreferencesResponse, String> {
    let setting = |key: &str| {
        state
            .storage
            .get_setting(key)
            .map_err(|error| error.to_string())
    };
    let language = setting(SETTING_UI_LANGUAGE)?
        .filter(|value| matches!(value.as_str(), "fa" | "en"))
        .unwrap_or_else(|| "fa".to_owned());
    let theme = setting(SETTING_UI_THEME)?
        .filter(|value| is_known_theme(value))
        .unwrap_or_else(|| "ember-forge".to_owned());
    Ok(UiPreferencesResponse {
        language,
        theme,
        close_to_tray: tray::close_to_tray_enabled(state),
    })
}

#[tauri::command]
fn get_ui_preferences(state: State<'_, AppState>) -> Result<UiPreferencesResponse, String> {
    ui_preferences(&state)
}

#[tauri::command]
fn set_ui_preferences(
    state: State<'_, AppState>,
    preferences: UiPreferencesResponse,
) -> Result<UiPreferencesResponse, String> {
    if !matches!(preferences.language.as_str(), "fa" | "en") {
        return Err("unsupported language".to_owned());
    }
    if !is_known_theme(&preferences.theme) {
        return Err("unsupported theme".to_owned());
    }
    let save = |key: &str, value: &str| {
        state
            .storage
            .set_setting(key, value)
            .map_err(|error| error.to_string())
    };
    save(SETTING_UI_LANGUAGE, &preferences.language)?;
    save(SETTING_UI_THEME, &preferences.theme)?;
    save(
        tray::SETTING_CLOSE_TO_TRAY,
        if preferences.close_to_tray {
            "true"
        } else {
            "false"
        },
    )?;
    ui_preferences(&state)
}

/// The window reports the overall speed in the user's language; the tray
/// shows it as its tooltip. Menu labels follow the language too.
#[tauri::command]
fn set_tray_status(
    app: AppHandle,
    state: State<'_, AppState>,
    tooltip: String,
    labels: Option<tray::TrayLabels>,
) {
    let tooltip: String = tooltip.chars().take(120).collect();
    tray::update(&app, &state.tray_menu, &tooltip, labels.as_ref());
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

/// Expands a sequential pattern such as `https://a.test/part[01-20].rar`
/// into links for review in LinkGrabber.
#[tauri::command]
fn generate_links(pattern: String) -> Result<Vec<LinkCandidateResponse>, String> {
    dm_core::linkgrabber::generate_links(&pattern)
        .map(|links| links.into_iter().map(link_candidate_response).collect())
        .map_err(|error| error.to_string())
}

/// Reads a web page and the pages it links to, and returns the links to
/// files found, for review in LinkGrabber.
#[tauri::command]
async fn grab_site(
    state: State<'_, AppState>,
    url: String,
    depth: u8,
    same_host: bool,
    within_folder: bool,
    extensions: Vec<String>,
) -> Result<dm_ipc::SiteGrabResponse, String> {
    let options = dm_core::sitegrab::SiteGrabOptions {
        depth,
        same_host,
        within_folder,
        extensions,
    };
    let result = state
        .downloads
        .grab_site(url.trim(), options)
        .await
        .ok_or_else(|| "enter a web address that starts with http:// or https://".to_owned())?;
    Ok(dm_ipc::SiteGrabResponse {
        files: result
            .files
            .into_iter()
            .map(link_candidate_response)
            .collect(),
        pages_read: result.pages_read,
        truncated: result.truncated,
    })
}

/// Checks links before they become tasks: reachable, size, name, and
/// whether they can be resumed. Bounded in count and concurrency.
#[tauri::command]
async fn probe_links(
    state: State<'_, AppState>,
    urls: Vec<String>,
) -> Result<Vec<dm_ipc::LinkProbeResponse>, String> {
    Ok(state
        .downloads
        .probe_links(urls)
        .await
        .into_iter()
        .map(|probe| dm_ipc::LinkProbeResponse {
            url: probe.url,
            reachable: probe.reachable,
            filename: probe.filename,
            total_bytes: probe.total_bytes,
            content_type: probe.content_type,
            range_supported: probe.range_supported,
            error: probe.error,
        })
        .collect())
}

fn link_candidate_response(
    candidate: dm_core::linkgrabber::LinkCandidate,
) -> LinkCandidateResponse {
    LinkCandidateResponse {
        url: candidate.url,
        host: candidate.host,
        extension: candidate.extension,
    }
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
    /// Rows another program changed (the command-line tool); show them.
    Refresh(Vec<String>),
    /// An action from the command-line tool: pause, resume, cancel with
    /// task ids; pause-all; queue-start or queue-stop with a queue id.
    Control(String, Vec<String>),
}

const ARG_REFRESH: &str = "--refresh";
const ARG_CONTROL: &str = "--control";
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
            ARG_REFRESH if !values.is_empty() => requests.push(LaunchRequest::Refresh(values)),
            ARG_CONTROL => {
                let mut values = values.into_iter();
                if let Some(action) = values.next() {
                    requests.push(LaunchRequest::Control(action, values.collect()));
                }
            }
            _ => {}
        }

        index += 1 + consumed;
    }

    requests
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
                    if let Err(error) = hand_over(app, &state, &id) {
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
                .and_then(|task| hand_over(app, &state, &task.id));
                if let Err(error) = started {
                    warn!(error = %error, "browser handoff could not start");
                }
            }
            LaunchRequest::GrabLinks(urls) => {
                // A video page from the browser opens the small window, where
                // its quality is chosen; other links go to LinkGrabber.
                let videos = !urls.is_empty()
                    && urls.len() <= 20
                    && urls
                        .iter()
                        .all(|url| dm_core::ytdlp::handles(dm_core::ytdlp::without_fragment(url)));
                if videos && mini::compact(&state.storage) && !mini::main_in_view(app) {
                    mini::open_add(app, urls);
                    continue;
                }
                if let Ok(mut pending) = state.pending_link_intake.lock() {
                    pending.extend(urls.iter().cloned());
                }
                // A window that is already listening takes the links now; one
                // still loading collects them with `take_pending_link_intake`.
                let _ = app.emit(LINK_INTAKE_EVENT, ());
            }
            LaunchRequest::Refresh(ids) => {
                let publisher = EventPublisher::new(app.clone());
                for id in ids {
                    publisher.download_refreshed(&id);
                }
            }
            LaunchRequest::Control(action, ids) => {
                if let Err(error) = run_control(app, &state, &action, &ids) {
                    warn!(action = %action, error = %error, "command-line request failed");
                }
            }
        }
    }

    tray::show_main_window(app);
}

/// Starts or queues a task that arrived from the browser, applying the same
/// intake rules as Add Download, and shows the row immediately.
/// Carries out a request from the command-line tool with the same rules as
/// the buttons in the window.
fn run_control(
    app: &AppHandle,
    state: &AppState,
    action: &str,
    ids: &[String],
) -> Result<(), String> {
    let publisher = EventPublisher::new(app.clone());
    match action {
        "pause" => {
            for id in ids {
                match state.downloads.pause_task(id) {
                    Ok(record) => publisher.download_updated(record),
                    Err(error) => warn!(download_id = %id, error = %error, "pause refused"),
                }
            }
        }
        "resume" => {
            for id in ids {
                match resume_now(app, state, id) {
                    Ok(record) => publisher.download_updated(record),
                    Err(error) => warn!(download_id = %id, error = %error, "resume refused"),
                }
            }
        }
        "cancel" => {
            for id in ids {
                let id = id.clone();
                let downloads = state.downloads.clone();
                let publisher = publisher.clone();
                tauri::async_runtime::spawn(async move {
                    match downloads.cancel_task(&id).await {
                        Ok(record) => publisher.download_updated(record),
                        Err(error) => warn!(download_id = %id, error = %error, "cancel refused"),
                    }
                });
            }
        }
        "pause-all" => {
            for id in state.downloads.pause_all() {
                publisher.download_refreshed(&id);
            }
        }
        "queue-start" => {
            for id in ids {
                start_queue_now(app, state, id)?;
            }
        }
        "queue-stop" => {
            for id in ids {
                state
                    .queues
                    .stop_queue(id)
                    .map_err(|error| error.to_string())?;
            }
        }
        other => return Err(format!("unknown action {other:?}")),
    }
    Ok(())
}

/// Continues a task: back to its queue if it has one, otherwise now.
fn resume_now(app: &AppHandle, state: &AppState, id: &str) -> Result<DownloadRecord, String> {
    let record = state
        .storage
        .get_download(id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;

    if let Some(queue_id) = record.queue_id.clone() {
        return state
            .queues
            .enqueue_task(id, &queue_id, Some(record.priority))
            .map_err(|error| error.to_string());
    }

    let destination_directory = app
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;
    let claimed = state
        .downloads
        .claim_task(id)
        .map_err(|error| error.to_string())?;
    spawn_transfer(
        state.downloads.clone(),
        EventPublisher::new(app.clone()),
        id.to_owned(),
        destination_directory,
    );
    Ok(claimed)
}

fn start_queue_now(
    app: &AppHandle,
    state: &AppState,
    queue_id: &str,
) -> Result<QueueRecord, String> {
    let destination_directory = app
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;
    let queue = state
        .queues
        .start_queue(queue_id)
        .map_err(|error| error.to_string())?;

    // A queue resumed at startup is already running; starting it again must
    // reuse that runner rather than spawn a second one that immediately fails.
    if state
        .queues
        .is_running(queue_id)
        .map_err(|error| error.to_string())?
    {
        info!(queue_id = %queue_id, "queue runner already active");
        return Ok(queue);
    }

    info!(queue_id = %queue_id, "starting queue runner");
    spawn_queue_runner(
        state.queues.clone(),
        EventPublisher::new(app.clone()),
        queue_id.to_owned(),
        destination_directory,
    );
    Ok(queue)
}

/// Starts a task the browser handed over. When it is a fresh link for a
/// download that stopped part-way, that download continues instead (see
/// `DownloadService::adopt_fresh_link`); the check needs a request to the
/// server, so it runs in the background.
/// A download the browser handed over: shown in the small window, which
/// asks where to save it before it starts, or (with the main window chosen
/// in Settings) started at once.
fn hand_over(app: &AppHandle, state: &AppState, task_id: &str) -> Result<(), String> {
    if mini::compact(&state.storage) {
        EventPublisher::new(app.clone()).download_refreshed(task_id);
        mini::open_task(app, task_id, true);
        return Ok(());
    }
    start_handoff_task(app, state, task_id)
}

/// Starts a download the small window confirmed, and answers with the id
/// that is actually downloading: a fresh link can continue an earlier,
/// stopped download of the same file instead.
#[tauri::command]
async fn start_handoff(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let target = if state.downloads.may_adopt(&id) {
        match state.downloads.adopt_fresh_link(&id).await {
            Ok(Some(adopted)) => {
                info!(download_id = %adopted.id, "fresh link continues a stopped download");
                EventPublisher::new(app.clone()).download_removed(&id);
                adopted.id
            }
            _ => id,
        }
    } else {
        id
    };
    match state.storage.get_download(&target) {
        Ok(Some(record))
            if record.queue_id.is_some() && record.status == DownloadStatus::Created =>
        {
            resume_now(&app, &state, &target).map(|_| ())?;
        }
        _ => start_handoff_task_now(&app, &state, &target)?,
    }
    Ok(target)
}

/// One download, as the list shows it.
#[tauri::command]
fn get_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DownloadListItemResponse>, String> {
    state
        .storage
        .get_download(&id)
        .map(|record| record.map(download_list_item_response))
        .map_err(|error| error.to_string())
}

/// Links for the small add window; each is returned once.
#[tauri::command]
fn take_mini_links(state: State<'_, AppState>) -> Vec<String> {
    state
        .pending_mini_links
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default()
}

/// Whether the floating drop box is shown.
#[tauri::command]
fn get_drop_box(state: State<'_, AppState>) -> bool {
    mini::drop_box_enabled(&state.storage)
}

#[tauri::command]
fn set_drop_box(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state
        .storage
        .set_setting(
            mini::SETTING_DROP_BOX,
            if enabled { "true" } else { "false" },
        )
        .map_err(|error| error.to_string())?;
    mini::set_drop_box(&app, enabled);
    Ok(enabled)
}

/// Links dropped on the drop box (or pasted into it): they open where
/// copied links do, the small add window or the main window's Add dialog.
#[tauri::command]
fn add_dropped_links(app: AppHandle, state: State<'_, AppState>, text: String) -> usize {
    let links: Vec<String> = dm_core::linkgrabber::extract_links(&text)
        .into_iter()
        .map(|link| link.url)
        .take(500)
        .collect();
    if links.is_empty() {
        return 0;
    }
    let count = links.len();
    if mini::compact(&state.storage) {
        mini::open_add(&app, links);
    } else {
        tray::show_main_window(&app);
        let _ = app.emit(clipboard_watch::CLIPBOARD_LINKS_EVENT, links);
    }
    count
}

/// Opens the progress window of a download started from the add window.
#[tauri::command]
async fn open_download_window(app: AppHandle, id: String) {
    mini::open_task(&app, &id, false);
}

/// What happens once every download has finished: `none`, `sleep`,
/// `hibernate`, `shutdown` or `exit_app`. For this session only.
#[tauri::command]
fn get_after_all(state: State<'_, AppState>) -> String {
    state
        .automation
        .after_all()
        .unwrap_or(CompletionAction::None)
        .as_str()
        .to_owned()
}

#[tauri::command]
fn set_after_all(state: State<'_, AppState>, action: String) -> Result<String, String> {
    let parsed = action
        .parse::<CompletionAction>()
        .map_err(|_| "unknown action".to_owned())?;
    if parsed == CompletionAction::Notify {
        return Err("unknown action".to_owned());
    }
    state.automation.set_after_all(Some(parsed));
    Ok(get_after_all(state))
}

/// What happens when one download finishes: `none`, `open`, or a power
/// action.
#[tauri::command]
fn get_download_after(state: State<'_, AppState>, id: String) -> String {
    state
        .automation
        .after_download(&id)
        .map_or("none", automation::AfterDownload::as_str)
        .to_owned()
}

#[tauri::command]
fn set_download_after(
    state: State<'_, AppState>,
    id: String,
    action: String,
) -> Result<String, String> {
    let parsed = automation::AfterDownload::parse(&action);
    if parsed.is_none() && action != "none" {
        return Err("unknown action".to_owned());
    }
    state.automation.set_after_download(&id, parsed);
    Ok(get_download_after(state, id))
}

/// Brings the main window forward; with `focus`, on that download.
#[tauri::command]
fn show_main_window(app: AppHandle, focus: Option<String>) {
    tray::show_main_window(&app);
    if let Some(id) = focus {
        let _ = app.emit_to("main", FOCUS_DOWNLOAD_EVENT, id);
    }
}

const FOCUS_DOWNLOAD_EVENT: &str = "focus-download";

const SETTING_ONBOARDING_DONE: &str = "onboarding_done";

/// Whether the first-run guide has been completed or skipped.
#[tauri::command]
fn get_onboarding_done(state: State<'_, AppState>) -> bool {
    state
        .storage
        .get_setting(SETTING_ONBOARDING_DONE)
        .ok()
        .flatten()
        .as_deref()
        == Some("true")
}

#[tauri::command]
fn set_onboarding_done(state: State<'_, AppState>, done: bool) -> Result<(), String> {
    state
        .storage
        .set_setting(SETTING_ONBOARDING_DONE, if done { "true" } else { "false" })
        .map_err(|error| error.to_string())
}

/// `compact` for the small download window, `main` for the main window.
#[tauri::command]
fn get_intake_window(state: State<'_, AppState>) -> String {
    if mini::compact(&state.storage) {
        "compact".to_owned()
    } else {
        "main".to_owned()
    }
}

#[tauri::command]
fn set_intake_window(state: State<'_, AppState>, value: String) -> Result<String, String> {
    if !matches!(value.as_str(), "compact" | "main") {
        return Err("unknown window choice".to_owned());
    }
    state
        .storage
        .set_setting(mini::SETTING_INTAKE_WINDOW, &value)
        .map_err(|error| error.to_string())?;
    Ok(value)
}

fn start_handoff_task(app: &AppHandle, state: &AppState, task_id: &str) -> Result<(), String> {
    if !state.downloads.may_adopt(task_id) {
        return start_handoff_task_now(app, state, task_id);
    }
    let app = app.clone();
    let task_id = task_id.to_owned();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let target = match state.downloads.adopt_fresh_link(&task_id).await {
            Ok(Some(adopted)) => {
                info!(download_id = %adopted.id, "fresh link continues a stopped download");
                EventPublisher::new(app.clone()).download_removed(&task_id);
                adopted.id
            }
            _ => task_id,
        };
        let started = match state.storage.get_download(&target) {
            Ok(Some(record))
                if record.queue_id.is_some() && record.status == DownloadStatus::Created =>
            {
                resume_now(&app, &state, &target).map(|_| ())
            }
            _ => start_handoff_task_now(&app, &state, &target),
        };
        if let Err(error) = started {
            warn!(download_id = %target, error = %error, "browser handoff could not start");
        }
    });
    Ok(())
}

fn start_handoff_task_now(app: &AppHandle, state: &AppState, task_id: &str) -> Result<(), String> {
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

/// Receives browser sessions from the native messaging host for as long as
/// the application runs.
///
/// A session arrives together with the task it belongs to and is attached
/// before the task starts, so the very first request already carries it. It
/// is held in memory only; nothing here logs it, stores it, or sends it to
/// the window.
async fn serve_browser_sessions(app: AppHandle) {
    let handler_app = app.clone();

    let result = dm_system::session_channel::serve(move |handoff| {
        receive_browser_session(&handler_app, &handoff.task_id, &handoff.cookie)
    })
    .await;

    match result {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
            info!("browser session handover is not available on this system");
        }
        Err(error) => {
            warn!(error = %error, "browser session channel stopped");
        }
    }
}

fn receive_browser_session(
    app: &AppHandle,
    task_id: &str,
    cookie: &str,
) -> dm_system::session_channel::HandoffReply {
    use dm_system::session_channel::HandoffReply;

    let Some(state) = app.try_state::<AppState>() else {
        return HandoffReply::refused("Ratatoskr is still starting");
    };

    if let Err(error) = state.downloads.attach_browser_session(task_id, cookie) {
        warn!(download_id = %task_id, error = %error, "browser session refused");
        return HandoffReply::refused(error.to_string());
    }

    if let Err(error) = hand_over(app, &state, task_id) {
        state.downloads.forget_browser_session(task_id);
        warn!(download_id = %task_id, error = %error, "browser handoff could not start");
        return HandoffReply::refused(error);
    }

    info!(download_id = %task_id, "browser handoff accepted with a browser session");
    if !mini::compact(&state.storage) {
        tray::show_main_window(app);
    }
    HandoffReply::accepted()
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
                warn!(download_id = %download_id, error = %error.log_message(), "transfer failed");
                publisher.download_refreshed(&download_id);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    portable::prepare_webview();
    init_logging();

    let launch_requests = parse_launch_args(&std::env::args().skip(1).collect::<Vec<_>>());

    info!("starting Ratatoskr");

    tauri::Builder::default()
        // Registered first so a second launch exits before it opens a window
        // or the database; its arguments are handled by this process instead.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let requests = parse_launch_args(args.get(1..).unwrap_or_default());
            if requests.is_empty() {
                tray::show_main_window(app);
            } else {
                handle_launch_requests(app, requests);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            let app_data_dir = match portable::app_data(app.handle()) {
                Some(folder) => folder,
                None => app.path().app_data_dir()?,
            };
            let database_path = app_data_dir.join("downloads.db");

            // A restore chosen in the previous run is swapped in before the
            // database is opened. The replaced database is kept beside it.
            let restore_outcome = match dm_storage::apply_pending_restore(&database_path) {
                Ok(None) => None,
                Ok(Some(dm_storage::RestoreOutcome::Restored { kept_copy })) => {
                    info!(kept = %kept_copy.display(), "restored the database from a backup");
                    Some(RestoreOutcomeResponse {
                        restored: true,
                        kept_copy: Some(kept_copy.to_string_lossy().into_owned()),
                        reason: None,
                    })
                }
                Ok(Some(dm_storage::RestoreOutcome::Refused { reason })) => {
                    warn!(reason = %reason, "a waiting restore was refused");
                    Some(RestoreOutcomeResponse {
                        restored: false,
                        kept_copy: None,
                        reason: Some(reason),
                    })
                }
                Err(error) => {
                    warn!(error = %error, "a waiting restore could not be applied");
                    Some(RestoreOutcomeResponse {
                        restored: false,
                        kept_copy: None,
                        reason: Some(error.to_string()),
                    })
                }
            };

            let storage = Arc::new(Storage::open(&database_path)?);

            let downloads = DownloadService::new(Arc::clone(&storage))?;
            downloads.set_utc_offset_seconds(automation::local_utc_offset_seconds());

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
                pending_mini_links: Mutex::new(Vec::new()),
                automation: Automation::new(),
                tray_menu: Mutex::new(None),
                restore_outcome: Mutex::new(restore_outcome),
            });

            // A missing notification area (some Linux desktops) must not stop
            // the application from starting.
            let tray_ready = match tray::create(app.handle()) {
                Ok(menu) => {
                    if let Ok(mut slot) = app.state::<AppState>().tray_menu.lock() {
                        *slot = Some(menu);
                    }
                    true
                }
                Err(error) => {
                    warn!(error = %error, "tray icon unavailable");
                    false
                }
            };

            // Started by Windows at sign-in: stay in the tray. Without a tray
            // the window is the only way back, so it stays visible.
            // Also when started only to take a download from the browser:
            // the small download window is all that should appear.
            let started_hidden = std::env::args().any(|argument| {
                argument == dm_system::autostart::HIDDEN_ARGUMENT
                    || (mini::compact(&storage)
                        && matches!(
                            argument.as_str(),
                            ARG_HANDOFF_TASK | ARG_BROWSER_HANDOFF | ARG_GRAB_LINKS
                        ))
            });
            if started_hidden && tray_ready {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            if mini::drop_box_enabled(&storage) {
                mini::set_drop_box(app.handle(), true);
            }

            tauri::async_runtime::spawn(automation::run_keep_awake(app.handle().clone()));
            tauri::async_runtime::spawn(serve_browser_sessions(app.handle().clone()));

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

            browser_setup::register_quietly(app.handle());

            tools::configure(app.handle(), &downloads);
            tauri::async_runtime::spawn(tools::run_maintenance(
                app.handle().clone(),
                downloads.clone(),
                Arc::clone(&storage),
            ));

            tauri::async_runtime::spawn(updates::run_background_checks(
                app.handle().clone(),
                Arc::clone(&storage),
            ));

            tauri::async_runtime::spawn(clipboard_watch::run(
                app.handle().clone(),
                Arc::clone(&storage),
            ));

            tauri::async_runtime::spawn(run_queue_scheduler(
                app.handle().clone(),
                Arc::clone(&storage),
                queues.clone(),
                publisher.clone(),
                destination_directory.clone(),
            ));

            // After-download results arrive from the engine in the
            // background; the row (for notices) and the details panel follow.
            let mut post_events = downloads.subscribe_post_process();
            let checks_publisher = publisher.clone();
            let checks_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match post_events.recv().await {
                        Ok(id) => {
                            checks_publisher.download_refreshed(&id);
                            let _ = checks_app.emit(DOWNLOAD_CHECKS_EVENT, id);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            info!("application state initialized");

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing hides to the tray so downloads keep running; Quit in
            // the tray menu really exits.
            // Only the main window: a small download window really closes.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                let app = window.app_handle();
                let hide = app.try_state::<AppState>().is_some_and(|state| {
                    tray::close_to_tray_enabled(&state)
                        && state.tray_menu.lock().is_ok_and(|menu| menu.is_some())
                });
                if hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_download_speed_limit,
            get_clipboard_watch,
            get_start_with_windows,
            get_finish_sound,
            get_keep_server_time,
            set_keep_server_time,
            set_finish_sound,
            set_start_with_windows,
            get_browser_connection,
            connect_browsers,
            reveal_extension_folder,
            open_browser_extensions_page,
            install_firefox_extension,
            check_for_update,
            install_update,
            get_auto_update_check,
            set_auto_update_check,
            set_clipboard_watch,
            get_ytdlp_status,
            set_ytdlp_path,
            update_ytdlp,
            install_ffmpeg,
            set_download_speed_limit,
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
            generate_links,
            probe_links,
            grab_site,
            take_pending_link_intake,
            get_download_settings,
            set_default_download_directory,
            set_global_speed_limit,
            set_max_connections,
            list_download_mirrors,
            set_download_mirrors,
            list_stream_variants,
            probe_video,
            set_download_folder,
            set_download_name,
            start_handoff,
            get_download,
            take_mini_links,
            get_drop_box,
            set_drop_box,
            add_dropped_links,
            open_download_window,
            show_main_window,
            get_after_all,
            set_after_all,
            get_download_after,
            set_download_after,
            get_intake_window,
            set_intake_window,
            get_onboarding_done,
            set_onboarding_done,
            get_engine_settings,
            get_ffmpeg_status,
            set_ffmpeg_path,
            get_download_stats,
            backup_database,
            stage_restore,
            get_pending_restore,
            cancel_restore,
            take_restore_outcome,
            restart_app,
            export_downloads,
            get_diagnostics_report,
            save_diagnostics_report,
            check_database,
            check_connection,
            get_download_checks,
            set_expected_checksum,
            run_post_process,
            get_post_process_settings,
            set_post_process_settings,
            set_engine_settings,
            get_network_settings,
            set_network_settings,
            get_traffic_summary,
            set_traffic_quota,
            set_prevent_sleep,
            set_category_directory,
            save_download_rule,
            delete_download_rule,
            cancel_completion_action,
            open_download_file,
            open_download_with,
            open_extracted_folder,
            reveal_download_file,
            pause_all_downloads,
            get_ui_preferences,
            set_ui_preferences,
            set_tray_status,
            classify_media_source,
            parse_hls_manifest
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ratatoskr");
}

#[cfg(test)]
mod tests {
    #[test]
    fn plain_and_brand_themes_are_known_and_others_are_not() {
        for theme in [
            "dark",
            "light",
            "system",
            "ember-forge",
            "midnight-arcane",
            "forest-rune",
            "frost-byte",
        ] {
            assert!(super::is_known_theme(theme), "{theme}");
        }
        assert!(!super::is_known_theme("neon"));
        assert!(!super::is_known_theme(""));
    }

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
    fn parses_the_command_line_tool_switches() {
        assert_eq!(
            parse_launch_args(&args(&["--control", "pause", "a", "b"])),
            vec![LaunchRequest::Control(
                "pause".to_owned(),
                args(&["a", "b"])
            )]
        );
        assert_eq!(
            parse_launch_args(&args(&["--control", "pause-all"])),
            vec![LaunchRequest::Control("pause-all".to_owned(), Vec::new())]
        );
        assert_eq!(
            parse_launch_args(&args(&["--refresh", "t1"])),
            vec![LaunchRequest::Refresh(args(&["t1"]))]
        );
        assert!(parse_launch_args(&args(&["--control"])).is_empty());
        assert!(parse_launch_args(&args(&["--refresh"])).is_empty());
    }

    #[test]
    fn ignores_unknown_and_empty_switches() {
        assert!(parse_launch_args(&args(&[])).is_empty());
        assert!(parse_launch_args(&args(&["--handoff-task"])).is_empty());
        assert!(parse_launch_args(&args(&["--unknown", "x", "stray"])).is_empty());
    }
}
