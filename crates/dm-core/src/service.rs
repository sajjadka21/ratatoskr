use crate::session::BrowserSession;
use crate::{
    DownloadError, Downloader, RangeProgress, RangeTransferRequest, TransferProgress,
    TransferRequest,
    adaptive::{AdaptiveController, ThroughputSample},
    control::{StopReason, TaskControl},
    network::NetworkSettings,
    ratelimit::RateLimiter,
    resume::{ResumePlan, StoredTransfer, plan_resume},
    retry::{FailureClass, RetryPolicy, classify_failure},
    rules::{RuleDecision, evaluate_rules},
    segment_planner::{SegmentPlanError, covers_exactly, plan_segments},
    slot::RangeSlot,
    throughput::ThroughputMeter,
    traffic::{classify_host, local_day},
    validate_source_url,
};
use dm_common::{
    DownloadCompletion, DownloadRecord, DownloadSegment, RequestContext, SegmentStatus,
    TransferPlan,
};
use dm_storage::{Storage, StorageError, TrafficScope};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicI32, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinSet;

const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 3;
const DEFAULT_SEGMENT_CONNECTIONS: usize = 8;
const MAX_SEGMENT_CONNECTIONS: usize = 64;
const DEFAULT_SEGMENTED_THRESHOLD: u64 = 1024 * 1024;
const PROGRESS_PERSIST_INTERVAL: Duration = Duration::from_millis(500);
const PROGRESS_PERSIST_BYTES: u64 = 1024 * 1024;
/// A range is never planned shorter than this, so a small file does not
/// open eight connections for a few kilobytes each.
const DEFAULT_MIN_SEGMENT_BYTES: u64 = 1024 * 1024;
/// A running range is split only when both halves would be this long.
const DEFAULT_MIN_SPLIT_BYTES: u64 = 512 * 1024;
/// Written bytes between two flushes of the shared partial file.
const CHECKPOINT_BYTES: u64 = 8 * 1024 * 1024;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);
/// How long the connection count is held before its throughput is judged.
const DEFAULT_EVALUATION_WINDOW: Duration = Duration::from_secs(1);
/// Progress is reported to the interface at most this often.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(150);

/// Recorded on tasks that a process restart orphaned, so the row explains
/// itself instead of silently reappearing as unstarted work.
const INTERRUPTED_ERROR_CODE: &str = "interrupted";
const INTERRUPTED_ERROR_MESSAGE: &str =
    "Interrupted when the app closed. It will continue from where it stopped.";

/// Recorded when partial bytes had to be thrown away, so a transfer that
/// restarts from zero says why rather than appearing to lose progress.
const RESTARTED_NOTICE_CODE: &str = "restarted";

/// Recorded on a task paused because the international quota is used up.
pub const QUOTA_NOTICE_CODE: &str = "quota";

#[derive(Debug, Error)]
pub enum DownloadServiceError {
    #[error("the download folder must be an absolute path")]
    RelativeDirectory,

    #[error("download engine error: {0}")]
    Download(#[from] DownloadError),

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("system clock is before Unix epoch")]
    ClockBeforeUnixEpoch,

    #[error("timestamp does not fit in SQLite INTEGER")]
    TimestampOverflow,

    #[error("download execution is unavailable")]
    ExecutionUnavailable,

    #[error("could not plan download segments: {0}")]
    SegmentPlanning(#[from] SegmentPlanError),

    #[error("download control registry is unavailable")]
    ControlRegistryUnavailable,

    #[error("download {0} is not running")]
    NotRunning(String),

    #[error("a browser session can only be attached before a download starts, not while it is {0}")]
    SessionNotAccepted(dm_common::DownloadStatus),

    #[error("browser session refused: {0}")]
    BrowserSession(#[from] crate::session::SessionError),

    #[error("download {0} is already running")]
    AlreadyRunning(String),

    #[error("{0}")]
    Network(#[from] crate::network::NetworkError),

    #[error("the international traffic quota is used up ({used} of {quota} bytes)")]
    QuotaReached { used: u64, quota: u64 },
}

pub type Result<T> = std::result::Result<T, DownloadServiceError>;

/// Setting keys owned by the engine.
pub const SETTING_DEFAULT_DIRECTORY: &str = "default_download_directory";
pub const SETTING_GLOBAL_SPEED_LIMIT: &str = "global_speed_limit";
/// International bytes allowed per period; empty or `0` means no quota.
pub const SETTING_INTERNATIONAL_QUOTA: &str = "traffic_international_quota";
/// First day (`YYYY-MM-DD`) of the current quota period, such as the day an
/// internet package was bought. Without it the last 30 days are counted.
pub const SETTING_QUOTA_PERIOD_START: &str = "traffic_period_start";
/// Most connections one download may open.
pub const SETTING_MAX_CONNECTIONS: &str = "max_connections_per_download";

/// What the intake rules decided for one running transfer, after probing
/// told the engine the file's type and size.
#[derive(Clone)]
struct TaskOverrides {
    limiter: Option<Arc<RateLimiter>>,
    max_connections: Option<usize>,
}

/// What the traffic meter shows: the quota period, today and this month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficSummary {
    pub period_start: String,
    pub period_domestic_bytes: u64,
    pub period_international_bytes: u64,
    pub today_domestic_bytes: u64,
    pub today_international_bytes: u64,
    pub month_domestic_bytes: u64,
    pub month_international_bytes: u64,
    pub international_quota: Option<u64>,
    /// False when the period is the default rolling 30 days.
    pub explicit_period: bool,
}

#[derive(Clone)]
pub struct DownloadService {
    /// Rebuilt when the network settings change; clones share it.
    downloader: Arc<RwLock<Downloader>>,
    storage: Arc<Storage>,
    execution_slots: Arc<Semaphore>,
    /// Handles for transfers running in this process, so pause and cancel
    /// reach the transfer itself instead of only changing a row.
    controls: Arc<Mutex<HashMap<String, Arc<TaskControl>>>>,
    retry_policy: RetryPolicy,
    segment_connections: usize,
    segmented_threshold: u64,
    min_segment_bytes: u64,
    min_split_bytes: u64,
    evaluation_window: Duration,
    checkpoint_bytes: u64,
    checkpoint_interval: Duration,
    /// The local UTC offset, for counting traffic per local day.
    utc_offset_seconds: Arc<AtomicI32>,
    /// The application-wide bandwidth limit every transfer draws from.
    global_limiter: Arc<RateLimiter>,
    /// Rule-derived limits of transfers running now, by task id.
    overrides: Arc<Mutex<HashMap<String, TaskOverrides>>>,
    /// Browser sessions handed over for tasks, by task id. Memory only: a
    /// session is never written to storage and is forgotten on restart.
    sessions: Arc<Mutex<HashMap<String, Arc<BrowserSession>>>>,
}

impl DownloadService {
    pub fn new(storage: Arc<Storage>) -> Result<Self> {
        let global_limit = storage
            .get_setting(SETTING_GLOBAL_SPEED_LIMIT)?
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|limit| *limit > 0);

        let network = NetworkSettings::load(&storage);
        let segment_connections = storage
            .get_setting(SETTING_MAX_CONNECTIONS)?
            .and_then(|value| value.parse::<usize>().ok())
            .map_or(DEFAULT_SEGMENT_CONNECTIONS, |value| {
                value.clamp(1, MAX_SEGMENT_CONNECTIONS)
            });

        Ok(Self {
            downloader: Arc::new(RwLock::new(Downloader::with_network(&network)?)),
            storage,
            execution_slots: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_DOWNLOADS)),
            controls: Arc::new(Mutex::new(HashMap::new())),
            retry_policy: RetryPolicy::default(),
            segment_connections,
            segmented_threshold: DEFAULT_SEGMENTED_THRESHOLD,
            min_segment_bytes: DEFAULT_MIN_SEGMENT_BYTES,
            min_split_bytes: DEFAULT_MIN_SPLIT_BYTES,
            evaluation_window: DEFAULT_EVALUATION_WINDOW,
            checkpoint_bytes: CHECKPOINT_BYTES,
            checkpoint_interval: CHECKPOINT_INTERVAL,
            utc_offset_seconds: Arc::new(AtomicI32::new(0)),
            global_limiter: Arc::new(RateLimiter::new(global_limit)),
            overrides: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Attaches a browser session to a task the browser just handed over.
    ///
    /// Only a task that has not started yet accepts one, so a session always
    /// arrives before the first request and can never be swapped under a
    /// running transfer.
    pub fn attach_browser_session(&self, download_id: &str, cookie_header: &str) -> Result<()> {
        let task = self.get_task(download_id)?;

        if task.status != dm_common::DownloadStatus::Created {
            return Err(DownloadServiceError::SessionNotAccepted(task.status));
        }

        let session = BrowserSession::new(&task.source_url, cookie_header)
            .map_err(DownloadServiceError::BrowserSession)?;

        self.sessions
            .lock()
            .map_err(|_| DownloadServiceError::ControlRegistryUnavailable)?
            .insert(download_id.to_owned(), Arc::new(session));

        Ok(())
    }

    /// Checks links without downloading them. Uses the plain engine, never a
    /// task's browser session or context.
    pub async fn probe_links(&self, urls: Vec<String>) -> Vec<crate::linkgrabber::LinkProbe> {
        crate::linkgrabber::probe_links(&self.base_downloader(), urls).await
    }

    /// Drops a task's browser session. Called when the task can no longer
    /// need it: finished, cancelled or removed.
    pub fn forget_browser_session(&self, download_id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(download_id);
        }
    }

    /// Whether a task currently holds a browser session. Reports presence
    /// only; the session itself never leaves this service.
    pub fn has_browser_session(&self, download_id: &str) -> bool {
        self.sessions
            .lock()
            .map(|sessions| sessions.contains_key(download_id))
            .unwrap_or(false)
    }

    fn browser_session_for(&self, download_id: &str) -> Option<Arc<BrowserSession>> {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(download_id).cloned())
    }

    /// True while any transfer is moving bytes in this process.
    pub fn has_running_transfers(&self) -> bool {
        self.controls
            .lock()
            .map(|controls| !controls.is_empty())
            .unwrap_or(false)
    }

    /// The application-wide bandwidth limit in bytes per second, if any.
    pub fn global_speed_limit(&self) -> Option<u64> {
        self.global_limiter.limit()
    }

    /// Persists and applies the application-wide limit. Transfers already
    /// running slow down or speed up immediately.
    pub fn set_global_speed_limit(&self, limit: Option<u64>) -> Result<()> {
        let limit = limit.filter(|value| *value > 0);
        self.storage
            .set_setting(SETTING_GLOBAL_SPEED_LIMIT, &limit.unwrap_or(0).to_string())?;
        self.global_limiter.set_limit(limit);
        Ok(())
    }

