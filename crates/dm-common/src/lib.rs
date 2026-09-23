use serde::{Deserialize, Serialize};

/// The desktop application's bundle identifier. The application's data
/// directory is named after it, and the browser native host uses it to find
/// the same database. Must match `identifier` in `src-tauri/tauri.conf.json`.
pub const APP_IDENTIFIER: &str = "com.downloadmanager.desktop";

/// File name of the task database inside the application data directory.
pub const DATABASE_FILE_NAME: &str = "downloads.db";
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Created,
    Probing,
    Queued,
    Downloading,
    Paused,
    Retrying,
    Finalizing,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    /// Every canonical status, so callers can derive status groups from the
    /// canonical rules instead of restating them.
    pub const ALL: [Self; 10] = [
        Self::Created,
        Self::Probing,
        Self::Queued,
        Self::Downloading,
        Self::Paused,
        Self::Retrying,
        Self::Finalizing,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    /// Statuses whose row can be deleted from history. A task that an executor
    /// may still be writing to is never removable; a paused one is, because
    /// its executor has released the file.
    pub const fn is_removable(self) -> bool {
        matches!(
            self,
            Self::Created
                | Self::Queued
                | Self::Paused
                | Self::Completed
                | Self::Failed
                | Self::Cancelled
        )
    }

    /// Whether an executor owns this task right now. Used to decide whether a
    /// pause or cancel has to reach a running transfer or only has to be
    /// persisted.
    pub const fn is_executing(self) -> bool {
        matches!(self, Self::Probing | Self::Downloading | Self::Finalizing)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Probing => "probing",
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Retrying => "retrying",
            Self::Finalizing => "finalizing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// States that only exist while an executor in the owning process is
    /// driving the transfer. A process restart leaves such a row behind with
    /// nobody advancing it, so startup recovery has to move it back to a state
    /// the user or a queue runner can act on again.
    pub const fn is_orphaned_by_restart(self) -> bool {
        matches!(self, Self::Probing | Self::Downloading | Self::Finalizing)
    }

    /// The state an orphaned task returns to after a restart.
    ///
    /// A task that belongs to a queue goes back to its queue, where its runner
    /// picks it up and continues from whatever it had already transferred. A
    /// task with partial bytes on disk becomes paused, so the user resumes it
    /// rather than losing the transfer. Anything else goes back to the user as
    /// a created task.
    pub const fn restart_recovery_status(has_queue: bool, has_partial_transfer: bool) -> Self {
        if has_queue {
            Self::Queued
        } else if has_partial_transfer {
            Self::Paused
        } else {
            Self::Created
        }
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Probing | Self::Queued | Self::Cancelled)
                | (
                    Self::Probing,
                    Self::Downloading
                        | Self::Paused
                        | Self::Retrying
                        | Self::Failed
                        | Self::Cancelled
                )
                | (Self::Queued, Self::Created | Self::Probing | Self::Cancelled)
                | (
                    Self::Downloading,
                    Self::Finalizing | Self::Paused | Self::Retrying | Self::Failed
                        | Self::Cancelled
                )
                | (Self::Paused, Self::Created | Self::Probing | Self::Queued | Self::Cancelled)
                | (
                    Self::Retrying,
                    Self::Probing | Self::Queued | Self::Failed | Self::Cancelled
                )
                | (Self::Finalizing, Self::Completed | Self::Failed)
                // Retry and restart re-enter the lifecycle from a terminal
                // state; the task keeps its identity either way.
                | (Self::Failed, Self::Created | Self::Probing | Self::Queued)
                | (Self::Cancelled, Self::Created | Self::Probing | Self::Queued)
        )
    }

    /// Every status that may legally become `next`. Storage builds its update
    /// guards from this, so the canonical machine is the only place the rules
    /// exist.
    pub fn sources_of(next: Self) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|from| from.can_transition_to(next))
            .collect()
    }
}

impl fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDownloadStatusError {
    value: String,
}

impl fmt::Display for ParseDownloadStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown download status: {}", self.value)
    }
}

impl std::error::Error for ParseDownloadStatusError {}

