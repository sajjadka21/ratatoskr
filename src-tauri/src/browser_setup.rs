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
    /// The connector program is present beside the app or in bundled resources.
    pub host_found: bool,
    /// Browsers with a valid native-host registry entry. This does not prove
    /// that the extension is installed or enabled in that browser.
    pub registered: Vec<String>,
    pub connected: Vec<String>,
    /// The extension folder shipped with the app, for loading it by hand.
    pub extension_folder: Option<String>,
    pub chromium_extension_id: String,
    /// A Firefox package signed by Mozilla ships with the app, so Firefox
    /// can install it with one confirmation.
    pub firefox_package: bool,
}

fn manifests_folder<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    crate::portable::app_data(app).map(|folder| folder.join("native-messaging"))
}

fn host_program<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let resources = app
        .path()
        .resolve("dm-native-host.exe", BaseDirectory::Resource)
        .ok();
    browser_hosts::host_for(&executable, resources.as_deref())
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
        host_program(app).ok_or_else(|| "the browser connector program is missing".to_owned())?;
    let folder = manifests_folder(app).ok_or_else(|| "no application data folder".to_owned())?;
    let registered = browser_hosts::register(&host, &folder).map_err(|error| error.to_string())?;
    info!(browsers = registered.len(), "browser connector registered");
    Ok(registered)
}

pub fn register_quietly<R: Runtime>(app: &AppHandle<R>) {
    if host_program(app).is_none() {
        return;
    }
    if let Err(error) = register(app) {
        warn!(error = %error, "could not register the browser connector");
    }
}

pub fn connection<R: Runtime>(app: &AppHandle<R>) -> BrowserConnection {
    let storage = &app.state::<crate::AppState>().storage;
    let records: Vec<_> = dm_system::browser_health::BROWSERS
        .iter()
        .filter_map(|browser| {
            let key = format!("browser_ping_{browser}");
            storage
                .get_setting(&key)
                .ok()
                .flatten()
                .map(|value| (key, value))
        })
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    BrowserConnection {
        connected: dm_system::browser_health::connected_browsers(&records, now),
        host_found: host_program(app).is_some(),
        registered: browser_hosts::registered()
            .into_iter()
            .map(|browser| browser.name().to_owned())
            .collect(),
        extension_folder: extension_folder(app).map(|folder| folder.to_string_lossy().into_owned()),
        chromium_extension_id: browser_hosts::CHROMIUM_EXTENSION_ID.to_owned(),
        firefox_package: firefox_package(app).is_some(),
    }
}

/// The Mozilla-signed extension package, when it ships with the app
/// (`src-tauri/extras/ratatosk-firefox.xpi`; see RELEASING.md).
pub fn firefox_package<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .resolve("extras/ratatosk-firefox.xpi", BaseDirectory::Resource)
        .ok()
        .filter(|path| path.is_file())
}

/// Opens the signed package in Firefox, which asks once to add it.
pub fn install_in_firefox<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let package = firefox_package(app)
        .ok_or_else(|| "no Firefox package ships with this build".to_owned())?;
    launch("firefox", &package.to_string_lossy())
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
    launch(program, page)
}

/// Starts a browser program with one argument. Only fixed browser names
/// reach here.
fn launch(program: &str, argument: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `start` finds the browser through Windows' App Paths registration.
        std::process::Command::new("cmd")
            .args(["/C", "start", "", program, argument])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(program)
            .arg(argument)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
