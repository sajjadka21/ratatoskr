//! Setting up the browser extension from inside the app: the connector
//! (native messaging host) is registered for the current user on every
//! start, and Settings shows what is left to do in each browser.

use dm_system::browser_hosts::{self, Browser};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{path::BaseDirectory, AppHandle, Manager, Runtime};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConnection {
    /// The connector program is installed next to the app.
    pub host_found: bool,
    /// Browsers that can now reach the app: `chrome`, `edge`, `brave`,
    /// `chromium`, `firefox`.
    pub registered: Vec<String>,
    /// The extension folder shipped with the app, for loading it by hand.
    pub extension_folder: Option<String>,
    pub chromium_extension_id: String,
}

fn manifests_folder<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|folder| folder.join("native-messaging"))
}

fn host_program() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| browser_hosts::host_beside(&exe))
}

pub fn extension_folder<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .resolve("browser-extension", BaseDirectory::Resource)
        .ok()
        .filter(|folder| folder.join("manifest.json").is_file())
}

/// Registers the connector for every supported browser. Quiet when the
/// connector program is not there (a development build without it).
pub fn register<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<Browser>, String> {
    let host =
        host_program().ok_or_else(|| "the browser connector program is missing".to_owned())?;
    let folder = manifests_folder(app).ok_or_else(|| "no application data folder".to_owned())?;
    let registered = browser_hosts::register(&host, &folder).map_err(|error| error.to_string())?;
    info!(browsers = registered.len(), "browser connector registered");
    Ok(registered)
}

pub fn register_quietly<R: Runtime>(app: &AppHandle<R>) {
    if host_program().is_none() {
        return;
    }
    if let Err(error) = register(app) {
        warn!(error = %error, "could not register the browser connector");
    }
}

pub fn connection<R: Runtime>(app: &AppHandle<R>) -> BrowserConnection {
    BrowserConnection {
        host_found: host_program().is_some(),
        registered: browser_hosts::registered()
            .into_iter()
            .map(|browser| browser.name().to_owned())
            .collect(),
        extension_folder: extension_folder(app).map(|folder| folder.to_string_lossy().into_owned()),
        chromium_extension_id: browser_hosts::CHROMIUM_EXTENSION_ID.to_owned(),
    }
}

/// Opens a browser's extensions page, where the extension folder is loaded.
/// Only these fixed pages and programs are ever started.
pub fn open_extensions_page(browser: &str) -> Result<(), String> {
    let (program, page) = match browser {
        "chrome" => ("chrome", "chrome://extensions"),
        "edge" => ("msedge", "edge://extensions"),
        "brave" => ("brave", "brave://extensions"),
        "firefox" => ("firefox", "about:debugging#/runtime/this-firefox"),
        _ => return Err("unknown browser".to_owned()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `start` finds the browser through Windows' App Paths registration.
        std::process::Command::new("cmd")
            .args(["/C", "start", "", program, page])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(program)
            .arg(page)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