impl FromStr for DownloadStatus {
    type Err = ParseDownloadStatusError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(Self::Created),
            "probing" => Ok(Self::Probing),
            "queued" => Ok(Self::Queued),
            "downloading" => Ok(Self::Downloading),
            "paused" => Ok(Self::Paused),
            "retrying" => Ok(Self::Retrying),
            "finalizing" => Ok(Self::Finalizing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(ParseDownloadStatusError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPriority {
    Low,
    Normal,
    High,
    VeryHigh,
}

impl DownloadPriority {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::VeryHigh => "very_high",
        }
    }
}

impl fmt::Display for DownloadPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DownloadPriority {
    type Err = ParseDownloadPriorityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "very_high" => Ok(Self::VeryHigh),
            other => Err(ParseDownloadPriorityError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDownloadPriorityError {
    value: String,
}

impl fmt::Display for ParseDownloadPriorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown download priority: {}", self.value)
    }
}

impl std::error::Error for ParseDownloadPriorityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    Running,
    Stopped,
}

impl QueueState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

impl fmt::Display for QueueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for QueueState {
    type Err = ParseQueueStateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            other => Err(ParseQueueStateError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseQueueStateError {
    value: String,
}

impl fmt::Display for ParseQueueStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown queue state: {}", self.value)
    }
}

impl std::error::Error for ParseQueueStateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    Pending,
    Downloading,
    Completed,
}

impl SegmentStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for SegmentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SegmentStatus {
    type Err = ParseSegmentStatusError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "downloading" => Ok(Self::Downloading),
            "completed" => Ok(Self::Completed),
            other => Err(ParseSegmentStatusError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSegmentStatusError {
    value: String,
}

impl fmt::Display for ParseSegmentStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown segment status: {}", self.value)
    }
}

impl std::error::Error for ParseSegmentStatusError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSegment {
    pub download_id: String,
    pub segment_index: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_bytes: u64,
    pub temp_path: String,
    pub status: SegmentStatus,
}

impl DownloadSegment {
    pub fn expected_bytes(&self) -> Option<u64> {
        self.end_byte
            .checked_sub(self.start_byte)
            .and_then(|length| length.checked_add(1))
    }

    pub fn is_complete(&self) -> bool {
        self.status == SegmentStatus::Completed
            && self.expected_bytes() == Some(self.downloaded_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub state: QueueState,
    pub sort_order: i64,
    pub max_concurrent: u32,
    pub max_concurrent_per_host: Option<u32>,
    pub default_priority: DownloadPriority,
    pub created_at: i64,
    pub updated_at: i64,
}

impl QueueRecord {
    /// A queue only hands work to its runner while it is both enabled and
    /// running. `enabled` is the persistent configuration switch, `state` is
    /// what start/stop toggles; ignoring either one would let a queue schedule
    /// work the user has turned off.
    pub const fn is_schedulable(&self) -> bool {
        self.enabled && matches!(self.state, QueueState::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub source_url: String,
    pub resolved_url: Option<String>,
    pub filename: Option<String>,
    pub destination_path: Option<String>,
    pub temp_path: Option<String>,
    pub mime_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub range_supported: Option<bool>,
    pub status: DownloadStatus,
    pub queue_id: Option<String>,
    pub priority: DownloadPriority,
    pub queue_position: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    /// How many times this task has been attempted. Reset when the user
    /// starts it again by hand, so an automatic retry budget cannot be
    /// exhausted by history.
    pub attempts: u32,
    /// When an automatically retrying task becomes eligible again.
    pub retry_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// User-configurable destination/category metadata. Lists are kept canonical
/// and serializable so storage and IPC can preserve the exact rule inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryRecord {
    pub id: String,
    pub name: String,
    pub extensions: Vec<String>,
    pub mime_patterns: Vec<String>,
    pub default_directory: Option<String>,
    pub host_patterns: Vec<String>,
    pub priority: DownloadPriority,
    pub queue_id: Option<String>,
}

/// A deterministic rule evaluated before a task starts. Empty match fields are
/// wildcards; actions are optional and applied in precedence order by core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub sort_order: i64,
    pub domain: Option<String>,
    pub url_pattern: Option<String>,
    pub extension: Option<String>,
    pub mime_pattern: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub category_id: Option<String>,
    pub destination_directory: Option<String>,
    pub queue_id: Option<String>,
    pub priority: Option<DownloadPriority>,
    pub max_connections: Option<u32>,
    pub max_host_concurrency: Option<u32>,
    pub speed_cap: Option<u64>,
    pub browser_takeover_allowed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleKind {
    Once,
    Daily,
    Weekdays,
    Repeating,
}

impl ScheduleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Daily => "daily",
            Self::Weekdays => "weekdays",
            Self::Repeating => "repeating",
        }
    }
}

impl FromStr for ScheduleKind {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "once" => Ok(Self::Once),
            "daily" => Ok(Self::Daily),
            "weekdays" => Ok(Self::Weekdays),
            "repeating" => Ok(Self::Repeating),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionAction {
    None,
    Notify,
    ExitApp,
    Sleep,
    Hibernate,
    Shutdown,
}

impl CompletionAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Notify => "notify",
            Self::ExitApp => "exit_app",
            Self::Sleep => "sleep",
            Self::Hibernate => "hibernate",
            Self::Shutdown => "shutdown",
        }
    }
}