    /// The folder new downloads go to when no rule or category names one.
    pub fn default_directory(&self) -> Result<Option<PathBuf>> {
        Ok(self
            .storage
            .get_setting(SETTING_DEFAULT_DIRECTORY)?
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()))
    }

    /// Sets the default folder; `None` returns to the system Downloads folder.
    pub fn set_default_directory(&self, directory: Option<&Path>) -> Result<()> {
        match directory {
            Some(directory) if directory.is_absolute() => self
                .storage
                .set_setting(SETTING_DEFAULT_DIRECTORY, &directory.to_string_lossy())?,
            Some(_) => return Err(DownloadServiceError::RelativeDirectory),
            None => self.storage.set_setting(SETTING_DEFAULT_DIRECTORY, "")?,
        }
        Ok(())
    }

    /// Picks the folder for a task: a matching rule's folder, then its
    /// category's folder, then the configured default, then `fallback`
    /// (the system Downloads folder). Relative paths are never used.
    fn resolve_destination(&self, decision: Option<&RuleDecision>, fallback: &Path) -> PathBuf {
        let absolute = |value: &str| {
            let path = PathBuf::from(value.trim());
            (!value.trim().is_empty() && path.is_absolute()).then_some(path)
        };

        let rule_directory = decision
            .and_then(|decision| decision.destination_directory.as_deref())
            .and_then(absolute);
        let category_directory = || {
            let category_id = decision?.category_id.as_deref()?;
            let category = self.storage.get_category(category_id).ok()??;
            absolute(category.default_directory.as_deref()?)
        };

        rule_directory
            .or_else(category_directory)
            .or_else(|| self.default_directory().ok().flatten())
            .unwrap_or_else(|| fallback.to_path_buf())
    }

    fn task_overrides(&self, download_id: &str) -> Option<TaskOverrides> {
        self.overrides
            .lock()
            .ok()
            .and_then(|overrides| overrides.get(download_id).cloned())
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn with_segment_connections(mut self, segment_connections: usize) -> Self {
        self.segment_connections = segment_connections.clamp(1, MAX_SEGMENT_CONNECTIONS);
        self
    }

    pub fn with_segmented_threshold(mut self, threshold: u64) -> Self {
        self.segmented_threshold = threshold;
        self
    }

    /// Smallest planned range and smallest piece a running range is split
    /// into. Tests use tiny values to exercise splitting on small bodies.
    pub fn with_segment_sizes(mut self, min_segment_bytes: u64, min_split_bytes: u64) -> Self {
        self.min_segment_bytes = min_segment_bytes.max(1);
        self.min_split_bytes = min_split_bytes.max(1);
        self
    }

    /// How long a connection count is held before it is judged.
    pub fn with_evaluation_window(mut self, window: Duration) -> Self {
        self.evaluation_window = window;
        self
    }

    /// How often each connection flushes the shared file and records how
    /// far it safely got.
    pub fn with_checkpoints(mut self, bytes: u64, interval: Duration) -> Self {
        self.checkpoint_bytes = bytes.max(1);
        self.checkpoint_interval = interval;
        self
    }

    fn base_downloader(&self) -> Downloader {
        self.downloader
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The UTC offset used to decide which local day traffic belongs to.
    pub fn set_utc_offset_seconds(&self, offset: i32) {
        self.utc_offset_seconds.store(offset, Ordering::Relaxed);
    }

    fn today(&self) -> Result<String> {
        Ok(local_day(
            unix_timestamp_seconds()?,
            self.utc_offset_seconds.load(Ordering::Relaxed),
        ))
    }

    pub fn network_settings(&self) -> NetworkSettings {
        NetworkSettings::load(&self.storage)
    }

    /// Validates, stores and applies new network settings. Transfers that
    /// are already running keep their connections; new requests use the new
    /// route.
    pub fn set_network_settings(&self, settings: &NetworkSettings) -> Result<()> {
        settings.validate()?;
        let downloader = Downloader::with_network(settings)?;
        settings.save(&self.storage)?;
        *self
            .downloader
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = downloader;
        Ok(())
    }

    /// Most connections a download may use.
    pub fn max_connections(&self) -> usize {
        self.storage
            .get_setting(SETTING_MAX_CONNECTIONS)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<usize>().ok())
            .map_or(self.segment_connections, |value| {
                value.clamp(1, MAX_SEGMENT_CONNECTIONS)
            })
    }

    pub fn set_max_connections(&self, connections: usize) -> Result<()> {
        let connections = connections.clamp(1, MAX_SEGMENT_CONNECTIONS);
        self.storage
            .set_setting(SETTING_MAX_CONNECTIONS, &connections.to_string())?;
        Ok(())
    }

    /// Traffic counted in the current quota period, and the quota itself.
    pub fn traffic_summary(&self) -> Result<TrafficSummary> {
        let today = self.today()?;
        let period_start = self.quota_period_start(&today)?;
        let totals = self.storage.traffic_between(&period_start, &today)?;
        let month = crate::traffic::month_bounds(&today);
        let today_totals = self.storage.traffic_between(&today, &today)?;
        let month_totals = self.storage.traffic_between(&month.0, &month.1)?;
        Ok(TrafficSummary {
            period_start,
            period_domestic_bytes: totals.domestic_bytes,
            period_international_bytes: totals.international_bytes,
            today_domestic_bytes: today_totals.domestic_bytes,
            today_international_bytes: today_totals.international_bytes,
            month_domestic_bytes: month_totals.domestic_bytes,
            month_international_bytes: month_totals.international_bytes,
            international_quota: self.international_quota()?,
            explicit_period: self
                .storage
                .get_setting(SETTING_QUOTA_PERIOD_START)?
                .is_some_and(|value| !value.trim().is_empty()),
        })
    }

    /// Sets the international quota (bytes; `None` for no quota) and the day
    /// its period started (`None` for a rolling 30 days).
    pub fn set_traffic_quota(&self, quota: Option<u64>, period_start: Option<&str>) -> Result<()> {
        if let Some(day) = period_start {
            // Validates the shape by asking storage for the range.
            self.storage.traffic_between(day, day)?;
        }
        self.storage.set_setting(
            SETTING_INTERNATIONAL_QUOTA,
            &quota
                .filter(|value| *value > 0)
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )?;
        self.storage
            .set_setting(SETTING_QUOTA_PERIOD_START, period_start.unwrap_or(""))?;
        Ok(())
    }

    fn international_quota(&self) -> Result<Option<u64>> {
        Ok(self
            .storage
            .get_setting(SETTING_INTERNATIONAL_QUOTA)?
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0))
    }

    fn quota_period_start(&self, today: &str) -> Result<String> {
        if let Some(day) = self
            .storage
            .get_setting(SETTING_QUOTA_PERIOD_START)?
            .filter(|value| !value.trim().is_empty() && value.as_str() <= today)
        {
            return Ok(day);
        }
        let thirty_days_ago = unix_timestamp_seconds()?.saturating_sub(29 * 86_400);
        Ok(local_day(
            thirty_days_ago,
            self.utc_offset_seconds.load(Ordering::Relaxed),
        ))
    }

    /// Refuses to start international transfers once the quota is used up.
    fn ensure_quota_allows(&self, scope: TrafficScope) -> Result<()> {
        if scope == TrafficScope::Domestic {
            return Ok(());
        }
        let Some(quota) = self.international_quota()? else {
            return Ok(());
        };
        let today = self.today()?;
        let start = self.quota_period_start(&today)?;
        let used = self
            .storage
            .traffic_between(&start, &today)?
            .international_bytes;
        if used >= quota {
            return Err(DownloadServiceError::QuotaReached { used, quota });
        }
        Ok(())
    }

    /// The traffic scope of a URL, by its host.
    fn traffic_scope_of(&self, url: &str) -> TrafficScope {
        let host = normalized_host(url).unwrap_or_default();
        let domestic = NetworkSettings::load(&self.storage).domestic_hosts;
        classify_host(&host, &domestic)
    }

    /// Adds newly downloaded bytes to today's total. Counting is best
    /// effort: a failure to record never stops a download.
    fn count_traffic(&self, scope: TrafficScope, bytes: u64) {
        if bytes == 0 {
            return;
        }
        if let Ok(today) = self.today() {
            let _ = self.storage.record_traffic(&today, scope, bytes);
        }
    }

    /// Creates a task together with the browser context it arrived with.
    /// The context is stored only when the task itself was created.
    pub fn create_task_with_context(
        &self,
        source_url: &str,
        context: &RequestContext,
    ) -> Result<DownloadRecord> {
        let task = self.create_task(source_url)?;
        self.storage.set_request_context(&task.id, context)?;
        Ok(task)
    }

    /// The shared downloader carrying this task's stored browser context and
    /// every bandwidth limit that applies to it.
    fn downloader_for(&self, download_id: &str) -> Result<Downloader> {
        let context = self.storage.get_request_context(download_id)?;
        let mut downloader = self
            .base_downloader()
            .with_context(&context)
            .with_session(self.browser_session_for(download_id))
            .with_limiter(Arc::clone(&self.global_limiter));

        if let Some(limiter) = self
            .task_overrides(download_id)
            .and_then(|overrides| overrides.limiter)
        {
            downloader = downloader.with_limiter(limiter);
        }

        Ok(downloader)
    }

    pub fn create_task(&self, source_url: &str) -> Result<DownloadRecord> {
        validate_source_url(source_url)?;

        self.storage
            .create_download(source_url, unix_timestamp_seconds()?)
            .map_err(DownloadServiceError::Storage)
    }

    /// Evaluates the persisted intake rules using URL-only facts available
    /// before probing. MIME and size matches are applied later by the probe;
    /// this method keeps the decision backend-owned for Tauri/queue callers.
    pub fn rule_decision_for_url(&self, source_url: &str) -> Result<Option<RuleDecision>> {
        validate_source_url(source_url)?;
        let rules = self.storage.list_rules()?;
        let categories = self.storage.list_categories()?;
        Ok(evaluate_rules(source_url, None, None, &rules, &categories))
    }

    pub fn claim_task(&self, download_id: &str) -> Result<DownloadRecord> {
        self.storage.mark_probing(download_id)?;
        self.get_task(download_id)
    }

    /// Returns tasks that a previous process left mid-transfer to a state the
    /// user or a queue runner can act on again. Called once during startup,
    /// before any runner is spawned, so no row is ever presented as active
    /// with nobody driving it.
    pub fn recover_orphaned_tasks(&self) -> Result<Vec<DownloadRecord>> {
        self.storage
            .recover_orphaned_downloads(INTERRUPTED_ERROR_CODE, INTERRUPTED_ERROR_MESSAGE)
            .map_err(DownloadServiceError::Storage)
    }

    /// Stops a running transfer and keeps everything it has already written.
    ///
    /// The running transfer persists the pause itself, so the byte count that
    /// is stored is the one actually on disk.
    pub fn pause_task(&self, download_id: &str) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        match self.control_for(download_id)? {
            Some(control) => {
                control.request(StopReason::Pause);
                Ok(task)
            }
            None if task.status.is_executing() => {
                // The row claims to be running but nothing in this process is
                // driving it; persist the pause directly rather than leaving
                // a task nobody can stop.
                self.storage
                    .mark_paused(download_id, task.downloaded_bytes)?;
                self.get_task(download_id)
            }
            None => Err(DownloadServiceError::NotRunning(download_id.to_owned())),
        }
    }

    /// Asks every transfer running in this process to pause. Each one
    /// persists its own pause, exactly as a single pause does. Returns the
    /// ids that were asked.
    pub fn pause_all(&self) -> Vec<String> {
        let Ok(controls) = self.controls.lock() else {
            return Vec::new();
        };
        controls
            .iter()
            .map(|(id, control)| {
                control.request(StopReason::Pause);
                id.clone()
            })
            .collect()
    }

    /// Ends a task and discards its partial transfer. A running transfer
    /// persists the cancellation itself; anything else is cancelled in place.
    pub async fn cancel_task(&self, download_id: &str) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        if let Some(control) = self.control_for(download_id)? {
            control.request(StopReason::Cancel);
            return Ok(task);
        }

        remove_partial_file(task.temp_path.as_deref()).await;
        remove_task_segments(&self.storage, download_id).await;
        self.storage.mark_cancelled(download_id)?;
        self.forget_browser_session(download_id);
        self.get_task(download_id)
    }

    /// Explicitly discards all partial transfer bytes and metadata, preserving
    /// the task identity and source URL for a fresh probe. Completed history is
    /// intentionally final; users can remove it and add the source again.
    pub async fn restart_task(&self, download_id: &str) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        if self.control_for(download_id)?.is_some() {
            return Err(DownloadServiceError::AlreadyRunning(download_id.to_owned()));
        }

        remove_partial_file(task.temp_path.as_deref()).await;
        remove_task_segments(&self.storage, download_id).await;

        self.storage.reset_for_restart(download_id)?;
        self.get_task(download_id)
    }

    /// Replaces an expired or corrected source URL while preserving the task
    /// identity. The next attempt probes the new source from byte zero.
    pub fn refresh_source_url(
        &self,
        download_id: &str,
        source_url: &str,
    ) -> Result<DownloadRecord> {
        validate_source_url(source_url)?;

        if self.control_for(download_id)?.is_some() {
            return Err(DownloadServiceError::AlreadyRunning(download_id.to_owned()));
        }

        self.storage.update_source_url(download_id, source_url)?;
        self.get_task(download_id)
    }

    /// True while this process is transferring the task.
    pub fn is_running(&self, download_id: &str) -> Result<bool> {
        Ok(self.control_for(download_id)?.is_some())
    }

    /// Retrying tasks whose backoff has elapsed.
    pub fn due_retries(&self) -> Result<Vec<DownloadRecord>> {
        Ok(self.storage.list_due_retries(unix_timestamp_seconds()?)?)
    }

    pub async fn start_task(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
    ) -> Result<DownloadRecord> {
        self.start_task_with_progress(download_id, destination_directory, |_, _| {})
            .await
    }

    pub async fn start_task_with_progress<F>(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
        on_progress: F,
    ) -> Result<DownloadRecord>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        self.claim_task(download_id)?;
        self.execute_claimed_task_with_progress(download_id, destination_directory, on_progress)
            .await
    }

    pub async fn execute_claimed_task_with_progress<F>(
        &self,
        download_id: &str,
        destination_directory: impl AsRef<Path>,
        mut on_progress: F,
    ) -> Result<DownloadRecord>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let task = self.get_task(download_id)?;
        let _execution_slot = Arc::clone(&self.execution_slots)
            .acquire_owned()
            .await
            .map_err(|_| DownloadServiceError::ExecutionUnavailable)?;

        let control = self.register_control(download_id)?;
        let _guard = ControlGuard {
            controls: Arc::clone(&self.controls),
            overrides: Arc::clone(&self.overrides),
            download_id: download_id.to_owned(),
        };

        let result = self
            .run_transfer(
                &task,
                destination_directory.as_ref(),
                control,
                &mut on_progress,
            )
            .await;

        match result {
            Ok(outcome) => {
                self.storage.mark_finalizing(download_id)?;
                self.complete_download(download_id, outcome)
            }
            Err(DownloadServiceError::Download(DownloadError::Stopped(reason))) => {
                self.persist_stop(download_id, reason).await
            }
            Err(DownloadServiceError::Download(error)) => {
                self.persist_failure(download_id, &task, error)
            }
            Err(DownloadServiceError::QuotaReached { .. }) => {
                // Not a failure of the download: it waits, paused, until the
                // quota is raised or a new period starts.
                let current = self.get_task(download_id)?;
                self.storage
                    .mark_paused(download_id, current.downloaded_bytes)?;
                self.storage.record_notice(
                    download_id,
                    QUOTA_NOTICE_CODE,
                    "Paused: the international traffic quota is used up.",
                )?;
                self.get_task(download_id)
            }
            Err(error) => Err(error),
        }
    }

    /// Probes the source, decides what to do with any partial file, and moves
    /// the remaining bytes.
    async fn run_transfer<F>(
        &self,
        task: &DownloadRecord,
        destination_directory: &Path,
        control: Arc<TaskControl>,
        on_progress: &mut F,
    ) -> Result<crate::TransferOutcome>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let downloader = self.downloader_for(&task.id)?;
        let probe = match downloader.probe(&task.source_url).await {
            Ok(probe) => probe,
            Err(error) => {
                if let DownloadError::Http(http_error) = &error
                    && let Some(status) = http_error.status()
                    && matches!(status.as_u16(), 429 | 503)
                    && let Ok(host) = normalized_host(&task.source_url)
                {
                    self.storage.record_host_observation(
                        &host,
                        Some(status.as_u16()),
                        self.segment_connections.saturating_sub(1).max(1) as u32,
                        unix_timestamp_seconds()?,
                    )?;
                }
                return Err(DownloadServiceError::Download(error));
            }
        };

        // Probing told us the type and size, so the full rule set can decide
        // the folder and any per-task limits now.
        let decision = evaluate_rules(
            &task.source_url,
            probe.content_type.as_deref(),
            probe.total_bytes,
            &self.storage.list_rules()?,
            &self.storage.list_categories()?,
        );
        if let Ok(mut overrides) = self.overrides.lock() {
            overrides.insert(
                task.id.clone(),
                TaskOverrides {
                    limiter: decision
                        .as_ref()
                        .and_then(|decision| decision.speed_cap)
                        .filter(|cap| *cap > 0)
                        .map(|cap| Arc::new(RateLimiter::new(Some(cap)))),
                    max_connections: decision
                        .as_ref()
                        .and_then(|decision| decision.max_connections)
                        .map(|value| value.max(1) as usize),
                },
            );
        }

        // Paths are reserved once and then kept, so a resumed task writes to
        // the same partial file rather than starting a second one.
        let (destination_path, temp_path) = match (&task.destination_path, &task.temp_path) {
            (Some(destination), Some(temp)) => (PathBuf::from(destination), PathBuf::from(temp)),
            _ => {
                let directory = self.resolve_destination(decision.as_ref(), destination_directory);
                self.base_downloader()
                    .plan_paths(&directory, &probe.filename)
                    .await?
            }
        };

        let scope = self.traffic_scope_of(&probe.final_url);
        self.ensure_quota_allows(scope)?;

        let mut stored = StoredTransfer {
            downloaded_bytes: task.downloaded_bytes,
            total_bytes: task.total_bytes,
            etag: task.etag.clone(),
            last_modified: task.last_modified.clone(),
            partial_bytes_on_disk: partial_file_size(&temp_path).await,
        };

        self.storage.set_transfer_plan(
            &task.id,
            &TransferPlan {
                resolved_url: probe.final_url.clone(),
                filename: probe.filename.clone(),
                destination_path: destination_path.to_string_lossy().into_owned(),
                temp_path: temp_path.to_string_lossy().into_owned(),
                mime_type: probe.content_type.clone(),
                total_bytes: probe.total_bytes,
                etag: probe.etag.clone(),
                last_modified: probe.last_modified.clone(),
                range_supported: probe.range_supported,
            },
        )?;

        let segmented = probe.range_supported
            && probe
                .total_bytes
                .is_some_and(|total| total >= self.segmented_threshold)
            && self.max_connections() > 1;

        if !segmented && !self.storage.list_download_segments(&task.id)?.is_empty() {
            // The partial file was laid out for ranges (the source or the
            // settings changed since): a single stream cannot continue it.
            self.storage.clear_download_segments(&task.id)?;
            self.storage.reset_transfer_progress(&task.id)?;
            stored = StoredTransfer::fresh();
        }

        if segmented {
            self.storage
                .mark_downloading(&task.id, unix_timestamp_seconds()?)?;

            match self
                .run_segmented_transfer(
                    task,
                    &probe,
                    &temp_path,
                    &destination_path,
                    Arc::clone(&control),
                    scope,
                    on_progress,
                )
                .await?
            {
                SegmentedTransferResult::Completed(outcome) => return Ok(outcome),
                SegmentedTransferResult::Fallback(segments) => {
                    self.storage.clear_download_segments(&task.id)?;
                    self.storage.reset_transfer_progress(&task.id)?;
                    remove_segment_files(&segments).await;
                    stored = StoredTransfer::fresh();
                }
            }
        }

        self.run_single_stream(
            SingleStreamRequest {
                task,
                probe: &probe,
                temp_path: &temp_path,
                destination_path: &destination_path,
                stored,
                control,
                scope,
            },
            on_progress,
        )
        .await
    }

    async fn run_single_stream<F>(
        &self,
        request: SingleStreamRequest<'_>,
        on_progress: &mut F,
    ) -> Result<crate::TransferOutcome>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let start_offset = match plan_resume(&request.stored, request.probe) {
            ResumePlan::ContinueFrom(offset) | ResumePlan::AlreadyComplete(offset) => offset,
            ResumePlan::StartFromZero(reason) => {
                self.storage.reset_transfer_progress(&request.task.id)?;
                if request.stored.downloaded_bytes > 0 {
                    self.storage.record_notice(
                        &request.task.id,
                        RESTARTED_NOTICE_CODE,
                        reason.explanation(),
                    )?;
                }
                0
            }
        };

        self.storage
            .mark_downloading(&request.task.id, unix_timestamp_seconds()?)?;

        let storage = Arc::clone(&self.storage);
        let counter = self.clone();
        let scope = request.scope;
        let progress_id = request.task.id.clone();
        let reached = Arc::new(AtomicU64::new(start_offset));
        let reached_writer = Arc::clone(&reached);
        let mut last_persisted_at = Instant::now();
        let mut last_persisted_bytes = start_offset;
        let mut has_persisted_progress = false;
        let mut meter = ThroughputMeter::new(Instant::now(), start_offset);

        let outcome = self
            .downloader_for(&request.task.id)?
            .transfer(
                TransferRequest {
                    source_url: &request.task.source_url,
                    temp_path: request.temp_path,
                    destination_path: request.destination_path,
                    start_offset,
                    total_bytes: request.probe.total_bytes,
                },
                request.control.as_ref(),
                |progress| {
                    reached_writer.store(progress.downloaded_bytes, Ordering::Relaxed);
                    let reached_known_end = progress.total_bytes == Some(progress.downloaded_bytes);
                    let bytes_since_persist = progress
                        .downloaded_bytes
                        .saturating_sub(last_persisted_bytes);
                    let should_persist = !has_persisted_progress
                        || reached_known_end
                        || bytes_since_persist >= PROGRESS_PERSIST_BYTES
                        || last_persisted_at.elapsed() >= PROGRESS_PERSIST_INTERVAL;

                    if should_persist {
                        storage
                            .update_progress(
                                &progress_id,
                                progress.downloaded_bytes,
                                progress.total_bytes,
                            )
                            .map_err(|error| DownloadError::ProgressCallback(error.to_string()))?;
                        counter.count_traffic(
                            scope,
                            progress
                                .downloaded_bytes
                                .saturating_sub(last_persisted_bytes),
                        );
                        has_persisted_progress = true;
                        last_persisted_at = Instant::now();
                        last_persisted_bytes = progress.downloaded_bytes;
                    }

                    let bytes_per_second = meter.sample(progress.downloaded_bytes, Instant::now());
                    on_progress(
                        &progress_id,
                        TransferProgress {
                            downloaded_bytes: progress.downloaded_bytes,
                            total_bytes: progress.total_bytes,
                            bytes_per_second,
                            eta_seconds: meter
                                .eta_seconds(progress.downloaded_bytes, progress.total_bytes),
                            active_connections: Some(1),
                            max_connections: Some(1),
                            adaptive_reason: Some("single stream"),
                        },
                    );
                    Ok(())
                },
            )
            .await;

        let reached_bytes = reached.load(Ordering::Relaxed);
        if outcome.is_err() {
            let _ = self.storage.update_progress(
                &request.task.id,
                reached_bytes,
                request.probe.total_bytes,
            );
        }
        Ok(outcome?)
    }

    /// Downloads a ranged source over several connections into one
    /// preallocated partial file.
    ///
    /// Each connection owns a range of the file. The number of connections
    /// follows the adaptive controller, judged once per evaluation window;
    /// when a connection is free and no planned range is left, the largest
    /// range still running is split and its untouched half handed over, so
    /// the end of a file is never left to one slow connection.
    #[allow(clippy::too_many_arguments)]
    async fn run_segmented_transfer<F>(
        &self,
        task: &DownloadRecord,
        probe: &crate::SourceProbe,
        temp_path: &Path,
        destination_path: &Path,
        control: Arc<TaskControl>,
        scope: TrafficScope,
        on_progress: &mut F,
    ) -> Result<SegmentedTransferResult>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let total_bytes = probe.total_bytes.ok_or(SegmentPlanError::EmptyResource)?;
        let host = normalized_host(&probe.final_url)?;
        let max_connections = self.max_connections();
        let connection_limit = self
            .storage
            .get_host_profile(&host)?
            .map(|profile| profile.preferred_max_connections as usize)
            .unwrap_or(max_connections)
            .clamp(1, max_connections)
            .min(
                self.task_overrides(&task.id)
                    .and_then(|overrides| overrides.max_connections)
                    .unwrap_or(usize::MAX),
            );

        let existing = self.storage.list_download_segments(&task.id)?;
        let source_changed = (task.total_bytes.is_some() && task.total_bytes != probe.total_bytes)
            || (task.etag.is_some() && task.etag != probe.etag)
            || (task.last_modified.is_some() && task.last_modified != probe.last_modified);
        let reusable = !source_changed
            && covers_exactly(&existing, total_bytes, temp_path)
            && partial_file_size(temp_path).await == Some(total_bytes);

        if !reusable {
            // A changed source, a damaged map, or ranges kept in separate
            // files by an older version: start the file over.
            remove_segment_files_except(&existing, temp_path).await;
            if !existing.is_empty() || source_changed {
                self.storage.reset_transfer_progress(&task.id)?;
                if existing.iter().any(|segment| segment.downloaded_bytes > 0) {
                    self.storage.record_notice(
                        &task.id,
                        RESTARTED_NOTICE_CODE,
                        if source_changed {
                            "The file changed on the server, so it is being downloaded again."
                        } else {
                            "The partial file could not be continued, so it is being downloaded again."
                        },
                    )?;
                }
            }
            let planned = plan_segments(
                &task.id,
                temp_path,
                total_bytes,
                connection_limit,
                self.min_segment_bytes,
            )?;
            crate::preallocate_partial_file(temp_path, total_bytes).await?;
            self.storage.replace_download_segments(&task.id, &planned)?;
        }

        self.storage.reset_incomplete_download_segments(&task.id)?;
        let current = self.storage.list_download_segments(&task.id)?;
        let initial_bytes: u64 = current.iter().map(|segment| segment.downloaded_bytes).sum();
        self.storage
            .update_progress(&task.id, initial_bytes, Some(total_bytes))?;

        let mut pool = SegmentPool {
            pending: current
                .iter()
                .filter(|segment| !segment.is_complete())
                .cloned()
                .collect(),
            active: HashMap::new(),
            next_index: current
                .iter()
                .map(|segment| segment.segment_index + 1)
                .max()
                .unwrap_or(0),
            workers: JoinSet::new(),
            min_split_bytes: self.min_split_bytes,
        };
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
        let context = SegmentPoolContext {
            downloader: self.downloader_for(&task.id)?,
            storage: Arc::clone(&self.storage),
            control: Arc::clone(&control),
            source_url: probe.final_url.clone(),
            temp_path: temp_path.to_path_buf(),
            total_bytes,
            checkpoint_bytes: self.checkpoint_bytes,
            checkpoint_interval: self.checkpoint_interval,
            events: event_tx,
        };

        let mut adaptive = AdaptiveController::new(connection_limit);
        let mut target_connections = adaptive.target_connections();
        pool.fill(&task.id, target_connections, &context)?;

        let progress_id = task.id.clone();
        let mut downloaded = initial_bytes;
        let mut meter = ThroughputMeter::new(Instant::now(), initial_bytes);
        let mut last_persisted_at = Instant::now();
        let mut last_persisted_bytes = initial_bytes;
        let mut last_emitted_at: Option<Instant> = None;
        let mut window = EvaluationWindow::start(pool.workers.len(), downloaded);

        let outcome: Result<()> = loop {
            if pool.workers.is_empty() {
                break Ok(());
            }
            tokio::select! {
                Some(written) = event_rx.recv() => {
                    downloaded = downloaded.saturating_add(written).min(total_bytes);
                }
                joined = pool.workers.join_next() => {
                    let Some(joined) = joined else {
                        break Ok(());
                    };
                    match joined {
                        Ok(Ok(finished)) => {
                            pool.active.remove(&finished.segment_index);
                            if let Err(error) = self.storage.update_download_segment_progress(
                                &task.id,
                                finished.segment_index,
                                finished.downloaded_bytes,
                            ).and_then(|()| self.storage.complete_download_segment(
                                &task.id,
                                finished.segment_index,
                            )) {
                                break Err(error.into());
                            }
                            if let Err(error) = pool.fill(&task.id, target_connections, &context) {
                                break Err(error);
                            }
                        }
                        Ok(Err(error)) => break Err(error),
                        Err(_join_error) => break Err(DownloadServiceError::ExecutionUnavailable),
                    }
                }
            }

            let now = Instant::now();
            let should_persist = downloaded == total_bytes
                || downloaded.saturating_sub(last_persisted_bytes) >= PROGRESS_PERSIST_BYTES
                || now.duration_since(last_persisted_at) >= PROGRESS_PERSIST_INTERVAL;
            if should_persist {
                if let Err(error) =
                    self.storage
                        .update_progress(&task.id, downloaded, Some(total_bytes))
                {
                    break Err(error.into());
                }
                self.count_traffic(scope, downloaded.saturating_sub(last_persisted_bytes));
                last_persisted_at = now;
                last_persisted_bytes = downloaded;
            }

            let bytes_per_second = meter.sample(downloaded, now);
            if let Some(sample) =
                window.observe(pool.workers.len(), downloaded, now, self.evaluation_window)
            {
                target_connections = adaptive.observe(sample).target_connections;
                if let Err(error) = pool.fill(&task.id, target_connections, &context) {
                    break Err(error);
                }
                window = EvaluationWindow::start(pool.workers.len(), downloaded);
            }

            if last_emitted_at.is_none_or(|at| now.duration_since(at) >= PROGRESS_EMIT_INTERVAL)
                || downloaded == total_bytes
            {
                last_emitted_at = Some(now);
                on_progress(
                    &progress_id,
                    TransferProgress {
                        downloaded_bytes: downloaded,
                        total_bytes: Some(total_bytes),
                        bytes_per_second,
                        eta_seconds: meter.eta_seconds(downloaded, Some(total_bytes)),
                        active_connections: Some(pool.workers.len().max(1) as u32),
                        max_connections: Some(connection_limit as u32),
                        adaptive_reason: Some(adaptive.reason().as_str()),
                    },
                );
            }
        };

        if let Err(error) = outcome {
            pool.workers.abort_all();
            while pool.workers.join_next().await.is_some() {}
            // Durable progress was written by the connections themselves.
            self.storage.reset_incomplete_download_segments(&task.id)?;
            let before = last_persisted_bytes;
            persist_segment_total(&self.storage, &task.id, total_bytes)?;
            let after = self
                .storage
                .list_download_segments(&task.id)?
                .iter()
                .map(|segment| segment.downloaded_bytes)
                .sum::<u64>();
            self.count_traffic(scope, after.saturating_sub(before));

            return match error {
                DownloadServiceError::Download(DownloadError::InvalidRangeResponse { .. }) => {
                    let fallback_segments = self.storage.list_download_segments(&task.id)?;
                    Ok(SegmentedTransferResult::Fallback(fallback_segments))
                }
                DownloadServiceError::Download(DownloadError::HttpStatus { status })
                    if matches!(status, 429 | 503) =>
                {
                    if let Some(decision) = adaptive.record_server_status(status) {
                        self.storage.record_host_observation(
                            &host,
                            Some(status),
                            decision.target_connections as u32,
                            unix_timestamp_seconds()?,
                        )?;
                        if let Some(backoff) = decision.backoff {
                            tokio::time::sleep(backoff).await;
                        }
                    }
                    Err(DownloadServiceError::Download(DownloadError::HttpStatus {
                        status,
                    }))
                }
                other => Err(other),
            };
        }

        let final_segments = self.storage.list_download_segments(&task.id)?;
        if final_segments.iter().any(|segment| !segment.is_complete())
            || !covers_exactly(&final_segments, total_bytes, temp_path)
        {
            self.storage.reset_incomplete_download_segments(&task.id)?;
            return Err(DownloadServiceError::Download(
                DownloadError::IncompleteTransfer {
                    expected: total_bytes,
                    actual: final_segments
                        .iter()
                        .map(|segment| segment.downloaded_bytes)
                        .sum(),
                },
            ));
        }

        self.count_traffic(scope, total_bytes.saturating_sub(last_persisted_bytes));
        let outcome = self
            .base_downloader()
            .finalize_shared_file(temp_path, destination_path, total_bytes)
            .await?;
        self.storage.clear_download_segments(&task.id)?;
        self.storage
            .update_progress(&task.id, total_bytes, Some(total_bytes))?;
        Ok(SegmentedTransferResult::Completed(outcome))
    }

    /// Persists a stop the user asked for. A pause keeps the partial file; a
    /// cancellation deletes it, because cancelling is terminal.
    async fn persist_stop(&self, download_id: &str, reason: StopReason) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        match reason {
            StopReason::Pause => {
                self.storage
                    .reset_incomplete_download_segments(download_id)?;
                self.storage
                    .mark_paused(download_id, task.downloaded_bytes)?;
            }
            StopReason::Cancel => {
                remove_partial_file(task.temp_path.as_deref()).await;
                remove_task_segments(&self.storage, download_id).await;
                self.storage.mark_cancelled(download_id)?;
                self.forget_browser_session(download_id);
            }
        }

        self.get_task(download_id)
    }

    /// Fails the task, or schedules another attempt when the failure is the
    /// kind that tends to pass and the attempt budget still allows it.
    fn persist_failure(
        &self,
        download_id: &str,
        task: &DownloadRecord,
        error: DownloadError,
    ) -> Result<DownloadRecord> {
        let message = error.redacted_message();
        let class = classify_failure(&error);
        let attempts = task.attempts.saturating_add(1);

        if class == FailureClass::Retryable
            && let Some(delay) = self.retry_policy.delay_for(attempts)
        {
            let retry_at = unix_timestamp_seconds()?.saturating_add(delay.as_secs() as i64);

            self.storage.mark_retrying(
                download_id,
                attempts,
                retry_at,
                error_code_for(&error),
                &message,
            )?;

            return Err(DownloadServiceError::Download(error));
        }

        self.storage.record_attempt(download_id, attempts)?;
        self.storage
            .mark_failed(download_id, error_code_for(&error), &message)?;

        Err(DownloadServiceError::Download(error))
    }

    fn register_control(&self, download_id: &str) -> Result<Arc<TaskControl>> {
        let control = Arc::new(TaskControl::new());

        self.controls
            .lock()
            .map_err(|_| DownloadServiceError::ControlRegistryUnavailable)?
            .insert(download_id.to_owned(), Arc::clone(&control));

        Ok(control)
    }

    fn control_for(&self, download_id: &str) -> Result<Option<Arc<TaskControl>>> {
        Ok(self
            .controls
            .lock()
            .map_err(|_| DownloadServiceError::ControlRegistryUnavailable)?
            .get(download_id)
            .map(Arc::clone))
    }

    fn get_task(&self, download_id: &str) -> Result<DownloadRecord> {
        self.storage
            .get_download(download_id)?
            .ok_or_else(|| StorageError::DownloadNotFound(download_id.to_owned()))
            .map_err(DownloadServiceError::Storage)
    }

    fn complete_download(
        &self,
        download_id: &str,
        outcome: crate::TransferOutcome,
    ) -> Result<DownloadRecord> {
        let task = self.get_task(download_id)?;

        let completion = DownloadCompletion {
            resolved_url: task
                .resolved_url
                .clone()
                .unwrap_or_else(|| task.source_url.clone()),
            filename: task
                .filename
                .clone()
                .or_else(|| {
                    outcome
                        .final_path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "download.bin".to_owned()),
            destination_path: outcome.final_path.to_string_lossy().into_owned(),
            mime_type: task.mime_type.clone(),
            total_bytes: task.total_bytes.or(Some(outcome.downloaded_bytes)),
            downloaded_bytes: outcome.downloaded_bytes,
        };

        self.storage
            .mark_completed(download_id, &completion, unix_timestamp_seconds()?)?;

        // A finished download never needs its session again.
        self.forget_browser_session(download_id);

        self.get_task(download_id)
    }
}

