use dm_common::DownloadRecord;
use dm_core::{service::DownloadService, CoreService};
use dm_ipc::{
    AppInfoResponse, ComponentHealth, DownloadListItemResponse, DownloadTaskEvent,
    HealthCheckResponse,
};
use dm_storage::Storage;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(100);

pub struct AppState {
    core: CoreService,
    storage: Arc<Storage>,
    downloads: DownloadService,
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
        total_bytes: record.total_bytes,
        downloaded_bytes: record.downloaded_bytes,
        status: record.status.to_string(),
        created_at: record.created_at,
        started_at: record.started_at,
        completed_at: record.completed_at,
        error_code: record.error_code,
        error_message: record.error_message,
    }
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

    let status = record.status.as_str();

    if !matches!(status, "completed" | "failed" | "cancelled") {
        return Err(format!("cannot remove download while status is '{status}'"));
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
    state
        .downloads
        .create_task(&url)
        .map(download_list_item_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    on_event: Channel<DownloadTaskEvent>,
) -> Result<DownloadListItemResponse, String> {
    let destination_directory = app
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;

    let service = state.downloads.clone();
    let storage = Arc::clone(&state.storage);
    let claimed = service.claim_task(&id).map_err(|error| error.to_string())?;
    let response = download_list_item_response(claimed);
    let task_id = id.clone();
    let progress_events = on_event.clone();

    info!(download_id = %task_id, "starting background download");

    tauri::async_runtime::spawn(async move {
        let mut last_progress_event_at = Instant::now();
        let mut sent_progress_event = false;

        let result = service
            .execute_claimed_task_with_progress(
                &task_id,
                &destination_directory,
                move |download_id, progress| {
                    let reached_known_end = progress.total_bytes == Some(progress.downloaded_bytes);
                    let should_send = !sent_progress_event
                        || reached_known_end
                        || last_progress_event_at.elapsed() >= PROGRESS_EVENT_INTERVAL;

                    if !should_send {
                        return;
                    }

                    let event = DownloadTaskEvent::progress(
                        download_id,
                        progress.downloaded_bytes,
                        progress.total_bytes,
                    );

                    if let Err(error) = progress_events.send(event) {
                        warn!(
                            error = %error,
                            "failed to send background download event"
                        );
                    }

                    sent_progress_event = true;
                    last_progress_event_at = Instant::now();
                },
            )
            .await;

        match result {
            Ok(record) => {
                info!(download_id = %task_id, "background download completed");
                let _ = on_event.send(DownloadTaskEvent::updated(download_list_item_response(
                    record,
                )));
            }
            Err(_) => {
                warn!(download_id = %task_id, "background download failed");

                if let Ok(Some(record)) = storage.get_download(&task_id) {
                    let _ = on_event.send(DownloadTaskEvent::updated(download_list_item_response(
                        record,
                    )));
                }
            }
        }
    });

    Ok(response)
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

            info!(
                database = %storage.path().display(),
                schema_version = storage.schema_version()?,
                "storage initialized"
            );

            app.manage(AppState {
                core: CoreService::new(),
                storage,
                downloads,
            });

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
            start_download
        ])
        .run(tauri::generate_context!())
        .expect("error while running Download Manager");
}