impl FromStr for CompletionAction {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            "notify" => Ok(Self::Notify),
            "exit_app" => Ok(Self::ExitApp),
            "sleep" => Ok(Self::Sleep),
            "hibernate" => Ok(Self::Hibernate),
            "shutdown" => Ok(Self::Shutdown),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueSchedule {
    pub queue_id: String,
    pub enabled: bool,
    pub kind: ScheduleKind,
    pub start_at: i64,
    pub stop_at: Option<i64>,
    pub weekdays_mask: u8,
    pub interval_seconds: Option<u64>,
    pub completion_action: CompletionAction,
    pub prevent_sleep: bool,
    pub updated_at: i64,
    /// Local time of day the daily/weekday window opens, in minutes after
    /// midnight (0..1440). `None` on schedules saved before windows existed.
    pub window_start_minute: Option<u16>,
    /// Local time of day the window closes. Earlier than the start means the
    /// window runs past midnight (23:00 to 07:00).
    pub window_end_minute: Option<u16>,
}

const MINUTES_PER_DAY: i64 = 24 * 60;

impl QueueSchedule {
    /// Whether the queue should be running at `timestamp`, given the local
    /// clock's offset from UTC. Daily and weekday windows are wall-clock
    /// times ("02:00 to 07:00"), so they must be evaluated in local time.
    pub fn is_active_at(&self, timestamp: i64, utc_offset_seconds: i32) -> bool {
        if !self.enabled || timestamp < self.start_at {
            return false;
        }

        match self.kind {
            ScheduleKind::Once => self.stop_at.is_none_or(|stop| timestamp < stop),
            ScheduleKind::Daily | ScheduleKind::Weekdays => {
                let local = timestamp + i64::from(utc_offset_seconds);
                let local_day = local.div_euclid(86_400);
                let minute = local.rem_euclid(86_400) / 60;
                let (start, end) = self.window(utc_offset_seconds);

                // The day a window belongs to is the day it opened, so the
                // after-midnight part of 23:00-07:00 counts for the evening
                // before when checking weekdays.
                let opened_on = if start == end {
                    Some(local_day)
                } else if start < end {
                    (start..end).contains(&minute).then_some(local_day)
                } else if minute >= start {
                    Some(local_day)
                } else if minute < end {
                    Some(local_day - 1)
                } else {
                    None
                };

                opened_on.is_some_and(|day| self.kind == ScheduleKind::Daily || self.runs_on(day))
            }
            ScheduleKind::Repeating => {
                self.stop_at.is_none_or(|stop| timestamp < stop)
                    && self.interval_seconds.is_some_and(|interval| {
                        interval > 0
                            && i64::try_from(interval).ok().is_some_and(|interval| {
                                (timestamp - self.start_at).rem_euclid(interval) < 60
                            })
                    })
            }
        }
    }

    /// Weekday bit for a day counted from the Unix epoch: bit 0 is Sunday.
    fn runs_on(&self, local_day: i64) -> bool {
        let weekday = (local_day + 4).rem_euclid(7) as u8;
        self.weekdays_mask & (1 << weekday) != 0
    }