/// Unregisters the control handle when a transfer ends, however it ends.
struct ControlGuard {
    controls: Arc<Mutex<HashMap<String, Arc<TaskControl>>>>,
    overrides: Arc<Mutex<HashMap<String, TaskOverrides>>>,
    download_id: String,
}

impl Drop for ControlGuard {
    fn drop(&mut self) {
        if let Ok(mut controls) = self.controls.lock() {
            controls.remove(&self.download_id);
        }
        if let Ok(mut overrides) = self.overrides.lock() {
            overrides.remove(&self.download_id);
        }
    }
}

async fn partial_file_size(temp_path: &Path) -> Option<u64> {
    tokio::fs::metadata(temp_path)
        .await
        .ok()
        .map(|metadata| metadata.len())
}

async fn remove_segment_files(segments: &[DownloadSegment]) {
    for segment in segments {
        let _ = tokio::fs::remove_file(&segment.temp_path).await;
    }
}

/// Removes separate segment files left by older versions, never the shared
/// partial file itself.
async fn remove_segment_files_except(segments: &[DownloadSegment], shared: &Path) {
    let shared = shared.to_string_lossy();
    for segment in segments {
        if segment.temp_path != shared {
            let _ = tokio::fs::remove_file(&segment.temp_path).await;
        }
    }
}

