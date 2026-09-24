//! What happens around downloads rather than inside them: the action a queue
//! takes when it finishes, and keeping the computer awake while downloads
//! run.

use crate::AppState;
use dm_common::{CompletionAction, QueueState};
use dm_ipc::CompletionActionEvent;
use dm_system::{KeepAwake, PowerAction};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tracing::{info, warn};

pub const COMPLETION_ACTION_EVENT: &str = "completion-action";

/// Time the user has to cancel a power or exit action.
const CANCEL_WINDOW: Duration = Duration::from_secs(60);
const KEEP_AWAKE_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Setting key; keeping the computer awake during downloads is on unless the
/// user turns it off.
pub const SETTING_PREVENT_SLEEP: &str = "prevent_sleep_while_downloading";

pub struct Automation {
    keep_awake: KeepAwake,
    /// Id of the action waiting out its cancel window, if any.
    pending: Mutex<Option<u64>>,
    next_id: AtomicU64,
}

impl Automation {
    pub fn new() -> Self {
        Self {
            keep_awake: KeepAwake::new(),
            pending: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// Cancels the waiting action. Returns whether there was one.
    pub fn cancel_pending(&self) -> Option<u64> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.take())
    }
}

pub fn prevent_sleep_enabled(state: &AppState) -> bool {
    state
        .storage
        .get_setting(SETTING_PREVENT_SLEEP)
        .ok()
        .flatten()
        .is_none_or(|value| value != "false")
}

/// Called when a queue runner returns. Runs the queue's completion action
/// only when the runner actually processed work and nothing is left queued,
/// so stopping a queue by hand never shuts the computer down.
pub fn queue_finished(app: &AppHandle, queue_id: &str, processed_work: bool) {
    if !processed_work {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let drained = state
        .storage
        .list_queued_downloads(queue_id)
        .map(|queued| queued.is_empty())
        .unwrap_or(false);
    if !drained {
        return;
    }

    let Ok(Some(schedule)) = state.storage.get_queue_schedule(queue_id) else {
        return;
    };
    if !schedule.enabled || schedule.completion_action == CompletionAction::None {
        return;
    }

    let queue_name = state
        .storage
        .get_queue(queue_id)
        .ok()
        .flatten()
        .map_or_else(|| queue_id.to_owned(), |queue| queue.name);

    info!(queue_id, action = %schedule.completion_action.as_str(), "queue finished");

    match schedule.completion_action {
        CompletionAction::None => {}
        CompletionAction::Notify => notify(
            app,
            "Queue finished",
            &format!("“{queue_name}” has finished downloading."),
        ),
        action => schedule_action(app, &state, queue_name, action),
    }
}

fn schedule_action(
    app: &AppHandle,
    state: &AppState,
    queue_name: String,
    action: CompletionAction,
) {
    let id = state.automation.next_id.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut pending) = state.automation.pending.lock() {
        *pending = Some(id);
    }

    let due_at = unix_now() + CANCEL_WINDOW.as_secs() as i64;
    let label = action_label(action);
    publish(app, id, &queue_name, action, due_at, "pending", None);
    notify(
        app,
        "Queue finished",
        &format!(
            "“{queue_name}” is done. The computer will {label} in {} seconds unless you cancel.",
            CANCEL_WINDOW.as_secs()
        ),
    );

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CANCEL_WINDOW).await;
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };

        let still_pending = state
            .automation
            .pending
            .lock()
            .map(|mut pending| {
                if *pending == Some(id) {
                    *pending = None;
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if !still_pending {
            return;
        }

        // Other downloads started during the wait: finishing them matters
        // more than the power action, which is skipped rather than delayed.
        if state.downloads.has_running_transfers() {
            let message = "Skipped because other downloads are still running.";
            publish(
                &app,
                id,
                &queue_name,
                action,
                due_at,
                "skipped",
                Some(message),
            );
            notify(&app, "Power action skipped", message);
            return;
        }

        publish(&app, id, &queue_name, action, due_at, "running", None);

        let result = match action {
            CompletionAction::ExitApp => {
                app.exit(0);
                Ok(())
            }
            CompletionAction::Sleep => dm_system::perform(PowerAction::Sleep),
            CompletionAction::Hibernate => dm_system::perform(PowerAction::Hibernate),
            CompletionAction::Shutdown => dm_system::perform(PowerAction::Shutdown),
            CompletionAction::None | CompletionAction::Notify => Ok(()),
        };

        if let Err(error) = result {
            warn!(error = %error, "completion action failed");
            let message = format!("Windows refused to {label}: {error}");
            publish(
                &app,
                id,
                &queue_name,
                action,
                due_at,
                "skipped",
                Some(&message),
            );
        }
    });
}

pub fn publish_cancelled(app: &AppHandle, id: u64) {
    let _ = app.emit(
        COMPLETION_ACTION_EVENT,
        CompletionActionEvent {
            id,
            queue_name: String::new(),
            action: String::new(),
            due_at: unix_now(),
            state: "cancelled".to_owned(),
            message: None,
        },
    );
}

fn publish(
    app: &AppHandle,
    id: u64,
    queue_name: &str,
    action: CompletionAction,
    due_at: i64,
    state: &str,
    message: Option<&str>,
) {
    let _ = app.emit(
        COMPLETION_ACTION_EVENT,
        CompletionActionEvent {
            id,
            queue_name: queue_name.to_owned(),
            action: action.as_str().to_owned(),
            due_at,
            state: state.to_owned(),
            message: message.map(str::to_owned),
        },
    );
}

fn action_label(action: CompletionAction) -> &'static str {
    match action {
        CompletionAction::ExitApp => "close Ratatosk",
        CompletionAction::Sleep => "sleep",
        CompletionAction::Hibernate => "hibernate",
        CompletionAction::Shutdown => "shut down",
        CompletionAction::None | CompletionAction::Notify => "continue",
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        warn!(error = %error, "could not show a notification");
    }
}

/// Holds the keep-awake request while transfers run, when the user wants it
/// or a running queue's schedule asks for it.
pub async fn run_keep_awake(app: AppHandle) {
    loop {
        tokio::time::sleep(KEEP_AWAKE_POLL_INTERVAL).await;
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };

        let running = state.downloads.has_running_transfers();
        let wanted =
            running && (prevent_sleep_enabled(&state) || running_queue_prevents_sleep(&state));
        state.automation.keep_awake.set(wanted);

        if app.webview_windows().is_empty() {
            state.automation.keep_awake.set(false);
            return;
        }
    }
}

fn running_queue_prevents_sleep(state: &AppState) -> bool {
    let Ok(schedules) = state.storage.list_queue_schedules() else {
        return false;
    };
    let Ok(queues) = state.queues.list_queues() else {
        return false;
    };
    schedules.iter().any(|schedule| {
        schedule.enabled
            && schedule.prevent_sleep
            && queues
                .iter()
                .any(|queue| queue.id == schedule.queue_id && queue.state == QueueState::Running)
    })
}

/// Seconds east of UTC for the local clock right now, for wall-clock
/// schedule windows. Read on every check so a daylight-saving change or a
/// time-zone change is picked up without a restart.
pub fn local_utc_offset_seconds() -> i32 {
    chrono::Local::now().offset().local_minus_utc()
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