    /// The window in local minutes. Older schedules stored it as the time of
    /// day of `start_at` and `stop_at`; with neither, it is the whole day.
    fn window(&self, utc_offset_seconds: i32) -> (i64, i64) {
        let minute_of =
            |timestamp: i64| (timestamp + i64::from(utc_offset_seconds)).rem_euclid(86_400) / 60;
        match (self.window_start_minute, self.window_end_minute) {
            (Some(start), Some(end)) => (
                i64::from(start).rem_euclid(MINUTES_PER_DAY),
                i64::from(end).rem_euclid(MINUTES_PER_DAY),
            ),
            _ => match self.stop_at {
                Some(stop) => (minute_of(self.start_at), minute_of(stop)),
                None => (0, 0),
            },
        }
    }
}

/// Everything probing learned about a source, persisted before any bytes are
/// written. A restart reads this back to decide whether the partial file on
/// disk still belongs to the same remote content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPlan {
    pub resolved_url: String,
    pub filename: String,
    pub destination_path: String,
    pub temp_path: String,
    pub mime_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub range_supported: bool,
}

/// Host-only adaptive observations. The key is a normalized hostname; no URL
/// path, query, credentials, or request headers are part of this model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostProfile {
    pub host: String,
    pub preferred_max_connections: u32,
    pub rate_limited_count: u32,
    pub busy_count: u32,
    pub last_status: Option<u16>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCompletion {
    pub resolved_url: String,
    pub filename: String,
    pub destination_path: String,
    pub mime_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
}