async fn remove_task_segments(storage: &Storage, download_id: &str) {
    if let Ok(segments) = storage.list_download_segments(download_id) {
        remove_segment_files(&segments).await;
        let _ = storage.clear_download_segments(download_id);
    }
}

fn persist_segment_total(storage: &Storage, download_id: &str, total_bytes: u64) -> Result<()> {
    let segments = storage.list_download_segments(download_id)?;
    let downloaded: u64 = segments
        .iter()
        .map(|segment| segment.downloaded_bytes)
        .sum();
    storage.update_progress(download_id, downloaded, Some(total_bytes))?;
    Ok(())
}

struct SingleStreamRequest<'a> {
    task: &'a DownloadRecord,
    probe: &'a crate::SourceProbe,
    temp_path: &'a Path,
    destination_path: &'a Path,
    stored: StoredTransfer,
    control: Arc<TaskControl>,
    scope: TrafficScope,
}

struct SegmentPoolContext {
    downloader: Downloader,
    storage: Arc<Storage>,
    control: Arc<TaskControl>,
    source_url: String,
    temp_path: PathBuf,
    total_bytes: u64,
    checkpoint_bytes: u64,
    checkpoint_interval: Duration,
    /// Bytes written, as they are written.
    events: UnboundedSender<u64>,
}

