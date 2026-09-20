use dm_core::CoreService;
use dm_ipc::{AppInfoResponse, ComponentHealth, HealthCheckResponse};
use dm_storage::Storage;
use tauri::{Manager, State};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub struct AppState {
    core: CoreService,
    storage: Storage,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    info!("starting Download Manager");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database_path = app_data_dir.join("downloads.db");

            let storage = Storage::open(&database_path)?;

            info!(
                database = %storage.path().display(),
                schema_version = storage.schema_version()?,
                "storage initialized"
            );

            app.manage(AppState {
                core: CoreService::new(),
                storage,
            });

            info!("application state initialized");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_app_info, health_check])
        .run(tauri::generate_context!())
        .expect("error while running Download Manager");
}
