//! Updates of the application itself.
//!
//! Every release is signed with the project's private key and the app
//! accepts only files whose signature matches the public key built into it,
//! so a changed or fake installer is refused. Checking happens in the
//! background (once shortly after start, then daily) when the user allows
//! it; installing only ever happens when the user asks.
//!
//! Where releases are published is set in `tauri.conf.json`
//! (`plugins.updater.endpoints`); see `RELEASING.md`. Without an address the
//! check reports that updates are not set up and nothing is contacted.

use dm_core::network::{NetworkSettings, ProxyMode};
use dm_storage::Storage;
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

pub const SETTING_AUTO_UPDATE_CHECK: &str = "auto_update_check";
pub const UPDATE_AVAILABLE_EVENT: &str = "update-available";
pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(90);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const NOT_CONFIGURED: &str = "not_configured";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// On unless the user turned it off.
pub fn auto_check_enabled(storage: &Storage) -> bool {
    storage
        .get_setting(SETTING_AUTO_UPDATE_CHECK)
        .ok()
        .flatten()
        .as_deref()
        != Some("false")
}

fn updater<R: Runtime>(
    app: &AppHandle<R>,
    storage: &Storage,
) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder().timeout(Duration::from_secs(60));
    // The update travels the way downloads do when a proxy is set by hand.
    let network = NetworkSettings::load(storage);
    let manual_proxy = (network.mode == ProxyMode::Manual)
        .then(|| network.proxy_url.as_deref())
        .flatten()
        .and_then(|url| url.parse::<tauri::Url>().ok());
    if let Some(proxy) = manual_proxy {
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|error| match error {
        tauri_plugin_updater::Error::EmptyEndpoints => NOT_CONFIGURED.to_owned(),
        other => other.to_string(),
    })
}

/// The newer version on offer, if there is one.
pub async fn check<R: Runtime>(
    app: &AppHandle<R>,
    storage: &Storage,
) -> Result<Option<UpdateInfo>, String> {
    let updater = updater(app, storage)?;
    let update = updater.check().await.map_err(|error| error.to_string())?;
    Ok(update.map(|update| UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
    }))
}

/// Downloads, verifies and installs the newer version, then restarts into
/// it. Returns only when there was nothing to install or it failed.
pub async fn install<R: Runtime>(app: &AppHandle<R>, storage: &Storage) -> Result<(), String> {
    let updater = updater(app, storage)?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(());
    };
    info!(version = %update.version, "installing update");
    let mut downloaded = 0_u64;
    let progress_app = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress_app.emit(
                    UPDATE_PROGRESS_EVENT,
                    UpdateProgress { downloaded, total },
                );
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())?;
    app.restart();
}

/// Checks shortly after start and then daily, when allowed.
pub async fn run_background_checks(app: AppHandle, storage: Arc<Storage>) {
    tokio::time::sleep(FIRST_CHECK_DELAY).await;
    loop {
        if auto_check_enabled(&storage) {
            match check(&app, &storage).await {
                Ok(Some(update)) => {
                    let _ = app.emit(UPDATE_AVAILABLE_EVENT, update);
                }
                Ok(None) => {}
                Err(error) if error == NOT_CONFIGURED => return,
                Err(error) => warn!(error = %error, "update check failed"),
            }
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}