struct FinishedRange {
    segment_index: u32,
    downloaded_bytes: u64,
}

/// The ranges of one transfer: those waiting for a connection and those
/// being downloaded, by segment index.
struct SegmentPool {
    pending: std::collections::VecDeque<DownloadSegment>,
    active: HashMap<u32, Arc<RangeSlot>>,
    next_index: u32,
    workers: JoinSet<std::result::Result<FinishedRange, DownloadServiceError>>,
    min_split_bytes: u64,
}

impl SegmentPool {
    /// Starts connections until `target` run: planned ranges first, then
    /// halves split off the largest ranges still running.
    fn fill(
        &mut self,
        download_id: &str,
        target: usize,
        context: &SegmentPoolContext,
    ) -> Result<()> {
        while self.workers.len() < target {
            let segment = match self.pending.pop_front() {
                Some(segment) => segment,
                None => match self.split_largest(download_id, context)? {
                    Some(segment) => segment,
                    None => break,
                },
            };
            self.spawn(segment, context)?;
        }
        Ok(())
    }

    fn split_largest(
        &mut self,
        download_id: &str,
        context: &SegmentPoolContext,
    ) -> Result<Option<DownloadSegment>> {
        let Some((&index, slot)) = self.active.iter().max_by_key(|(_, slot)| slot.remaining())
        else {
            return Ok(None);
        };
        let Some((tail_start, tail_end)) = slot.split_off(self.min_split_bytes) else {
            return Ok(None);
        };
        let tail = DownloadSegment {
            download_id: download_id.to_owned(),
            segment_index: self.next_index,
            start_byte: tail_start,
            end_byte: tail_end,
            downloaded_bytes: 0,
            temp_path: context.temp_path.to_string_lossy().into_owned(),
            status: SegmentStatus::Pending,
        };
        context
            .storage
            .split_download_segment(download_id, index, tail_start - 1, &tail)?;
        self.next_index += 1;
        Ok(Some(tail))
    }

    fn spawn(&mut self, segment: DownloadSegment, context: &SegmentPoolContext) -> Result<()> {
        context
            .storage
            .claim_download_segment(&segment.download_id, segment.segment_index)?;
        let slot = Arc::new(RangeSlot::new(
            segment.start_byte,
            segment.end_byte,
            segment.downloaded_bytes,
        ));
        self.active.insert(segment.segment_index, Arc::clone(&slot));

        let downloader = context.downloader.clone();
        let storage = Arc::clone(&context.storage);
        let control = Arc::clone(&context.control);
        let source_url = context.source_url.clone();
        let temp_path = context.temp_path.clone();
        let total_bytes = context.total_bytes;
        let checkpoint_bytes = context.checkpoint_bytes;
        let checkpoint_interval = context.checkpoint_interval;
        let events = context.events.clone();

        self.workers.spawn(async move {
            let download_id = segment.download_id.clone();
            let index = segment.segment_index;
            let outcome = downloader
                .transfer_range(
                    RangeTransferRequest {
                        source_url: &source_url,
                        file_path: &temp_path,
                        total_bytes,
                        checkpoint_bytes,
                        checkpoint_interval,
                    },
                    &slot,
                    control.as_ref(),
                    |progress: RangeProgress| {
                        if progress.written > 0 {
                            let _ = events.send(progress.written);
                        }
                        if let Some(durable) = progress.durable_bytes {
                            storage
                                .update_download_segment_progress(&download_id, index, durable)
                                .map_err(|error| {
                                    DownloadError::ProgressCallback(error.to_string())
                                })?;
                        }
                        Ok(())
                    },
                )
                .await?;
            Ok(FinishedRange {
                segment_index: index,
                downloaded_bytes: outcome.downloaded_bytes,
            })
        });
        Ok(())
    }
}

/// One period during which the number of running connections stayed the
/// same. Only such a period says anything about that number.
struct EvaluationWindow {
    connections: usize,
    started_at: Instant,
    started_bytes: u64,
}

impl EvaluationWindow {
    fn start(connections: usize, downloaded: u64) -> Self {
        Self {
            connections,
            started_at: Instant::now(),
            started_bytes: downloaded,
        }
    }

    /// A sample once the window is long enough; restarts silently when the
    /// connection count changed in between.
    fn observe(
        &mut self,
        connections: usize,
        downloaded: u64,
        now: Instant,
        length: Duration,
    ) -> Option<ThroughputSample> {
        if connections != self.connections {
            *self = Self {
                connections,
                started_at: now,
                started_bytes: downloaded,
            };
            return None;
        }
        let elapsed = now.duration_since(self.started_at);
        if elapsed < length || connections == 0 {
            return None;
        }
        let bytes = downloaded.saturating_sub(self.started_bytes);
        let rate = (bytes as f64 / elapsed.as_secs_f64()) as u64;
        Some(ThroughputSample {
            connections,
            bytes_per_second: rate,
        })
    }
}

enum SegmentedTransferResult {
    Completed(crate::TransferOutcome),
    Fallback(Vec<DownloadSegment>),
}

async fn remove_partial_file(temp_path: Option<&str>) {
    if let Some(path) = temp_path {
        let _ = tokio::fs::remove_file(path).await;
    }
}

fn error_code_for(error: &DownloadError) -> &'static str {
    match error {
        DownloadError::InvalidUrl(_) | DownloadError::UnsupportedScheme(_) => "invalid_source",
        DownloadError::Http(_) => "download_error",
        DownloadError::Io(_) => "filesystem_error",
        DownloadError::ProgressCallback(_) => "storage_error",
        DownloadError::IncompleteTransfer { .. } => "incomplete_transfer",
        DownloadError::InvalidRangeResponse { .. } => "invalid_range_response",
        DownloadError::SegmentOverflow { .. } => "segment_overflow",
        DownloadError::HttpStatus { status: 429 } => "rate_limited",
        DownloadError::HttpStatus { status: 503 } => "server_busy",
        DownloadError::HttpStatus { .. } => "download_error",
        DownloadError::Stopped(_) => "stopped",
    }
}

fn unix_timestamp_seconds() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DownloadServiceError::ClockBeforeUnixEpoch)?;

    i64::try_from(duration.as_secs()).map_err(|_| DownloadServiceError::TimestampOverflow)
}

