//! Watches the clipboard for download links while the app runs.
//!
//! Copying a link to a file (or a video page yt-dlp can read) brings the
//! window forward with the Add download dialog filled in. The copied text
//! is only read and compared in memory: it is never stored or logged, and
//! only a hash of the last text is kept to notice a change.

use crate::tray::show_main_window;
use dm_storage::Storage;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
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

fn fingerprint(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Links that are not in the download list yet: copying the link of a
/// download already there (with "Copy link", say) offers nothing.
fn new_links(storage: &Storage, links: Vec<String>) -> Vec<String> {
    let known: std::collections::HashSet<String> = storage
        .list_downloads()
        .map(|downloads| {
            downloads
                .into_iter()
                .map(|download| download.source_url)
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
        let Ok(text) = app.clipboard().read_text() else {
            continue;
        };
        let current = fingerprint(&text);
        let previous = last.replace(current);
        if previous.is_none() || previous == Some(current) {
            continue;
        }
        let links = new_links(&storage, dm_core::linkgrabber::downloadable_links(&text));
        if links.is_empty() {
            continue;
        }
        show_main_window(&app);
        let _ = app.emit(CLIPBOARD_LINKS_EVENT, links);
    }
}
