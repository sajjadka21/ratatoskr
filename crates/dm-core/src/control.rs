use std::sync::atomic::{AtomicU8, Ordering};
use tokio::sync::Notify;

/// Why a running transfer is being stopped. The distinction matters at the
/// storage layer: a pause keeps the partial file so the transfer can continue,
/// a cancel discards it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Pause,
    Cancel,
}

const RUNNING: u8 = 0;
const PAUSING: u8 = 1;
const CANCELLING: u8 = 2;

/// The handle a running transfer watches so pause and cancel take effect
/// between chunks instead of waiting for the transfer to end on its own.
///
/// The state is set before the notification is raised, and every waiter checks
/// the state before waiting, so a request cannot be missed by arriving between
/// the two.
#[derive(Debug)]
pub struct TaskControl {
    state: AtomicU8,
    changed: Notify,
}

impl TaskControl {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(RUNNING),
            changed: Notify::new(),
        }
    }

    /// Asks the transfer to stop. The first request wins: a cancel that
    /// arrives after a pause still upgrades to cancel, but a pause never
    /// downgrades a cancel already in flight.
    pub fn request(&self, reason: StopReason) {
        let requested = match reason {
            StopReason::Pause => PAUSING,
            StopReason::Cancel => CANCELLING,
        };

        let current = self.state.load(Ordering::Acquire);

        if current == CANCELLING {
            return;
        }

        self.state.store(requested, Ordering::Release);
        self.changed.notify_waiters();
    }

    pub fn stop_reason(&self) -> Option<StopReason> {
        match self.state.load(Ordering::Acquire) {
            PAUSING => Some(StopReason::Pause),
            CANCELLING => Some(StopReason::Cancel),
            _ => None,
        }
    }

    /// Resolves as soon as a stop has been requested. Safe to use as a branch
    /// of a `select!` in the transfer loop.
    pub async fn stopped(&self) -> StopReason {
        loop {
            // Registering before the check closes the gap where a request
            // could land between checking and waiting.
            let notified = self.changed.notified();

            if let Some(reason) = self.stop_reason() {
                return reason;
            }

            notified.await;
        }
    }
}

impl Default for TaskControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::{Duration, timeout};

    #[test]
    fn a_fresh_control_is_running() {
        assert_eq!(TaskControl::new().stop_reason(), None);
    }

    #[test]
    fn cancel_is_never_downgraded_to_pause() {
        let control = TaskControl::new();

        control.request(StopReason::Cancel);
        control.request(StopReason::Pause);

        assert_eq!(control.stop_reason(), Some(StopReason::Cancel));
    }

    #[test]
    fn pause_can_be_upgraded_to_cancel() {
        let control = TaskControl::new();

        control.request(StopReason::Pause);
        control.request(StopReason::Cancel);

        assert_eq!(control.stop_reason(), Some(StopReason::Cancel));
    }

    #[tokio::test]
    async fn a_request_made_before_waiting_is_not_missed() {
        let control = TaskControl::new();
        control.request(StopReason::Pause);

        let reason = timeout(Duration::from_millis(100), control.stopped())
            .await
            .expect("an already requested stop must resolve immediately");

        assert_eq!(reason, StopReason::Pause);
    }

    #[tokio::test]
    async fn waiting_resolves_when_a_stop_arrives_later() {
        let control = Arc::new(TaskControl::new());
        let waiter = Arc::clone(&control);

        let waiting = tokio::spawn(async move { waiter.stopped().await });

        tokio::task::yield_now().await;
        control.request(StopReason::Cancel);

        let reason = timeout(Duration::from_secs(1), waiting)
            .await
            .expect("a requested stop must wake the waiter")
            .unwrap();

        assert_eq!(reason, StopReason::Cancel);
    }
}
