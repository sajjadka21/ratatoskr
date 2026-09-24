//! The notification-area icon: shows overall speed in its tooltip, brings the
//! window back, pauses everything, and quits. Closing the window hides it to
//! the tray when the user wants that, so downloads keep running.

use crate::AppState;
use serde::Deserialize;
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};
use tracing::warn;

const TRAY_ID: &str = "main";
const MENU_SHOW: &str = "tray-show";
const MENU_PAUSE_ALL: &str = "tray-pause-all";
const MENU_QUIT: &str = "tray-quit";

/// Setting key; closing the window hides it to the tray unless turned off.
pub const SETTING_CLOSE_TO_TRAY: &str = "ui_close_to_tray";

/// Handles to the tray's menu items so their text can follow the UI language.
pub struct TrayMenu {
    show: MenuItem<tauri::Wry>,
    pause_all: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub show: String,
    pub pause_all: String,
    pub quit: String,
}

pub fn create(app: &AppHandle) -> tauri::Result<TrayMenu> {
    let show = MenuItem::with_id(app, MENU_SHOW, "Show Download Manager", true, None::<&str>)?;
    let pause_all = MenuItem::with_id(app, MENU_PAUSE_ALL, "Pause all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &pause_all, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Download Manager")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SHOW => show_main_window(app),
            MENU_PAUSE_ALL => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.downloads.pause_all();
                }
            }
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray: &TrayIcon, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    Ok(TrayMenu {
        show,
        pause_all,
        quit,
    })
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn close_to_tray_enabled(state: &AppState) -> bool {
    state
        .storage
        .get_setting(SETTING_CLOSE_TO_TRAY)
        .ok()
        .flatten()
        .is_none_or(|value| value != "false")
}

/// Updates the tooltip (speed and active count, already formatted in the
/// user's language) and, when given, the menu labels.
pub fn update(
    app: &AppHandle,
    menu: &Mutex<Option<TrayMenu>>,
    tooltip: &str,
    labels: Option<&TrayLabels>,
) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(error) = tray.set_tooltip(Some(tooltip)) {
            warn!(error = %error, "could not update the tray tooltip");
        }
    }

    let Some(labels) = labels else {
        return;
    };
    if let Ok(menu) = menu.lock() {
        if let Some(menu) = menu.as_ref() {
            let _ = menu.show.set_text(&labels.show);
            let _ = menu.pause_all.set_text(&labels.pause_all);
            let _ = menu.quit.set_text(&labels.quit);
        }
    }
}