fn normalized_host(source_url: &str) -> Result<String> {
    let url = validate_source_url(source_url)?;
    let host = url.host_str().ok_or_else(|| {
        DownloadServiceError::Download(DownloadError::InvalidUrl(
            "source URL has no host".to_owned(),
        ))
    })?;
    Ok(host.trim_end_matches('.').to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{DEFAULT_BODY, ServerBehaviour, TestServer};
    use dm_common::{DownloadPriority, DownloadRule, DownloadStatus};
    use tempfile::tempdir;
    use tokio::time::sleep;

    struct Harness {
        _root: tempfile::TempDir,
        destination: PathBuf,
        storage: Arc<Storage>,
        service: DownloadService,
    }

    fn harness() -> Harness {
        let root = tempdir().unwrap();
        let storage = Arc::new(Storage::open(root.path().join("downloads.db")).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();
        let destination = root.path().join("files");

        Harness {
            _root: root,
            destination,
            storage,
            service,
        }
    }

    /// A body slow enough to be paused or cancelled part-way through.
    fn slow_server() -> ServerBehaviour {
        ServerBehaviour {
            chunk_size: 4,
            chunk_delay: Some(Duration::from_millis(25)),
            ..ServerBehaviour::default()
        }
    }

    /// Waits until the partial file on disk actually holds bytes, which is
    /// the only way to know a transfer has moved data rather than merely
    /// having been claimed.
    async fn await_partial_bytes(storage: &Storage, id: &str) -> u64 {
        for _ in 0..400 {
            if let Some(temp) = storage.get_download(id).unwrap().unwrap().temp_path
                && let Ok(metadata) = tokio::fs::metadata(&temp).await
                && metadata.len() > 0
            {
                return metadata.len();
            }

            sleep(Duration::from_millis(5)).await;
        }

        panic!("{id} never wrote anything to its partial file");
    }

    async fn await_segment_bytes(storage: &Storage, id: &str) -> u64 {
        for _ in 0..400 {
            let bytes: u64 = storage
                .list_download_segments(id)
                .unwrap()
                .iter()
                .map(|segment| segment.downloaded_bytes)
                .sum();
            if bytes > 0 {
                return bytes;
            }
            sleep(Duration::from_millis(5)).await;
        }
        panic!("{id} never wrote anything to its segment files");
    }

    async fn await_status(storage: &Storage, id: &str, expected: DownloadStatus) {
        for _ in 0..400 {
            if storage.get_download(id).unwrap().unwrap().status == expected {
                return;
            }

            sleep(Duration::from_millis(10)).await;
        }

        let actual = storage.get_download(id).unwrap().unwrap().status;
        panic!("{id} never reached {expected}; it is {actual}");
    }

    #[tokio::test]
    async fn creates_task_without_contacting_the_source() {
        let harness = harness();

        let record = harness
            .service
            .create_task("http://127.0.0.1:1/source-is-offline")
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Created);
        assert_eq!(record.downloaded_bytes, 0);
        assert!(record.started_at.is_none());
        assert_eq!(harness.storage.list_downloads().unwrap().len(), 1);
    }

    #[test]
    fn rejects_non_http_task_without_persisting_it() {
        let harness = harness();

        let error = harness
            .service
            .create_task("file:///private/file.bin")
            .unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::Download(DownloadError::UnsupportedScheme(_))
        ));
        assert!(harness.storage.list_downloads().unwrap().is_empty());
    }

    #[tokio::test]
    async fn starts_existing_task_with_stable_id_and_no_duplicate_record() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("download.bin"))
            .unwrap();

        let record = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(record.id, created.id);
        assert_eq!(record.filename.as_deref(), Some("payload.bin"));
        assert_eq!(record.downloaded_bytes, DEFAULT_BODY.len() as u64);

        let bytes = tokio::fs::read(record.destination_path.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(bytes, DEFAULT_BODY);

        let persisted = harness.storage.get_download(&record.id).unwrap().unwrap();
        assert_eq!(persisted.status, DownloadStatus::Completed);
        assert!(persisted.started_at.is_some());
        assert!(persisted.completed_at.is_some());
        assert!(
            persisted.temp_path.is_none(),
            "a finished download no longer has a partial file"
        );

        let downloads = harness.storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
    }

    #[tokio::test]
    async fn restart_from_zero_removes_old_file_and_preserves_task_id() {
        let harness = harness();
        let created = harness
            .service
            .create_task("https://example.com/restart.bin")
            .unwrap();
        harness.storage.mark_probing(&created.id).unwrap();
        harness.storage.mark_paused(&created.id, 128).unwrap();

        let restarted = harness.service.restart_task(&created.id).await.unwrap();

        assert_eq!(restarted.id, created.id);
        assert_eq!(restarted.status, DownloadStatus::Created);
        assert_eq!(restarted.downloaded_bytes, 0);
        assert!(restarted.destination_path.is_none());
        assert_eq!(harness.storage.list_downloads().unwrap().len(), 1);
    }

    #[test]
    fn refreshing_source_url_validates_input_and_keeps_task_id() {
        let harness = harness();
        let created = harness
            .service
            .create_task("https://example.com/expired.bin")
            .unwrap();
        harness.storage.mark_probing(&created.id).unwrap();
        harness
            .storage
            .mark_failed(&created.id, "http_404", "not found")
            .unwrap();

        let refreshed = harness
            .service
            .refresh_source_url(&created.id, "https://cdn.example.com/fresh.bin")
            .unwrap();
        assert_eq!(refreshed.id, created.id);
        assert_eq!(refreshed.source_url, "https://cdn.example.com/fresh.bin");

        let invalid = harness
            .service
            .refresh_source_url(&created.id, "file:///not-http")
            .unwrap_err();
        assert!(matches!(
            invalid,
            DownloadServiceError::Download(DownloadError::UnsupportedScheme(_))
        ));
    }

    fn size_rule(name: &str) -> DownloadRule {
        DownloadRule {
            id: String::new(),
            name: name.to_owned(),
            enabled: true,
            sort_order: 0,
            domain: None,
            url_pattern: None,
            extension: None,
            mime_pattern: None,
            min_size: Some(1),
            max_size: None,
            category_id: None,
            destination_directory: None,
            queue_id: None,
            priority: None,
            max_connections: None,
            max_host_concurrency: None,
            speed_cap: None,
            browser_takeover_allowed: None,
        }
    }

    #[tokio::test]
    async fn files_go_to_rule_then_category_then_default_folder() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let root = harness.destination.parent().unwrap().to_path_buf();

        // 1. Configured default folder.
        let default_folder = root.join("default");
        harness
            .service
            .set_default_directory(Some(&default_folder))
            .unwrap();
        let task = harness.service.create_task(&server.url("a.bin")).unwrap();
        let record = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert!(
            Path::new(record.destination_path.as_deref().unwrap()).starts_with(&default_folder)
        );

        // 2. The matched category's folder beats the default. The test
        //    server answers application/octet-stream, which is Applications.
        let category_folder = root.join("apps");
        harness
            .storage
            .set_category_directory("applications", Some(&category_folder.to_string_lossy()))
            .unwrap();
        let task = harness.service.create_task(&server.url("b.bin")).unwrap();
        let record = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert!(
            Path::new(record.destination_path.as_deref().unwrap()).starts_with(&category_folder)
        );

        // 3. A matching rule's folder beats both.
        let rule_folder = root.join("rule");
        let mut rule = size_rule("Everything with a size");
        rule.destination_directory = Some(rule_folder.to_string_lossy().into_owned());
        harness.storage.create_rule(&rule).unwrap();
        let task = harness.service.create_task(&server.url("c.bin")).unwrap();
        let record = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert!(Path::new(record.destination_path.as_deref().unwrap()).starts_with(&rule_folder));
    }

    #[test]
    fn relative_default_folders_are_refused() {
        let harness = harness();
        assert!(matches!(
            harness
                .service
                .set_default_directory(Some(Path::new("relative/folder"))),
            Err(DownloadServiceError::RelativeDirectory)
        ));
    }

    #[tokio::test]
    async fn global_speed_limit_slows_the_transfer_and_persists() {
        let body = vec![7_u8; 60_000];
        let server = TestServer::start(ServerBehaviour {
            body: body.clone(),
            chunk_size: 4_096,
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        harness
            .service
            .set_global_speed_limit(Some(60_000))
            .unwrap();

        // Burst 16 KiB, the remaining ~44 KB at 60 KB/s take ~0.7 s.
        let task = harness
            .service
            .create_task(&server.url("slow.bin"))
            .unwrap();
        let started = std::time::Instant::now();
        let record = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(record.status, DownloadStatus::Completed);
        assert!(
            started.elapsed() >= Duration::from_millis(500),
            "{:?}",
            started.elapsed()
        );

        let reopened = DownloadService::new(Arc::clone(&harness.storage)).unwrap();
        assert_eq!(reopened.global_speed_limit(), Some(60_000));
        reopened.set_global_speed_limit(None).unwrap();
        assert_eq!(reopened.global_speed_limit(), None);
    }

    #[tokio::test]
    async fn a_rule_speed_cap_limits_only_matching_tasks() {
        let body = vec![7_u8; 60_000];
        let server = TestServer::start(ServerBehaviour {
            body,
            chunk_size: 4_096,
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let mut rule = size_rule("Slow capped");
        rule.min_size = None;
        rule.url_pattern = Some("*capped*".to_owned());
        rule.speed_cap = Some(60_000);
        harness.storage.create_rule(&rule).unwrap();

        let fast = harness
            .service
            .create_task(&server.url("free.bin"))
            .unwrap();
        let started = std::time::Instant::now();
        harness
            .service
            .start_task(&fast.id, &harness.destination)
            .await
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(400));

        let slow = harness
            .service
            .create_task(&server.url("capped.bin"))
            .unwrap();
        let started = std::time::Instant::now();
        harness
            .service
            .start_task(&slow.id, &harness.destination)
            .await
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(500));
    }

    #[tokio::test]
    async fn pause_all_pauses_every_running_transfer() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let first = harness.service.create_task(&server.url("a.bin")).unwrap();
        let second = harness.service.create_task(&server.url("b.bin")).unwrap();

        let runner = |id: String| {
            let service = harness.service.clone();
            let destination = harness.destination.clone();
            tokio::spawn(async move { service.start_task(&id, destination).await })
        };
        let first_run = runner(first.id.clone());
        let second_run = runner(second.id.clone());

        for _ in 0..100 {
            if harness.service.controls.lock().unwrap().len() == 2 {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        let mut paused = harness.service.pause_all();
        paused.sort();
        let mut expected = vec![first.id.clone(), second.id.clone()];
        expected.sort();
        assert_eq!(paused, expected);

        for handle in [first_run, second_run] {
            let record = handle.await.unwrap().unwrap();
            assert_eq!(record.status, DownloadStatus::Paused);
        }
        assert!(!harness.service.has_running_transfers());
    }

    #[test]
    fn intake_rule_decision_is_backend_owned_and_explainable() {
        let harness = harness();
        harness
            .storage
            .create_rule(&DownloadRule {
                id: String::new(),
                name: "Prefer media queue".to_owned(),
                enabled: true,
                sort_order: 0,
                domain: Some("media.example.com".to_owned()),
                url_pattern: None,
                extension: None,
                mime_pattern: None,
                min_size: None,
                max_size: None,
                category_id: None,
                destination_directory: None,
                queue_id: Some("default".to_owned()),
                priority: Some(DownloadPriority::High),
                max_connections: Some(4),
                max_host_concurrency: None,
                speed_cap: None,
                browser_takeover_allowed: None,
            })
            .unwrap();

        let decision = harness
            .service
            .rule_decision_for_url("https://media.example.com/movie.mp4")
            .unwrap()
            .unwrap();
        assert_eq!(decision.priority, Some(DownloadPriority::High));
        assert_eq!(decision.queue_id.as_deref(), Some("default"));
        assert!(decision.explanation.contains("Prefer media queue"));
    }

    #[tokio::test]
    async fn segmented_service_downloads_and_assembles_ranges() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(2)
            .with_segmented_threshold(1);
        let created = service.create_task(&server.url("segmented.bin")).unwrap();

        let record = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(record.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
        assert!(
            harness
                .storage
                .list_download_segments(&created.id)
                .unwrap()
                .is_empty()
        );
        assert!(server.ranged_request_count() >= 2);
    }

    #[tokio::test]
    async fn segmented_progress_explains_adaptive_connection_changes() {
        let server = TestServer::start(ServerBehaviour {
            chunk_size: 2,
            chunk_delay: Some(Duration::from_millis(25)),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(3)
            .with_segmented_threshold(1)
            .with_segment_sizes(1, 1)
            .with_evaluation_window(Duration::from_millis(40));
        let created = service.create_task(&server.url("adaptive.bin")).unwrap();
        let mut saw_multiple_connections = false;
        let mut saw_explanation = false;

        let record = service
            .start_task_with_progress(&created.id, &harness.destination, |_, progress| {
                if progress.active_connections.unwrap_or(0) > 1 {
                    saw_multiple_connections = true;
                }
                if progress.adaptive_reason.is_some() {
                    saw_explanation = true;
                }
            })
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert!(saw_multiple_connections);
        assert!(saw_explanation);
    }

    #[tokio::test]
    async fn browser_context_is_sent_to_hotlink_protected_servers() {
        let server = TestServer::start(ServerBehaviour {
            required_header: Some("referer: https://example.com/page".to_owned()),
            ..ServerBehaviour::default()
        })
        .await;
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(directory.path().join("downloads.db")).unwrap());
        let service = DownloadService::new(Arc::clone(&storage)).unwrap();

        let without = service.create_task(&server.url("file.bin")).unwrap();
        let failed = service
            .start_task(&without.id, directory.path().join("out"))
            .await;
        assert!(failed.is_err() || failed.unwrap().status == DownloadStatus::Failed);

        let with = service
            .create_task_with_context(
                &server.url("file.bin"),
                &RequestContext {
                    referrer: Some("https://example.com/page".to_owned()),
                    user_agent: Some("Mozilla/5.0 Test".to_owned()),
                },
            )
            .unwrap();
        let record = service
            .start_task(&with.id, directory.path().join("out"))
            .await
            .unwrap();
        assert_eq!(record.status, DownloadStatus::Completed);
    }

    #[tokio::test]
    async fn rate_limited_probe_updates_the_host_profile() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((429, "Too Many Requests")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let created = harness
            .service
            .create_task(&server.url("limited.bin"))
            .unwrap();

        let _ = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await;

        let profile = harness
            .storage
            .get_host_profile("127.0.0.1")
            .unwrap()
            .unwrap();
        assert_eq!(profile.rate_limited_count, 1);
        assert_eq!(profile.last_status, Some(429));
    }

    #[tokio::test]
    async fn segmented_pause_reuses_persisted_segment_files() {
        let server = TestServer::start(ServerBehaviour {
            chunk_size: 2,
            chunk_delay: Some(Duration::from_millis(25)),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(2)
            .with_segmented_threshold(1)
            .with_segment_sizes(1, 1)
            .with_checkpoints(2, Duration::from_millis(5));
        let created = service
            .create_task(&server.url("segmented-slow.bin"))
            .unwrap();
        let service_for_task = service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer =
            tokio::spawn(async move { service_for_task.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        let partial = await_segment_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();
        let paused = transfer.await.unwrap().unwrap();

        assert_eq!(paused.status, DownloadStatus::Paused);
        assert!(paused.downloaded_bytes >= partial);
        assert!(
            !harness
                .storage
                .list_download_segments(&created.id)
                .unwrap()
                .is_empty()
        );

        let resumed = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(resumed.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(resumed.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
    }

    #[tokio::test]
    async fn no_range_source_uses_single_stream_without_segment_rows() {
        let server = TestServer::start(ServerBehaviour {
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(2)
            .with_segmented_threshold(1);
        let created = service.create_task(&server.url("no-range.bin")).unwrap();

        let record = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert!(
            harness
                .storage
                .list_download_segments(&created.id)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            tokio::fs::read(record.destination_path.unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
    }

    #[test]
    fn rejects_duplicate_start_claim_with_typed_transition_error() {
        let harness = harness();

        let created = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();

        let claimed = harness.service.claim_task(&created.id).unwrap();
        assert_eq!(claimed.status, DownloadStatus::Probing);

        let error = harness.service.claim_task(&created.id).unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::Storage(StorageError::InvalidStatusTransition {
                from: DownloadStatus::Probing,
                to: DownloadStatus::Probing,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_permanent_failure_keeps_the_task_id_and_is_not_retried() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((404, "Not Found")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("missing.bin"))
            .unwrap();

        let error = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadServiceError::Download(_)));

        let downloads = harness.storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id, created.id);
        assert_eq!(
            downloads[0].status,
            DownloadStatus::Failed,
            "a 404 will not become a 200 by trying again"
        );
        assert_eq!(downloads[0].error_code.as_deref(), Some("download_error"));
        assert!(
            !downloads[0]
                .error_message
                .as_deref()
                .unwrap()
                .contains(&server.url("missing.bin")),
            "the persisted message must not carry the source URL"
        );
    }

    #[tokio::test]
    async fn a_temporary_failure_schedules_another_attempt() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((503, "Service Unavailable")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("busy.bin"))
            .unwrap();

        let _ = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await;

        let record = harness.storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(record.status, DownloadStatus::Retrying);
        assert_eq!(record.attempts, 1);
        assert!(
            record.retry_at.is_some(),
            "a retrying task must know when it becomes eligible again"
        );
    }

    #[tokio::test]
    async fn the_retry_budget_is_bounded() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((503, "Service Unavailable")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = harness.service.clone().with_retry_policy(RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            maximum_delay: Duration::from_millis(1),
        });

        let created = service.create_task(&server.url("busy.bin")).unwrap();

        let _ = service.start_task(&created.id, &harness.destination).await;
        assert_eq!(
            harness
                .storage
                .get_download(&created.id)
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Retrying
        );

        let _ = service.start_task(&created.id, &harness.destination).await;

        let record = harness.storage.get_download(&created.id).unwrap().unwrap();

        assert_eq!(
            record.status,
            DownloadStatus::Failed,
            "the last attempt in the budget fails for good"
        );
        assert_eq!(record.attempts, 2);
    }

    #[tokio::test]
    async fn pausing_keeps_the_partial_file_and_resuming_finishes_the_download() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("slow.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();

        let paused = transfer.await.unwrap().unwrap();

        assert_eq!(paused.status, DownloadStatus::Paused);
        assert!(
            paused.downloaded_bytes > 0,
            "pausing must keep what was already transferred"
        );
        assert!(paused.downloaded_bytes < DEFAULT_BODY.len() as u64);

        let partial = paused.temp_path.clone().unwrap();
        assert!(tokio::fs::try_exists(&partial).await.unwrap());

        let resumed = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(resumed.id, created.id);
        assert_eq!(resumed.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(resumed.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY,
            "a resumed download must produce exactly the original file"
        );
        assert!(
            server.ranged_request_count() >= 2,
            "resuming must ask the server for a range"
        );
    }

    #[tokio::test]
    async fn cancelling_a_running_transfer_discards_its_partial_file() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("slow.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;

        let partial = harness
            .storage
            .get_download(&created.id)
            .unwrap()
            .unwrap()
            .temp_path
            .unwrap();

        harness.service.cancel_task(&created.id).await.unwrap();

        let cancelled = transfer.await.unwrap().unwrap();

        assert_eq!(cancelled.status, DownloadStatus::Cancelled);
        assert_eq!(cancelled.downloaded_bytes, 0);
        assert!(
            !tokio::fs::try_exists(&partial).await.unwrap(),
            "cancelling is terminal, so the partial file goes with it"
        );
    }

    #[tokio::test]
    async fn a_task_that_is_not_running_cannot_be_paused() {
        let harness = harness();

        let created = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();

        let error = harness.service.pause_task(&created.id).unwrap_err();

        assert!(matches!(error, DownloadServiceError::NotRunning(_)));
    }

    #[tokio::test]
    async fn a_source_that_changed_is_downloaded_again_instead_of_appended_to() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("changing.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();
        transfer.await.unwrap().unwrap();

        // The file behind the URL is replaced while the task is paused.
        let replacement = b"a completely different payload of its own".to_vec();
        server.update(|behaviour| {
            behaviour.body = replacement.clone();
            behaviour.etag = Some("\"v2\"".to_owned());
            behaviour.chunk_delay = None;
        });

        let resumed = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(resumed.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(resumed.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            b"a completely different payload of its own",
            "stale bytes must never be spliced into the new content"
        );
        assert_eq!(
            resumed.error_code.as_deref(),
            None,
            "a finished download carries no leftover notice"
        );
    }

    #[tokio::test]
    async fn an_interrupted_task_resumes_from_its_partial_file_after_a_restart() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();

        let created = harness
            .service
            .create_task(&server.url("interrupted.bin"))
            .unwrap();

        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();
        transfer.await.unwrap().unwrap();

        // Stand in for a process that died while downloading: the row is left
        // in an executing state with its partial file on disk.
        harness.storage.mark_probing(&created.id).unwrap();

        let recovered = harness.service.recover_orphaned_tasks().unwrap();

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].status, DownloadStatus::Paused);
        assert!(recovered[0].downloaded_bytes > 0);

        let finished = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
    }
    const SESSION_COOKIE: &str = "session=s3cr3t-browser-login";

    /// A server that, like a login-protected download, refuses every request
    /// that does not carry the browser's session.
    fn login_protected_server() -> ServerBehaviour {
        ServerBehaviour {
            required_header: Some(format!("cookie: {SESSION_COOKIE}")),
            ..ServerBehaviour::default()
        }
    }

    #[tokio::test]
    async fn a_login_protected_download_needs_the_browser_session() {
        let server = TestServer::start(login_protected_server()).await;
        let harness = harness();

        let without = harness
            .service
            .create_task(&server.url("private.bin"))
            .unwrap();
        let _ = harness
            .service
            .start_task(&without.id, &harness.destination)
            .await;

        assert_eq!(
            harness
                .storage
                .get_download(&without.id)
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Failed,
            "the server must really refuse a request without the session"
        );

        let with = harness
            .service
            .create_task(&server.url("private.bin"))
            .unwrap();
        harness
            .service
            .attach_browser_session(&with.id, SESSION_COOKIE)
            .unwrap();

        let finished = harness
            .service
            .start_task(&with.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
    }

    #[tokio::test]
    async fn a_browser_session_never_follows_a_redirect_to_another_origin() {
        // The file itself lives on a different origin, the way a download
        // page hands off to a CDN.
        let cdn = TestServer::start(ServerBehaviour::default()).await;
        let origin = TestServer::start(ServerBehaviour {
            redirect_to: Some(cdn.url("file.bin")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let task = harness
            .service
            .create_task(&origin.url("download"))
            .unwrap();
        harness
            .service
            .attach_browser_session(&task.id, SESSION_COOKIE)
            .unwrap();

        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        assert!(
            origin.requests_with_cookie() > 0,
            "the origin the session belongs to receives it"
        );
        assert_eq!(
            cdn.requests_with_cookie(),
            0,
            "a session must never reach a host it was not captured for"
        );
        assert!(cdn.request_count() > 0, "the CDN really served the file");
    }

    /// The segmented engine does not follow the redirect on every request: it
    /// sends its ranged requests straight to the resolved CDN address. Only
    /// the engine's own origin check keeps the session off those requests,
    /// so this is the test that fails if that check is ever removed.
    #[tokio::test]
    async fn segmented_requests_to_a_resolved_cdn_never_carry_the_session() {
        let cdn = TestServer::start(ServerBehaviour::default()).await;
        let origin = TestServer::start(ServerBehaviour {
            redirect_to: Some(cdn.url("file.bin")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(2)
            .with_segmented_threshold(1);

        let task = service.create_task(&origin.url("download")).unwrap();
        service
            .attach_browser_session(&task.id, SESSION_COOKIE)
            .unwrap();

        let finished = service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        assert!(
            cdn.ranged_request_count() > 1,
            "the segmented engine really requested ranges from the CDN"
        );
        assert_eq!(
            cdn.requests_with_cookie(),
            0,
            "a session must never reach a host it was not captured for"
        );
    }

    #[tokio::test]
    async fn a_browser_session_is_never_written_to_disk() {
        let server = TestServer::start(login_protected_server()).await;
        let harness = harness();

        let task = harness
            .service
            .create_task(&server.url("private.bin"))
            .unwrap();
        harness
            .service
            .attach_browser_session(&task.id, SESSION_COOKIE)
            .unwrap();
        harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        // The database and its write-ahead log are checked byte by byte, so
        // no future column or table can quietly start holding the session.
        let mut checked = 0;
        for entry in std::fs::read_dir(harness._root.path()).unwrap() {
            let path = entry.unwrap().path();
            let is_database = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("downloads.db"));

            if is_database && path.is_file() {
                let bytes = std::fs::read(&path).unwrap();
                assert!(
                    !bytes
                        .windows(SESSION_COOKIE.len())
                        .any(|window| window == SESSION_COOKIE.as_bytes()),
                    "{} holds the browser session",
                    path.display()
                );
                checked += 1;
            }
        }

        assert!(checked > 0, "the database files were not found");
    }

    #[tokio::test]
    async fn a_session_is_accepted_only_before_the_download_starts() {
        let harness = harness();

        let task = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();
        harness.service.claim_task(&task.id).unwrap();

        let error = harness
            .service
            .attach_browser_session(&task.id, SESSION_COOKIE)
            .unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::SessionNotAccepted(DownloadStatus::Probing)
        ));
        assert!(!harness.service.has_browser_session(&task.id));
    }

    #[tokio::test]
    async fn a_hostile_session_is_refused_without_being_echoed() {
        let harness = harness();

        let task = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();

        let error = harness
            .service
            .attach_browser_session(&task.id, "session=s3cr3t\r\nX-Injected: 1")
            .unwrap_err();

        assert!(matches!(
            error,
            DownloadServiceError::BrowserSession(crate::session::SessionError::InvalidCharacters)
        ));
        assert!(!error.to_string().contains("s3cr3t"));
        assert!(!harness.service.has_browser_session(&task.id));
    }

    #[tokio::test]
    async fn a_session_is_forgotten_once_the_download_finishes() {
        let server = TestServer::start(login_protected_server()).await;
        let harness = harness();

        let task = harness
            .service
            .create_task(&server.url("private.bin"))
            .unwrap();
        harness
            .service
            .attach_browser_session(&task.id, SESSION_COOKIE)
            .unwrap();
        assert!(harness.service.has_browser_session(&task.id));

        harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert!(!harness.service.has_browser_session(&task.id));
    }

    #[tokio::test]
    async fn cancelling_forgets_the_session() {
        let harness = harness();

        let task = harness
            .service
            .create_task("https://example.com/file.bin")
            .unwrap();
        harness
            .service
            .attach_browser_session(&task.id, SESSION_COOKIE)
            .unwrap();

        harness.service.cancel_task(&task.id).await.unwrap();

        assert!(!harness.service.has_browser_session(&task.id));
    }

    fn slow_body_server() -> ServerBehaviour {
        ServerBehaviour {
            body: (0..=255_u8).cycle().take(4_000).collect(),
            chunk_size: 50,
            chunk_delay: Some(Duration::from_millis(4)),
            ..ServerBehaviour::default()
        }
    }

    #[tokio::test]
    async fn an_idle_connection_takes_over_half_of_a_running_range() {
        let behaviour = slow_body_server();
        let body = behaviour.body.clone();
        let server = TestServer::start(behaviour).await;
        let harness = harness();
        // One planned range only: every further connection must come from
        // splitting the range that is already running.
        let service = harness
            .service
            .clone()
            .with_segment_connections(4)
            .with_segmented_threshold(1)
            .with_segment_sizes(1_000_000, 200)
            .with_evaluation_window(Duration::from_millis(30));
        let created = service.create_task(&server.url("split.bin")).unwrap();

        let record = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(record.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            body
        );
        assert!(
            server.peak_concurrent_bodies() >= 2,
            "a split range was downloaded alongside the original"
        );
        assert!(
            harness
                .storage
                .list_download_segments(&created.id)
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn every_range_is_written_into_one_partial_file() {
        let behaviour = slow_body_server();
        let server = TestServer::start(behaviour).await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(3)
            .with_segmented_threshold(1)
            .with_segment_sizes(1, 200)
            .with_checkpoints(100, Duration::from_millis(5));
        let created = service.create_task(&server.url("shared.bin")).unwrap();
        let service_for_task = service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer =
            tokio::spawn(async move { service_for_task.start_task(&task_id, destination).await });

        await_segment_bytes(&harness.storage, &created.id).await;
        let segments = harness.storage.list_download_segments(&created.id).unwrap();
        let task = harness.storage.get_download(&created.id).unwrap().unwrap();
        let temp = task.temp_path.clone().unwrap();
        assert!(segments.iter().all(|segment| segment.temp_path == temp));
        assert_eq!(
            tokio::fs::metadata(&temp).await.unwrap().len(),
            4_000,
            "the partial file is preallocated at full size"
        );

        let record = transfer.await.unwrap().unwrap();
        assert_eq!(record.status, DownloadStatus::Completed);
        assert!(!tokio::fs::try_exists(&temp).await.unwrap());
    }

    #[tokio::test]
    async fn ranges_kept_in_separate_files_by_older_versions_are_replaced() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_segment_connections(2)
            .with_segmented_threshold(1)
            .with_segment_sizes(1, 1);
        let created = service.create_task(&server.url("legacy.bin")).unwrap();

        // What an older version left behind: a paused task with its ranges in
        // their own files.
        tokio::fs::create_dir_all(&harness.destination)
            .await
            .unwrap();
        let old_file = harness
            .destination
            .join("legacy.bin.part.segment-0000.part");
        tokio::fs::write(&old_file, &DEFAULT_BODY[..10])
            .await
            .unwrap();
        harness
            .storage
            .replace_download_segments(
                &created.id,
                &[DownloadSegment {
                    download_id: created.id.clone(),
                    segment_index: 0,
                    start_byte: 0,
                    end_byte: DEFAULT_BODY.len() as u64 - 1,
                    downloaded_bytes: 10,
                    temp_path: old_file.to_string_lossy().into_owned(),
                    status: SegmentStatus::Pending,
                }],
            )
            .unwrap();

        let record = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(record.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
        assert!(!tokio::fs::try_exists(&old_file).await.unwrap());
    }

    #[tokio::test]
    async fn downloaded_bytes_are_counted_as_domestic_or_international() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();

        let foreign = harness.service.create_task(&server.url("a.bin")).unwrap();
        harness
            .service
            .start_task(&foreign.id, &harness.destination)
            .await
            .unwrap();
        let summary = harness.service.traffic_summary().unwrap();
        assert_eq!(summary.today_international_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(summary.today_domestic_bytes, 0);

        // Listing the host as domestic moves later traffic to that side.
        let mut network = harness.service.network_settings();
        network.domestic_hosts = vec!["127.0.0.1".to_owned()];
        harness.service.set_network_settings(&network).unwrap();
        let local = harness.service.create_task(&server.url("b.bin")).unwrap();
        harness
            .service
            .start_task(&local.id, &harness.destination)
            .await
            .unwrap();
        let summary = harness.service.traffic_summary().unwrap();
        assert_eq!(summary.today_domestic_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(
            summary.period_international_bytes,
            DEFAULT_BODY.len() as u64
        );
    }

    #[tokio::test]
    async fn a_used_up_international_quota_pauses_new_downloads_with_a_reason() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let today = harness.service.today().unwrap();
        harness
            .storage
            .record_traffic(&today, TrafficScope::International, 500)
            .unwrap();
        harness.service.set_traffic_quota(Some(400), None).unwrap();

        let task = harness.service.create_task(&server.url("big.bin")).unwrap();
        let record = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Paused);
        assert_eq!(record.error_code.as_deref(), Some(QUOTA_NOTICE_CODE));
        assert_eq!(
            server.ranged_request_count(),
            1,
            "only the probe reached the server"
        );

        // Domestic downloads are not held back by the international quota.
        let mut network = harness.service.network_settings();
        network.domestic_hosts = vec!["127.0.0.1".to_owned()];
        harness.service.set_network_settings(&network).unwrap();
        let domestic = harness.service.create_task(&server.url("ir.bin")).unwrap();
        let record = harness
            .service
            .start_task(&domestic.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(record.status, DownloadStatus::Completed);
    }

    #[test]
    fn quota_settings_are_validated_and_stored() {
        let harness = harness();
        assert!(
            harness
                .service
                .set_traffic_quota(Some(10), Some("next monday"))
                .is_err()
        );
        harness
            .service
            .set_traffic_quota(Some(10 * 1024), Some("2026-09-01"))
            .unwrap();
        let summary = harness.service.traffic_summary().unwrap();
        assert_eq!(summary.international_quota, Some(10 * 1024));
        assert!(summary.explicit_period);

        harness.service.set_traffic_quota(None, None).unwrap();
        let summary = harness.service.traffic_summary().unwrap();
        assert_eq!(summary.international_quota, None);
        assert!(!summary.explicit_period);
    }

    #[test]
    fn connection_limit_is_stored_and_clamped() {
        let harness = harness();
        assert_eq!(
            harness.service.max_connections(),
            DEFAULT_SEGMENT_CONNECTIONS
        );
        harness.service.set_max_connections(500).unwrap();
        assert_eq!(harness.service.max_connections(), MAX_SEGMENT_CONNECTIONS);
        harness.service.set_max_connections(0).unwrap();
        assert_eq!(harness.service.max_connections(), 1);
    }

    #[test]
    fn a_proxy_with_credentials_is_refused_and_nothing_is_stored() {
        let harness = harness();
        let mut network = harness.service.network_settings();
        network.mode = crate::network::ProxyMode::Manual;
        network.proxy_url = Some("socks5://me:secret@127.0.0.1:1080".to_owned());

        assert!(harness.service.set_network_settings(&network).is_err());
        assert_eq!(
            harness
                .storage
                .get_setting(crate::network::SETTING_PROXY_URL)
                .unwrap(),
            None
        );
    }
}
