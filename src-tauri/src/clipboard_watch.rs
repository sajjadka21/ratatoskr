//! Watches the clipboard for download links while the app runs.
//!
//! Copying a link to a file (or a video page yt-dlp can read) opens the
//! small download window with it (or the main window's Add dialog). The copied text and HTML
//! is only read and compared in memory: it is never stored or logged, and
//! only a hash of the last snapshot is kept to notice a change.

use crate::tray::show_main_window;
use dm_storage::Storage;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
#[cfg(not(windows))]
use tauri_plugin_clipboard_manager::ClipboardExt;

pub const SETTING_CLIPBOARD_WATCH: &str = "clipboard_watch";
pub const CLIPBOARD_LINKS_EVENT: &str = "clipboard-links";
const POLL_INTERVAL: Duration = Duration::from_millis(900);

/// On unless the user turned it off.
pub fn enabled(storage: &Storage) -> bool {
    storage
        .get_setting(SETTING_CLIPBOARD_WATCH)
        .ok()
        .flatten()
        .as_deref()
        != Some("false")
}

fn fingerprint(snapshot: &dm_system::clipboard::Snapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    snapshot.hash(&mut hasher);
    hasher.finish()
}

fn snapshot(app: &AppHandle) -> Result<dm_system::clipboard::Snapshot, String> {
    #[cfg(windows)]
    {
        let _ = app;
        dm_system::clipboard::read().map_err(|_| "Could not read clipboard".to_owned())
    }
    #[cfg(not(windows))]
    {
        Ok(dm_system::clipboard::Snapshot {
            text: app.clipboard().read_text().ok(),
            html: None,
        })
    }
}

pub fn links(app: &AppHandle) -> Result<Vec<String>, String> {
    let snapshot = snapshot(app)?;
    Ok(dm_core::clipboard_links::extract(
        snapshot.text.as_deref(),
        snapshot.html.as_deref(),
    ))
}

/// Links that are not in the download list yet: copying the link of a
/// download already there (with "Copy link", say) offers nothing. A quality
/// chosen for a video is kept in the link's fragment, so that is ignored.
fn new_links(storage: &Storage, links: Vec<String>) -> Vec<String> {
    let known: std::collections::HashSet<String> = storage
        .list_downloads()
        .map(|downloads| {
            downloads
                .into_iter()
                .map(|download| dm_core::ytdlp::without_fragment(&download.source_url).to_owned())
                .collect()
        })
        .unwrap_or_default();
    links
        .into_iter()
        .filter(|link| !known.contains(link))
        .collect()
}

pub async fn run(app: AppHandle, storage: Arc<Storage>) {
    // What is on the clipboard when the app starts was copied before; only
    // later copies count.
    let mut last: Option<u64> = None;
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        if !enabled(&storage) {
            last = None;
            continue;
        }
        let Ok(snapshot) = snapshot(&app) else {
            continue;
        };
        let current = fingerprint(&snapshot);
        let previous = last.replace(current);
        if previous.is_none() || previous == Some(current) {
            continue;
        }
        let extracted =
            dm_core::clipboard_links::extract(snapshot.text.as_deref(), snapshot.html.as_deref());
        let links = new_links(
            &storage,
            dm_core::linkgrabber::downloadable_links(&extracted.join("\n")),
        );
        if links.is_empty() {
            continue;
        }
        // The small window, unless the user is in the main window already
        // (its own Add dialog then opens) or chose the main window.
        if crate::mini::compact(&storage) && !crate::mini::main_in_view(&app) {
            crate::mini::open_add(&app, links);
            continue;
        }
        show_main_window(&app);
        let _ = app.emit(CLIPBOARD_LINKS_EVENT, links);
    }
}
