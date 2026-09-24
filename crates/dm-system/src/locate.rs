//! Where the desktop application keeps its database and where its program
//! is, for the helpers that run beside it (the command-line tool).

use dm_common::{APP_IDENTIFIER, DATABASE_FILE_NAME};
use std::{
    env,
    path::PathBuf,
    process::{Command, Stdio},
};

/// The database the application uses: the platform data directory joined
/// with the bundle identifier, as Tauri lays it out.
/// `DOWNLOAD_MANAGER_DATA_DIR` overrides it for portable setups.
pub fn database_path() -> Option<PathBuf> {
    if let Some(directory) = env::var_os("DOWNLOAD_MANAGER_DATA_DIR") {
        return Some(PathBuf::from(directory).join(DATABASE_FILE_NAME));
    }

    let base = if cfg!(windows) {
        env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    }?;

    Some(base.join(APP_IDENTIFIER).join(DATABASE_FILE_NAME))
}

/// The application program, looked for next to the running helper unless
/// `DOWNLOAD_MANAGER_APP_PATH` names it.
pub fn application_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("DOWNLOAD_MANAGER_APP_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = env::current_exe().ok()?.parent()?.to_owned();
    [
        "Ratatosk.exe",
        "download-manager.exe",
        "Download Manager.exe",
        "tauri-app.exe",
        "download-manager",
        "tauri-app",
    ]
    .into_iter()
    .map(|name| current.join(name))
    .find(|path| path.is_file())
}

/// Starts the application with `arguments`, or hands them to the instance
/// already running. Output is discarded so nothing the application prints
/// lands in the caller's streams.
pub fn launch(application: &PathBuf, arguments: &[String]) -> bool {
    Command::new(application)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_database_follows_the_bundle_identifier() {
        if env::var_os("DOWNLOAD_MANAGER_DATA_DIR").is_some() {
            return;
        }
        if let Some(path) = database_path() {
            assert!(path.ends_with(format!("{APP_IDENTIFIER}/{DATABASE_FILE_NAME}")));
        }
    }
}
