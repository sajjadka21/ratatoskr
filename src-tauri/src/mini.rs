//! The small download window, like the dialog other download managers show
//! when a download arrives from the browser or the clipboard: the link, the
//! quality and the folder, then the download's progress, without bringing
//! up the whole application.

use crate::AppState;
use dm_storage::Storage;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tracing::warn;

/// Setting key: `compact` (the small window, the default) or `main` (the
/// Add dialog of the main window, as before).
pub const SETTING_INTAKE_WINDOW: &str = "intake_window";
/// Sent to an open add window when more links arrive.
pub const MINI_LINKS_EVENT: &str = "mini-links";
const ADD_LABEL: &str = "mini-add";

pub fn compact(storage: &Storage) -> bool {
    storage
        .get_setting(SETTING_INTAKE_WINDOW)
        .ok()
        .flatten()
        .as_deref()
        != Some("main")
}

/// Whether the user is looking at the main window right now; then its own
/// Add dialog is less of an interruption than a second window.
pub fn main_in_view(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false)
            && window.is_focused().unwrap_or(false)
            && !window.is_minimized().unwrap_or(false)
    })
}

fn persian(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .and_then(|state| {
            state
                .storage
                .get_setting(crate::SETTING_UI_LANGUAGE)
                .ok()
                .flatten()
        })
        .is_none_or(|language| language != "en")
}

fn title(app: &AppHandle) -> &'static str {
    if persian(app) {
        "دانلود با راتاتوسک"
    } else {
        "Download with Ratatosk"
    }
}

/// Opens the add window with `links`, or adds them to the one already open.
pub fn open_add(app: &AppHandle, links: Vec<String>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Ok(mut pending) = state.pending_mini_links.lock() {
        for link in links {
            if !pending.contains(&link) {
                pending.push(link);
            }
        }
    }
    if let Some(window) = app.get_webview_window(ADD_LABEL) {
        let _ = window.emit(MINI_LINKS_EVENT, ());
        bring_forward(&window);
        return;
    }
    build(app, ADD_LABEL, "view=mini&mode=add", 480.0, 360.0);
}

/// Opens the progress window of one download. With `confirm`, it first asks
/// whether (and where) to start it.
pub fn open_task(app: &AppHandle, download_id: &str, confirm: bool) {
    let label = format!(
        "mini-{}",
        download_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(24)
            .collect::<String>()
    );
    if let Some(window) = app.get_webview_window(&label) {
        bring_forward(&window);
        return;
    }
    let query = format!(
        "view=mini&mode=task&id={}{}",
        percent_encode(download_id),
        if confirm { "&confirm=1" } else { "" }
    );
    build(
        app,
        &label,
        &query,
        460.0,
        if confirm { 320.0 } else { 250.0 },
    );
}

fn bring_forward(window: &tauri::WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let window = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let _ = window.set_always_on_top(false);
    });
}

fn build(app: &AppHandle, label: &str, query: &str, width: f64, height: f64) {
    let url = WebviewUrl::App(format!("index.html?{query}").into());
    let built = WebviewWindowBuilder::new(app, label, url)
        .title(title(app))
        .inner_size(width, height)
        .min_inner_size(400.0, 220.0)
        .resizable(true)
        .maximizable(false)
        .center()
        .focused(true)
        // Windows keeps a background program's new window behind the one in
        // front; on top for a moment, it comes forward like a dialog should.
        .always_on_top(true)
        .disable_drag_drop_handler()
        .build();
    if let Ok(window) = &built {
        let _ = window.set_focus();
        let window = window.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let _ = window.set_always_on_top(false);
        });
    }
    if let Err(error) = built {
        warn!(error = %error, "could not open the download window");
        // Without it, the main window's Add dialog still works.
        crate::tray::show_main_window(app);
    }
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::percent_encode;

    #[test]
    fn ids_are_safe_in_the_address() {
        assert_eq!(percent_encode("a1-b_2.c"), "a1-b_2.c");
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
    }
}