/// Non-secret request details a browser supplied with a download. Servers
/// that check the page a download came from, or reject non-browser clients,
/// need these to serve the file. Cookies and credentials are deliberately not
/// part of this model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestContext {
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionAction, DownloadPriority, DownloadSegment, DownloadStatus, QueueSchedule,
        QueueState, ScheduleKind, SegmentStatus,
    };
    use std::str::FromStr;

    #[test]
    fn status_string_roundtrip() {
        let statuses = [
            DownloadStatus::Created,
            DownloadStatus::Probing,
            DownloadStatus::Queued,
            DownloadStatus::Downloading,
            DownloadStatus::Paused,
            DownloadStatus::Retrying,
            DownloadStatus::Finalizing,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ];

        for status in statuses {
            let encoded = status.to_string();
            let decoded = DownloadStatus::from_str(&encoded).unwrap();
            assert_eq!(decoded, status);
        }
    }

    #[test]
    fn rejects_unknown_status() {
        assert!(DownloadStatus::from_str("unknown").is_err());
    }

    #[test]
    fn phase_one_lifecycle_transitions_are_legal() {
        assert!(DownloadStatus::Created.can_transition_to(DownloadStatus::Probing));
        assert!(DownloadStatus::Probing.can_transition_to(DownloadStatus::Downloading));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Finalizing));
        assert!(DownloadStatus::Finalizing.can_transition_to(DownloadStatus::Completed));
    }

    #[test]
    fn active_phase_one_states_can_fail() {
        assert!(DownloadStatus::Probing.can_transition_to(DownloadStatus::Failed));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Failed));
        assert!(DownloadStatus::Finalizing.can_transition_to(DownloadStatus::Failed));
    }

    #[test]
    fn phase_one_lifecycle_rejects_invalid_jumps() {
        assert!(!DownloadStatus::Created.can_transition_to(DownloadStatus::Downloading));
        assert!(!DownloadStatus::Created.can_transition_to(DownloadStatus::Completed));
        assert!(!DownloadStatus::Completed.can_transition_to(DownloadStatus::Downloading));
        assert!(!DownloadStatus::Failed.can_transition_to(DownloadStatus::Completed));
    }

    #[test]
    fn only_executor_owned_states_are_orphaned_by_a_restart() {
        for status in [
            DownloadStatus::Probing,
            DownloadStatus::Downloading,
            DownloadStatus::Finalizing,
        ] {
            assert!(status.is_orphaned_by_restart(), "{status} must recover");
        }

        for status in [
            DownloadStatus::Created,
            DownloadStatus::Queued,
            DownloadStatus::Paused,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ] {
            assert!(
                !status.is_orphaned_by_restart(),
                "{status} must survive a restart untouched"
            );
        }
    }

    #[test]
    fn removable_statuses_never_overlap_executor_owned_statuses() {
        for status in DownloadStatus::ALL {
            assert!(
                !(status.is_removable() && status.is_executing()),
                "{status} cannot be both removable and owned by an executor"
            );
        }

        assert!(DownloadStatus::Created.is_removable());
        assert!(DownloadStatus::Queued.is_removable());
        assert!(
            DownloadStatus::Paused.is_removable(),
            "a paused task has released its file"
        );
        assert!(!DownloadStatus::Downloading.is_removable());
    }

    #[test]
    fn restart_recovery_keeps_partial_transfers_resumable() {
        assert_eq!(
            DownloadStatus::restart_recovery_status(true, false),
            DownloadStatus::Queued
        );
        assert_eq!(
            DownloadStatus::restart_recovery_status(true, true),
            DownloadStatus::Queued,
            "a queued task resumes through its runner"
        );
        assert_eq!(
            DownloadStatus::restart_recovery_status(false, true),
            DownloadStatus::Paused,
            "partial bytes must survive as something the user can resume"
        );
        assert_eq!(
            DownloadStatus::restart_recovery_status(false, false),
            DownloadStatus::Created
        );
    }

    #[test]
    fn pause_and_resume_are_legal_in_both_directions() {
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Paused));
        assert!(DownloadStatus::Paused.can_transition_to(DownloadStatus::Probing));
        assert!(DownloadStatus::Paused.can_transition_to(DownloadStatus::Queued));
    }

    #[test]
    fn terminal_states_can_be_retried_but_never_completed_directly() {
        assert!(DownloadStatus::Failed.can_transition_to(DownloadStatus::Probing));
        assert!(DownloadStatus::Cancelled.can_transition_to(DownloadStatus::Probing));
        assert!(!DownloadStatus::Completed.can_transition_to(DownloadStatus::Probing));
        assert!(!DownloadStatus::Failed.can_transition_to(DownloadStatus::Completed));
    }

    #[test]
    fn restart_from_zero_is_legal_for_reusable_states() {
        for status in [
            DownloadStatus::Paused,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ] {
            assert!(status.can_transition_to(DownloadStatus::Created));
        }

        assert!(!DownloadStatus::Downloading.can_transition_to(DownloadStatus::Created));
    }

    #[test]
    fn schedule_windows_are_deterministic_and_disabled_by_default() {
        let schedule = QueueSchedule {
            queue_id: "default".to_owned(),
            enabled: true,
            kind: ScheduleKind::Daily,
            start_at: 86_400,
            stop_at: Some(90_000),
            weekdays_mask: 0,
            interval_seconds: None,
            completion_action: CompletionAction::None,
            prevent_sleep: false,
            updated_at: 0,
            window_start_minute: None,
            window_end_minute: None,
        };
        assert!(!schedule.is_active_at(86_399, 0));
        assert!(schedule.is_active_at(86_500, 0));
        assert!(!schedule.is_active_at(90_000, 0));
        // A legacy daily window repeats on the following days too.
        assert!(schedule.is_active_at(86_400 * 5 + 100, 0));
        assert!(
            !QueueSchedule {
                enabled: false,
                ..schedule
            }
            .is_active_at(86_500, 0)
        );
    }

    fn night_window(kind: ScheduleKind, weekdays_mask: u8) -> QueueSchedule {
        QueueSchedule {
            queue_id: "night".to_owned(),
            enabled: true,
            kind,
            start_at: 0,
            stop_at: None,
            weekdays_mask,
            interval_seconds: None,
            completion_action: CompletionAction::None,
            prevent_sleep: false,
            updated_at: 0,
            window_start_minute: Some(2 * 60),
            window_end_minute: Some(7 * 60),
        }
    }

    /// Tehran is UTC+03:30.
    const TEHRAN: i32 = 3 * 3600 + 1800;
    /// 2026-09-23 00:00 in Tehran (a Wednesday), as a Unix timestamp.
    const WEDNESDAY_MIDNIGHT_TEHRAN: i64 = 1_790_121_600 - TEHRAN as i64;

    #[test]
    fn daily_windows_use_local_wall_clock_time() {
        let schedule = night_window(ScheduleKind::Daily, 0);
        let at = |hour: i64, minute: i64| WEDNESDAY_MIDNIGHT_TEHRAN + hour * 3600 + minute * 60;

        assert!(!schedule.is_active_at(at(1, 59), TEHRAN));
        assert!(schedule.is_active_at(at(2, 0), TEHRAN));
        assert!(schedule.is_active_at(at(6, 59), TEHRAN));
        assert!(!schedule.is_active_at(at(7, 0), TEHRAN));
        // Evaluated in UTC the same instants would be wrong by 3.5 hours.
        assert!(!schedule.is_active_at(at(2, 0), 0));
        // Every day, not only the first one.
        assert!(schedule.is_active_at(at(24 * 10 + 3, 0), TEHRAN));
    }

    #[test]
    fn overnight_weekday_windows_belong_to_the_day_they_open() {
        let mut schedule = night_window(ScheduleKind::Weekdays, 1 << 3); // Wednesday
        schedule.window_start_minute = Some(23 * 60);
        schedule.window_end_minute = Some(7 * 60);
        let at = |hour: i64| WEDNESDAY_MIDNIGHT_TEHRAN + hour * 3600;

        assert!(!schedule.is_active_at(at(3), TEHRAN)); // Wed 03:00 is Tuesday's night
        assert!(schedule.is_active_at(at(23), TEHRAN)); // Wed 23:00
        assert!(schedule.is_active_at(at(24 + 3), TEHRAN)); // Thu 03:00, Wednesday's night
        assert!(!schedule.is_active_at(at(24 + 23), TEHRAN)); // Thu 23:00
    }

    #[test]
    fn repeating_schedule_uses_a_poll_window_instead_of_an_exact_second() {
        let schedule = QueueSchedule {
            queue_id: "default".to_owned(),
            enabled: true,
            kind: ScheduleKind::Repeating,
            start_at: 100,
            stop_at: None,
            weekdays_mask: 0,
            interval_seconds: Some(300),
            completion_action: CompletionAction::None,
            prevent_sleep: false,
            updated_at: 0,
            window_start_minute: None,
            window_end_minute: None,
        };
        assert!(schedule.is_active_at(101, 0));
        assert!(schedule.is_active_at(159, 0));
        assert!(!schedule.is_active_at(160, 0));
        assert!(schedule.is_active_at(400, 0));
    }

    #[test]
    fn a_completed_download_is_final() {
        for next in DownloadStatus::ALL {
            assert!(
                !DownloadStatus::Completed.can_transition_to(next),
                "completed must never move to {next}"
            );
        }
    }

    #[test]
    fn transition_sources_match_the_canonical_rules() {
        for next in DownloadStatus::ALL {
            let sources = DownloadStatus::sources_of(next);

            for from in DownloadStatus::ALL {
                assert_eq!(
                    sources.contains(&from),
                    from.can_transition_to(next),
                    "{from} -> {next}"
                );
            }
        }
    }

    #[test]
    fn queue_state_string_roundtrip() {
        for state in [QueueState::Running, QueueState::Stopped] {
            assert_eq!(QueueState::from_str(state.as_str()).unwrap(), state);
        }
    }

    #[test]
    fn download_priority_string_roundtrip() {
        let priorities = [
            DownloadPriority::Low,
            DownloadPriority::Normal,
            DownloadPriority::High,
            DownloadPriority::VeryHigh,
        ];

        for priority in priorities {
            assert_eq!(
                DownloadPriority::from_str(priority.as_str()).unwrap(),
                priority
            );
        }
    }

    #[test]
    fn segment_status_string_roundtrip() {
        for status in [
            SegmentStatus::Pending,
            SegmentStatus::Downloading,
            SegmentStatus::Completed,
        ] {
            assert_eq!(SegmentStatus::from_str(status.as_str()).unwrap(), status);
        }
    }

    #[test]
    fn segment_reports_inclusive_range_length_and_completion() {
        let segment = DownloadSegment {
            download_id: "task".to_owned(),
            segment_index: 0,
            start_byte: 10,
            end_byte: 19,
            downloaded_bytes: 10,
            temp_path: "task.0.part".to_owned(),
            status: SegmentStatus::Completed,
        };

        assert_eq!(segment.expected_bytes(), Some(10));
        assert!(segment.is_complete());
    }
}
