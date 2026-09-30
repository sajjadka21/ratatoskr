//! Portable mode: when a file named `portable.txt` sits next to the program,
//! everything the app keeps (database, tools, browser setup, the web view's
//! own data) lives in a `data` folder beside it instead of in the user's
//! profile, so the whole folder can be carried on a USB drive.

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

const MARKER: &str = "portable.txt";
const DATA_FOLDER: &str = "data";

/// The data folder for a program at `exe`, when it is run portably.
pub fn data_folder_beside(exe: &Path) -> Option<PathBuf> {
    let folder = exe.parent()?;
    folder
        .join(MARKER)
        .is_file()
        .then(|| folder.join(DATA_FOLDER))
}

/// Where this copy keeps its data when it is portable.
pub fn data_folder() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| data_folder_beside(&exe))
}

/// The folder for the app's data: the portable folder, or the usual one.
pub fn app_data<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    data_folder().or_else(|| app.path().app_data_dir().ok())
}

/// Points the web view at a data folder of its own beside the program. Must
/// run before the first window is made.
pub fn prepare_webview() {
    if let Some(folder) = data_folder() {
        let webview = folder.join("webview");
        let _ = std::fs::create_dir_all(&webview);
        // SAFETY: called once at start-up, before any other thread exists.
        unsafe { std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", webview) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_beside_the_program_makes_it_portable() {
        let folder = tempfile::tempdir().unwrap();
        let exe = folder.path().join("ratatosk.exe");
        assert_eq!(data_folder_beside(&exe), None);
        std::fs::write(folder.path().join(MARKER), "").unwrap();
        assert_eq!(data_folder_beside(&exe), Some(folder.path().join("data")));
    }
}
