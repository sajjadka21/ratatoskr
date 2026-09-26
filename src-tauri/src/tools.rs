//! Helper programs that ship with the app and keep themselves current.
//!
//! yt-dlp is installed with Ratatosk (in the `extras` resources). On first
//! start it is copied into the app's data folder, where it may update itself:
//! sites like YouTube change often and an old yt-dlp soon stops working.
//! Without a shipped copy (a development build), the latest official release
//! is downloaded once, through the app's own network route, and kept only if
//! it matches its published checksum. A yt-dlp the user chose in Settings is
//! never touched.

use dm_core::{service::DownloadService, ytdlp};
use dm_storage::Storage;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{path::BaseDirectory, AppHandle, Manager, Runtime};
use tracing::{info, warn};

const FIRST_RUN_DELAY: Duration = Duration::from_secs(20);
const UPDATE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// The app's own copies, which it may replace and update.
pub fn managed_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|folder| folder.join("tools"))
}

/// The copies installed with the app (read-only in Program Files).
pub fn bundled_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().resolve("extras", BaseDirectory::Resource).ok()
}

pub fn configure<R: Runtime>(app: &AppHandle<R>, downloads: &DownloadService) {
    downloads.set_tool_dirs(
        [managed_dir(app), bundled_dir(app)]
            .into_iter()
            .flatten()
            .collect(),
    );
}

/// Makes sure the app's own yt-dlp exists and, when `update` is set, is
/// current. Returns a short description of what happened.
pub async fn ensure_ytdlp<R: Runtime>(
    app: &AppHandle<R>,
    downloads: &DownloadService,
    update: bool,
) -> Result<&'static str, String> {
    if downloads.ytdlp_path_setting().is_some() {
        return Ok("chosen by the user");
    }
    let managed = managed_dir(app)
        .ok_or_else(|| "no application data folder".to_owned())?
        .join(ytdlp::program_name());
    if !managed.is_file() {
        let bundled = bundled_dir(app).map(|folder| folder.join(ytdlp::program_name()));
        if let Some(bundled) = bundled.filter(|path| path.is_file()) {
            let bytes = tokio::fs::read(&bundled)
                .await
                .map_err(|error| error.to_string())?;
            ytdlp::write_program(&managed, &bytes)
                .await
                .map_err(|error| error.to_string())?;
            info!("yt-dlp copied from the installation");
        } else {
            downloads
                .install_ytdlp(&managed)
                .await
                .map_err(|error| error.to_string())?;
            info!("yt-dlp installed from its official release");
            return Ok("installed");
        }
    }
    if update {
        let proxy = downloads.tool_proxy_for(ytdlp::RELEASE_BASE);
        ytdlp::YtDlp::new(&managed)
            .self_update(&proxy)
            .await
            .map_err(|error| error.to_string())?;
        return Ok("updated");
    }
    Ok("ready")
}

/// Shortly after start and then daily: install if missing, and update when
/// automatic updates are allowed.
pub async fn run_maintenance(app: AppHandle, downloads: DownloadService, storage: Arc<Storage>) {
    tokio::time::sleep(FIRST_RUN_DELAY).await;
    loop {
        let update = crate::updates::auto_check_enabled(&storage);
        if let Err(error) = ensure_ytdlp(&app, &downloads, update).await {
            warn!(error = %error, "could not prepare yt-dlp");
        }
        tokio::time::sleep(UPDATE_INTERVAL).await;
    }
}
