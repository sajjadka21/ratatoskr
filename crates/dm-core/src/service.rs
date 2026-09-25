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
/// Lost connections in a row, with no progress between them, before the
/// whole download stops and waits to retry.
const MAX_RANGE_RETRIES: u32 = 5;
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

/// Recorded on a task whose link stopped working after it had worked, or
/// whose signed link was refused.
pub const LINK_EXPIRED_CODE: &str = "link_expired";
const LINK_EXPIRED_MESSAGE: &str = "The link has expired. Start the download again from the page it came from, or paste a fresh link: what was downloaded is kept.";
/// Recorded on a task that continued with a fresh link found for it.
pub const LINK_ADOPTED_CODE: &str = "link_refreshed";
const LINK_ADOPTED_MESSAGE: &str = "Continuing with a fresh link for the same file.";
/// Whether a new link for a file whose old link expired is used for that
/// download instead of starting a second one.
pub const SETTING_AUTO_ADOPT_LINKS: &str = "auto_adopt_links";

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

    #[error("that is not a checksum: paste the MD5, SHA-1 or SHA-256 value")]
    InvalidChecksum,

    #[error("the command is empty or has an unclosed quote")]
    InvalidCommand,

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
/// Hosts treated gently: at most `POLITE_CONNECTIONS` connections, and no
/// splitting of running ranges. One domain per line.
pub const SETTING_POLITE_HOSTS: &str = "polite_hosts";
const POLITE_CONNECTIONS: usize = 2;
/// Highest stream quality chosen automatically (a height such as 720).
pub const SETTING_STREAM_MAX_HEIGHT: &str = "stream_max_height";
/// Stream segments fetched at the same time.
const STREAM_CONNECTIONS: usize = 4;
/// Where FFmpeg is; empty to look for it automatically.
pub const SETTING_FFMPEG_PATH: &str = "ffmpeg_path";
/// Where yt-dlp is, when the user chose it rather than letting the app look.
pub const SETTING_YTDLP_PATH: &str = "ytdlp_path";
/// The shape an exported download list takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Links,
}

/// What a test request found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConnectionCheck {
    pub host: String,
    /// `direct`, `proxy` or `system`.
    pub route: String,
    pub reachable: bool,
    pub elapsed_ms: u64,
    /// The host a redirect ended on, when it differs.
    pub final_host: Option<String>,
    pub filename: Option<String>,
    pub total_bytes: Option<u64>,
    pub range_supported: bool,
    /// Why it failed, with any link reduced to its host.
    pub error: Option<String>,
}

/// After-download steps, all off by default.
pub const SETTING_POST_HASH: &str = "post_hash_always";
pub const SETTING_POST_EXTRACT: &str = "post_extract_zip";
pub const SETTING_POST_SCAN: &str = "post_defender_scan";
pub const SETTING_POST_COMMAND: &str = "post_command";
pub const INTEGRITY_FAILED_CODE: &str = "integrity_failed";
pub const THREAT_FOUND_CODE: &str = "threat_found";

/// Which after-download steps run for every finished download.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PostProcessSettings {
    pub hash_always: bool,
    pub extract_zip: bool,
    pub scan: bool,
    /// A command such as `"C:\Tools\check.exe" {file}`; `None` for none.
    pub command: Option<String>,
}

/// Rewrap transport streams (`.ts`) as MP4 when FFmpeg is available.
pub const SETTING_STREAM_PREFER_MP4: &str = "stream_prefer_mp4";

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
    /// How long a connection may deliver nothing before it is dropped.
    stall_timeout: Duration,
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
    /// Limits the user set on single downloads, by task id. Kept while the
    /// app runs so a change reaches a running transfer at once.
    task_limiters: Arc<Mutex<HashMap<String, Arc<RateLimiter>>>>,
    /// Browser sessions handed over for tasks, by task id. Memory only: a
    /// session is never written to storage and is forgotten on restart.
    sessions: Arc<Mutex<HashMap<String, Arc<BrowserSession>>>>,
    /// Ids of downloads whose after-download steps just changed.
    post_events: tokio::sync::broadcast::Sender<String>,
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
            task_limiters: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            post_events: tokio::sync::broadcast::channel(64).0,
            stall_timeout: crate::network::DEFAULT_STALL_TIMEOUT,
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

    /// The limit set on one download, in bytes per second, if any.
    pub fn task_speed_limit(&self, download_id: &str) -> Result<Option<u64>> {
        Ok(self.storage.get_speed_limit(download_id)?)
    }

    /// Persists and applies one download's own limit; `None` removes it. A
    /// running transfer follows the change immediately. The global limit
    /// still applies on top.
    pub fn set_task_speed_limit(&self, download_id: &str, limit: Option<u64>) -> Result<()> {
        let limit = limit.filter(|value| *value > 0);
        self.storage.set_speed_limit(download_id, limit)?;
        if let Ok(limiters) = self.task_limiters.lock()
            && let Some(limiter) = limiters.get(download_id)
        {
            limiter.set_limit(limit);
        }
        Ok(())
    }

    /// The limiter a transfer of this download draws from, created the first
    /// time it is needed.
    fn task_limiter(&self, download_id: &str) -> Result<Arc<RateLimiter>> {
        let limit = self.storage.get_speed_limit(download_id)?;
        let mut limiters = self
            .task_limiters
            .lock()
            .map_err(|_| DownloadServiceError::ControlRegistryUnavailable)?;
        let limiter = limiters
            .entry(download_id.to_owned())
            .or_insert_with(|| Arc::new(RateLimiter::new(limit)));
        if limiter.limit() != limit {
            limiter.set_limit(limit);
        }
        Ok(Arc::clone(limiter))
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

    /// Drops a connection that delivers nothing for `timeout` (30 seconds
    /// unless changed), so its range goes to another connection.
    pub fn with_stall_timeout(mut self, timeout: Duration) -> Result<Self> {
        self.stall_timeout = timeout;
        let downloader =
            Downloader::with_network_and_stall(&NetworkSettings::load(&self.storage), timeout)?;
        self.downloader = Arc::new(RwLock::new(downloader));
        Ok(self)
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
        let downloader = Downloader::with_network_and_stall(settings, self.stall_timeout)?;
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

    /// Other addresses of a download's file.
    pub fn mirrors(&self, download_id: &str) -> Result<Vec<String>> {
        Ok(self.storage.list_mirrors(download_id)?)
    }

    /// Replaces a download's mirrors. Each must be an http(s) address; they
    /// are checked against the file itself when the download next runs.
    pub fn set_mirrors(&self, download_id: &str, urls: &[String]) -> Result<Vec<String>> {
        for url in urls.iter().filter(|url| !url.trim().is_empty()) {
            validate_source_url(url.trim())?;
        }
        Ok(self.storage.set_mirrors(download_id, urls)?)
    }

    /// Domains that get gentle treatment, one per line.
    pub fn polite_hosts(&self) -> Vec<String> {
        self.storage
            .get_setting(SETTING_POLITE_HOSTS)
            .ok()
            .flatten()
            .map(|value| crate::traffic::parse_host_list(&value))
            .unwrap_or_default()
    }

    pub fn set_polite_hosts(&self, hosts: &[String]) -> Result<()> {
        self.storage
            .set_setting(SETTING_POLITE_HOSTS, &hosts.join("\n"))?;
        Ok(())
    }

    fn is_polite_host(&self, host: &str) -> bool {
        self.polite_hosts()
            .iter()
            .any(|suffix| host == suffix || host.ends_with(&format!(".{suffix}")))
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
            .with_limiter(Arc::clone(&self.global_limiter))
            .with_limiter(self.task_limiter(download_id)?);

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

        self.storage.adopt_source_url(download_id, source_url)?;
        self.get_task(download_id)
    }

    pub fn auto_adopt_links(&self) -> bool {
        self.storage
            .get_setting(SETTING_AUTO_ADOPT_LINKS)
            .ok()
            .flatten()
            .as_deref()
            != Some("false")
    }

    pub fn set_auto_adopt_links(&self, enabled: bool) -> Result<()> {
        self.storage.set_setting(
            SETTING_AUTO_ADOPT_LINKS,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    /// Downloads a fresh link could continue: stopped part-way, not running,
    /// with a known name and size to compare against.
    fn adoption_candidates(&self, except: &str) -> Result<Vec<DownloadRecord>> {
        use dm_common::DownloadStatus as Status;
        Ok(self
            .storage
            .list_downloads()?
            .into_iter()
            .filter(|task| {
                task.id != except
                    && matches!(
                        task.status,
                        Status::Failed | Status::Paused | Status::Cancelled
                    )
                    && task.downloaded_bytes > 0
                    && task.total_bytes.is_some()
                    && task.filename.is_some()
            })
            .filter(|task| !matches!(self.control_for(&task.id), Ok(Some(_))))
            .collect())
    }

    /// Cheap check before probing: is there anything a new link could
    /// continue?
    pub fn may_adopt(&self, new_task_id: &str) -> bool {
        self.auto_adopt_links()
            && self
                .adoption_candidates(new_task_id)
                .is_ok_and(|candidates| !candidates.is_empty())
    }

    /// When a newly added link is a fresh link for a download that stopped
    /// part-way (typically because its link expired), moves the link to that
    /// download and removes the new task, so the file continues instead of
    /// starting over in a second copy.
    ///
    /// The new link is probed, and a download matches only when the server
    /// reports the same file name and size, and the same validator when
    /// both sides have one. Exactly one match is required. Returns the
    /// download that now carries the link.
    pub async fn adopt_fresh_link(&self, new_task_id: &str) -> Result<Option<DownloadRecord>> {
        if !self.auto_adopt_links() {
            return Ok(None);
        }
        let candidates = self.adoption_candidates(new_task_id)?;
        if candidates.is_empty() {
            return Ok(None);
        }
        let fresh = self.get_task(new_task_id)?;
        if fresh.status != dm_common::DownloadStatus::Created || fresh.downloaded_bytes > 0 {
            return Ok(None);
        }
        let Ok(probe) = self
            .downloader_for(new_task_id)?
            .probe(&fresh.source_url)
            .await
        else {
            return Ok(None);
        };
        let matches = candidates
            .into_iter()
            .filter(|task| {
                task.filename.as_deref() == Some(probe.filename.as_str())
                    && task.total_bytes == probe.total_bytes
                    && match (&task.etag, &probe.etag) {
                        (Some(stored), Some(now)) => stored == now,
                        _ => true,
                    }
            })
            .collect::<Vec<_>>();
        let [target] = matches.as_slice() else {
            return Ok(None);
        };

        let context = self.storage.get_request_context(new_task_id)?;
        self.storage
            .adopt_source_url(&target.id, &fresh.source_url)?;
        self.storage.set_request_context(&target.id, &context)?;
        if let Some(session) = self.browser_session_for(new_task_id)
            && let Ok(mut sessions) = self.sessions.lock()
        {
            sessions.remove(new_task_id);
            sessions.insert(target.id.clone(), session);
        }
        self.storage
            .record_notice(&target.id, LINK_ADOPTED_CODE, LINK_ADOPTED_MESSAGE)?;
        self.storage.remove_download_record(new_task_id)?;
        Ok(Some(self.get_task(&target.id)?))
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
                let record = self.complete_download(download_id, outcome)?;
                // Checks after the download run in the background, so the
                // download is reported finished as soon as it is.
                if self.post_process_wanted(download_id) {
                    let service = self.clone();
                    let id = download_id.to_owned();
                    tokio::spawn(async move {
                        let _ = service.post_process(&id).await;
                    });
                }
                Ok(record)
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
        // A video page (YouTube and the like) is not a file; yt-dlp reads it.
        if crate::ytdlp::handles(&task.source_url) {
            return self
                .run_ytdlp_transfer(task, destination_directory, control, on_progress)
                .await;
        }

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

        if let Some(kind @ (crate::media::MediaKind::Hls | crate::media::MediaKind::Dash)) =
            crate::media::classify_source(&probe.final_url, probe.content_type.as_deref())
        {
            return self
                .run_stream_transfer(
                    task,
                    &probe,
                    kind,
                    decision.as_ref(),
                    destination_directory,
                    control,
                    on_progress,
                )
                .await;
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

    /// Downloads an unprotected HLS or DASH stream.
    ///
    /// The quality is the best at or below the automatic limit, or the one
    /// the link names with `#rud-quality=<height>`. Each track (picture, and
    /// sound when it is separate) is fetched a few parts at a time and
    /// appended in order into its own file, decrypted when it uses plain
    /// AES-128. Separate tracks are joined, and transport streams rewrapped
    /// as MP4 when asked, by FFmpeg, copying the streams without re-encoding.
    ///
    /// Progress survives a pause or a restart: a small record next to each
    /// track file says how many parts are in it, and the file is cut back to
    /// that point before continuing.
    #[allow(clippy::too_many_arguments)]
    async fn run_stream_transfer<F>(
        &self,
        task: &DownloadRecord,
        probe: &crate::SourceProbe,
        kind: crate::media::MediaKind,
        decision: Option<&RuleDecision>,
        destination_directory: &Path,
        control: Arc<TaskControl>,
        on_progress: &mut F,
    ) -> Result<crate::TransferOutcome>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        let downloader = self.downloader_for(&task.id)?;
        let ffmpeg = self.ffmpeg();
        let max_height = quality_from_link(&task.source_url).or_else(|| self.stream_max_height());
        let plan = resolve_stream_plan(
            &downloader,
            &probe.final_url,
            kind,
            max_height,
            ffmpeg.is_some(),
            control.as_ref(),
        )
        .await?;

        let joining = plan.tracks.len() > 1;
        let rewrapping =
            !joining && plan.extension == "ts" && ffmpeg.is_some() && self.stream_prefer_mp4();
        let extension = if joining || rewrapping {
            "mp4"
        } else {
            plan.extension
        };

        let (destination_path, temp_path) = match (&task.destination_path, &task.temp_path) {
            (Some(destination), Some(temp)) => (PathBuf::from(destination), PathBuf::from(temp)),
            _ => {
                let directory = self.resolve_destination(decision, destination_directory);
                let filename = crate::hls::stream_filename(&probe.final_url, extension);
                self.base_downloader()
                    .plan_paths(&directory, &filename)
                    .await?
            }
        };
        let filename = destination_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| crate::hls::stream_filename(&probe.final_url, extension));
        self.storage.set_transfer_plan(
            &task.id,
            &TransferPlan {
                resolved_url: plan.tracks[0].identity.clone(),
                filename,
                destination_path: destination_path.to_string_lossy().into_owned(),
                temp_path: temp_path.to_string_lossy().into_owned(),
                mime_type: Some(
                    if extension == "mp4" {
                        "video/mp4"
                    } else {
                        "video/mp2t"
                    }
                    .to_owned(),
                ),
                total_bytes: None,
                etag: None,
                last_modified: None,
                range_supported: false,
            },
        )?;
        let scope = self.traffic_scope_of(&plan.tracks[0].identity);
        self.ensure_quota_allows(scope)?;
        self.storage
            .mark_downloading(&task.id, unix_timestamp_seconds()?)?;

        // With FFmpeg afterwards, each track goes to its own file; otherwise
        // the only track is the partial file itself.
        let track_files: Vec<PathBuf> = if joining || rewrapping {
            (0..plan.tracks.len())
                .map(|index| {
                    let mut name = temp_path.as_os_str().to_owned();
                    name.push(format!(".track{index}"));
                    PathBuf::from(name)
                })
                .collect()
        } else {
            vec![temp_path.clone()]
        };

        let mut progress = StreamProgress {
            parts_total: plan.tracks.iter().map(|track| track.parts.len()).sum(),
            ..StreamProgress::default()
        };
        progress.meter = Some(ThroughputMeter::new(Instant::now(), 0));
        for (track, file) in plan.tracks.iter().zip(&track_files) {
            self.download_stream_track(
                task,
                &downloader,
                track,
                file,
                scope,
                &control,
                &mut progress,
                on_progress,
            )
            .await?;
        }

        if joining || rewrapping {
            let ffmpeg = ffmpeg.ok_or(DownloadError::Stream(crate::hls::HlsError::NeedsMuxing))?;
            on_progress(
                &task.id,
                TransferProgress {
                    downloaded_bytes: progress.written,
                    total_bytes: Some(progress.written),
                    bytes_per_second: None,
                    eta_seconds: None,
                    active_connections: Some(1),
                    max_connections: Some(1),
                    adaptive_reason: Some(if joining {
                        "joining picture and sound"
                    } else {
                        "rewrapping as mp4"
                    }),
                },
            );
            let result = if joining {
                ffmpeg
                    .join(
                        &track_files[0],
                        &track_files[1],
                        &temp_path,
                        control.as_ref(),
                    )
                    .await
            } else {
                ffmpeg
                    .remux(&track_files[0], &temp_path, control.as_ref())
                    .await
            };
            match result {
                Ok(()) => {}
                Err(crate::ffmpeg::FfmpegError::Stopped) => {
                    return Err(DownloadError::Stopped(
                        control.stop_reason().unwrap_or(StopReason::Pause),
                    )
                    .into());
                }
                Err(error) => return Err(DownloadError::Ffmpeg(error.to_string()).into()),
            }
            for file in &track_files {
                let _ = tokio::fs::remove_file(file).await;
                let _ = tokio::fs::remove_file(stream_record_path(file)).await;
            }
        } else {
            let _ = tokio::fs::remove_file(stream_record_path(&temp_path)).await;
        }

        let size = partial_file_size(&temp_path).await.unwrap_or(0);
        self.storage.update_progress(&task.id, size, Some(size))?;
        let outcome = self
            .base_downloader()
            .finalize_shared_file(&temp_path, &destination_path, size)
            .await?;
        Ok(outcome)
    }

    /// Fetches one track's parts into `file`, continuing from its record.
    #[allow(clippy::too_many_arguments)]
    async fn download_stream_track<F>(
        &self,
        task: &DownloadRecord,
        downloader: &Downloader,
        track: &StreamTrack,
        file_path: &Path,
        scope: TrafficScope,
        control: &Arc<TaskControl>,
        progress: &mut StreamProgress,
        on_progress: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};

        let record_path = stream_record_path(file_path);
        let total_parts = track.parts.len();
        let (mut done_parts, mut written) = match read_stream_record(&record_path).await {
            Some(record)
                if record.media_url == track.identity
                    && record.parts == total_parts
                    && partial_file_size(file_path)
                        .await
                        .is_some_and(|size| size >= record.bytes) =>
            {
                (record.done, record.bytes)
            }
            _ => (0, 0),
        };
        progress.written += written;
        progress.parts_done += done_parts;
        progress.parts_skipped += done_parts;
        progress.bytes_skipped += written;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(file_path)
            .await
            .map_err(DownloadError::Io)?;
        file.set_len(written).await.map_err(DownloadError::Io)?;
        file.seek(std::io::SeekFrom::Start(written))
            .await
            .map_err(DownloadError::Io)?;
        self.storage
            .update_progress(&task.id, progress.written, None)?;

        let mut keys: HashMap<String, Arc<Vec<u8>>> = HashMap::new();
        let mut in_flight: JoinSet<std::result::Result<(usize, Vec<u8>), DownloadError>> =
            JoinSet::new();
        let mut ready: std::collections::BTreeMap<usize, Vec<u8>> =
            std::collections::BTreeMap::new();
        let mut next_to_fetch = done_parts;
        let mut last_counted = progress.written;

        while done_parts < total_parts {
            while in_flight.len() < STREAM_CONNECTIONS
                && next_to_fetch < total_parts
                && ready.len() < STREAM_CONNECTIONS * 2
            {
                let (uri, range, key, sequence) = track.parts[next_to_fetch].clone();
                let key_bytes = match &key {
                    Some(key) => {
                        if !keys.contains_key(&key.uri) {
                            let bytes = downloader
                                .fetch_bytes(&key.uri, None, 64, control.as_ref())
                                .await?;
                            keys.insert(key.uri.clone(), Arc::new(bytes));
                        }
                        keys.get(&key.uri).cloned()
                    }
                    None => None,
                };
                let downloader = downloader.clone();
                let control = Arc::clone(control);
                let index = next_to_fetch;
                in_flight.spawn(async move {
                    let mut data = downloader
                        .fetch_bytes(&uri, range, crate::hls::MAX_SEGMENT_BYTES, control.as_ref())
                        .await?;
                    if let (Some(key), Some(key_bytes)) = (key, key_bytes) {
                        crate::hls::decrypt_segment(&mut data, &key_bytes, key.iv, sequence)?;
                    }
                    Ok((index, data))
                });
                next_to_fetch += 1;
            }

            let Some(joined) = in_flight.join_next().await else {
                break;
            };
            let (index, data) = match joined {
                Ok(Ok(part)) => part,
                Ok(Err(error)) => {
                    in_flight.abort_all();
                    file.flush().await.map_err(DownloadError::Io)?;
                    file.sync_data().await.map_err(DownloadError::Io)?;
                    self.count_traffic(scope, progress.written.saturating_sub(last_counted));
                    return Err(error.into());
                }
                Err(_) => return Err(DownloadServiceError::ExecutionUnavailable),
            };
            ready.insert(index, data);

            // Append whatever is now next in line.
            let mut appended = false;
            while let Some(data) = ready.remove(&done_parts) {
                file.write_all(&data).await.map_err(DownloadError::Io)?;
                written += data.len() as u64;
                progress.written += data.len() as u64;
                done_parts += 1;
                progress.parts_done += 1;
                appended = true;
            }
            if !appended {
                continue;
            }

            // Parts take seconds each, so recording after every one is cheap
            // and loses nothing on a pause or a crash.
            file.flush().await.map_err(DownloadError::Io)?;
            file.sync_data().await.map_err(DownloadError::Io)?;
            write_stream_record(
                &record_path,
                &StreamRecord {
                    media_url: track.identity.clone(),
                    parts: total_parts,
                    done: done_parts,
                    bytes: written,
                },
            )
            .await;
            self.storage
                .update_progress(&task.id, progress.written, None)?;
            self.count_traffic(scope, progress.written.saturating_sub(last_counted));
            last_counted = progress.written;

            let now = Instant::now();
            let estimate = progress.estimated_total();
            let meter = progress
                .meter
                .get_or_insert_with(|| ThroughputMeter::new(now, 0));
            let bytes_per_second = meter.sample(progress.written, now);
            let eta_seconds = meter.eta_seconds(progress.written, estimate);
            on_progress(
                &task.id,
                TransferProgress {
                    downloaded_bytes: progress.written,
                    total_bytes: estimate,
                    bytes_per_second,
                    eta_seconds,
                    active_connections: Some(in_flight.len().max(1) as u32),
                    max_connections: Some(STREAM_CONNECTIONS as u32),
                    adaptive_reason: Some("stream segments"),
                },
            );
        }

        file.flush().await.map_err(DownloadError::Io)?;
        file.sync_all().await.map_err(DownloadError::Io)?;
        self.count_traffic(scope, progress.written.saturating_sub(last_counted));
        Ok(())
    }

    /// yt-dlp, from Settings or found next to the application or on `PATH`.
    pub fn ytdlp(&self) -> Option<crate::ytdlp::YtDlp> {
        crate::ytdlp::YtDlp::locate(self.ytdlp_path_setting().map(PathBuf::from).as_deref())
    }

    pub fn ytdlp_path_setting(&self) -> Option<String> {
        self.storage
            .get_setting(SETTING_YTDLP_PATH)
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
    }

    /// Where yt-dlp is; `None` to look for it automatically. A path that
    /// does not name a file is refused.
    pub fn set_ytdlp_path(&self, path: Option<&Path>) -> Result<()> {
        if let Some(path) = path
            && (!path.is_absolute() || !path.is_file())
        {
            return Err(DownloadServiceError::RelativeDirectory);
        }
        self.storage.set_setting(
            SETTING_YTDLP_PATH,
            &path
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )?;
        Ok(())
    }

    /// Downloads a video page with yt-dlp into a private folder next to the
    /// destination, then moves the finished file out of it.
    async fn run_ytdlp_transfer<F>(
        &self,
        task: &DownloadRecord,
        destination_directory: &Path,
        control: Arc<TaskControl>,
        on_progress: &mut F,
    ) -> Result<crate::TransferOutcome>
    where
        F: FnMut(&str, TransferProgress) + Send,
    {
        use crate::ytdlp::{ProxyChoice, YtDlpError, YtDlpRequest};

        let ytdlp = self.ytdlp().ok_or(DownloadError::NeedsYtDlp)?;
        let decision = evaluate_rules(
            &task.source_url,
            None,
            None,
            &self.storage.list_rules()?,
            &self.storage.list_categories()?,
        );
        // A paused run continues in the folder it started in.
        let work_dir = match task.temp_path.as_deref().map(PathBuf::from) {
            Some(path) if crate::ytdlp::is_work_dir(&path) => path,
            _ => self
                .resolve_destination(decision.as_ref(), destination_directory)
                .join(format!("{}{}", crate::ytdlp::WORK_DIR_PREFIX, task.id)),
        };
        let directory = work_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| destination_directory.to_path_buf());
        let mut filename = task.filename.clone().unwrap_or_else(|| "video".to_owned());
        let plan = |filename: &str, total: Option<u64>| TransferPlan {
            resolved_url: task.source_url.clone(),
            filename: filename.to_owned(),
            destination_path: directory.join(filename).to_string_lossy().into_owned(),
            temp_path: work_dir.to_string_lossy().into_owned(),
            mime_type: None,
            total_bytes: total,
            etag: None,
            last_modified: None,
            range_supported: true,
        };
        self.storage
            .set_transfer_plan(&task.id, &plan(&filename, None))?;

        let scope = self.traffic_scope_of(&task.source_url);
        self.ensure_quota_allows(scope)?;
        self.storage
            .mark_downloading(&task.id, unix_timestamp_seconds()?)?;

        let network = self.network_settings();
        let host = normalized_host(&task.source_url).unwrap_or_default();
        let proxy = match network.mode {
            _ if network.goes_direct(&host) => ProxyChoice::Direct,
            crate::network::ProxyMode::Off => ProxyChoice::Direct,
            crate::network::ProxyMode::System => ProxyChoice::System,
            // yt-dlp cannot run a setup script; it gets the script's answer
            // for the page when one is available.
            crate::network::ProxyMode::Pac => network
                .pac_url
                .as_deref()
                .map(crate::pac::PacResolver::new)
                .zip(reqwest::Url::parse(&task.source_url).ok())
                .and_then(|(resolver, url)| resolver.answer_for(&url))
                .map_or(ProxyChoice::System, |answer| match answer {
                    crate::pac::PacAnswer::Direct => ProxyChoice::Direct,
                    crate::pac::PacAnswer::Proxy(proxy) => ProxyChoice::Url(proxy),
                }),
            crate::network::ProxyMode::Manual => network
                .proxy_url
                .clone()
                .map_or(ProxyChoice::System, ProxyChoice::Url),
        };
        // yt-dlp takes one fixed rate: the tightest limit at the start.
        let rate_limit = [
            self.global_limiter.limit(),
            self.task_speed_limit(&task.id)?,
            decision.as_ref().and_then(|decision| decision.speed_cap),
        ]
        .into_iter()
        .flatten()
        .filter(|limit| *limit > 0)
        .min();
        let ffmpeg = self.ffmpeg();
        let max_height = quality_from_link(&task.source_url).or_else(|| self.stream_max_height());
        let request = YtDlpRequest {
            url: &task.source_url,
            work_dir: &work_dir,
            max_height,
            ffmpeg: ffmpeg.as_ref().map(|ffmpeg| ffmpeg.path()),
            proxy,
            rate_limit,
        };

        // Picture and sound arrive one after the other, each counting from
        // zero; earlier parts are carried in `finished_parts`.
        let mut finished_parts = 0_u64;
        let mut last = 0_u64;
        let mut counted = 0_u64;
        let mut last_saved = Instant::now();
        let mut named = task.filename.is_some();
        let task_id = task.id.clone();
        let storage = Arc::clone(&self.storage);
        let result = ytdlp
            .download(&request, control.as_ref(), |progress| {
                if progress.downloaded < last {
                    finished_parts += last;
                }
                last = progress.downloaded;
                let downloaded = finished_parts + progress.downloaded;
                let total = progress.total.map(|total| finished_parts + total);
                if downloaded > counted {
                    self.count_traffic(scope, downloaded - counted);
                    counted = downloaded;
                }
                if !named && let Some(title) = &progress.title {
                    named = true;
                    filename = crate::sanitize_filename(title);
                    let _ = storage.set_transfer_plan(&task_id, &plan(&filename, total));
                }
                if last_saved.elapsed() >= PROGRESS_PERSIST_INTERVAL {
                    last_saved = Instant::now();
                    let _ = storage.update_progress(&task_id, downloaded, total);
                }
                on_progress(
                    &task_id,
                    TransferProgress {
                        downloaded_bytes: downloaded,
                        total_bytes: total,
                        bytes_per_second: progress.bytes_per_second.map(|rate| rate as u64),
                        eta_seconds: progress.eta_seconds,
                        active_connections: Some(1),
                        max_connections: Some(1),
                        adaptive_reason: Some("downloading with yt-dlp"),
                    },
                );
            })
            .await;

        let file = match result {
            Ok(file) => file,
            Err(YtDlpError::Stopped) => {
                return Err(DownloadError::Stopped(
                    control.stop_reason().unwrap_or(StopReason::Pause),
                )
                .into());
            }
            Err(error) => {
                let temporary = error.is_temporary();
                return Err(DownloadError::YtDlp {
                    message: error.to_string(),
                    temporary,
                }
                .into());
            }
        };

        let size = tokio::fs::metadata(&file)
            .await
            .map_err(DownloadError::Io)?
            .len();
        let final_name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(filename);
        self.storage
            .set_transfer_plan(&task.id, &plan(&final_name, Some(size)))?;
        self.storage.update_progress(&task.id, size, Some(size))?;
        let outcome = self
            .base_downloader()
            .finalize_shared_file(&file, &directory.join(&final_name), size)
            .await?;
        remove_work_dir(&work_dir).await;
        Ok(outcome)
    }

    /// FFmpeg, from Settings or found next to the application or on `PATH`.
    pub fn ffmpeg(&self) -> Option<crate::ffmpeg::Ffmpeg> {
        let configured = self
            .storage
            .get_setting(SETTING_FFMPEG_PATH)
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        crate::ffmpeg::Ffmpeg::locate(configured.as_deref())
    }

    pub fn ffmpeg_path_setting(&self) -> Option<String> {
        self.storage
            .get_setting(SETTING_FFMPEG_PATH)
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
    }

    /// Where FFmpeg is; `None` to look for it automatically. A path that
    /// does not name a file is refused.
    pub fn set_ffmpeg_path(&self, path: Option<&Path>) -> Result<()> {
        if let Some(path) = path
            && (!path.is_absolute() || !path.is_file())
        {
            return Err(DownloadServiceError::RelativeDirectory);
        }
        self.storage.set_setting(
            SETTING_FFMPEG_PATH,
            &path
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )?;
        Ok(())
    }

    /// Whether transport streams are rewrapped as MP4 when FFmpeg is there.
    pub fn stream_prefer_mp4(&self) -> bool {
        self.storage
            .get_setting(SETTING_STREAM_PREFER_MP4)
            .ok()
            .flatten()
            .as_deref()
            != Some("false")
    }

    pub fn set_stream_prefer_mp4(&self, enabled: bool) -> Result<()> {
        self.storage.set_setting(
            SETTING_STREAM_PREFER_MP4,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    // -- statistics, export, diagnostics --------------------------------------

    /// Figures for the last `days` local days, today included.
    pub fn download_stats(&self, days: u32) -> Result<crate::stats::DownloadStats> {
        let days = days.clamp(1, 366);
        let now = unix_timestamp_seconds()?;
        let offset = self.utc_offset_seconds.load(Ordering::Relaxed);
        let list: Vec<String> = (0..i64::from(days))
            .rev()
            .map(|back| crate::traffic::local_day(now - back * 86_400, offset))
            .collect();
        let traffic = self
            .storage
            .traffic_by_day(&list[0], list.last().expect("at least one day"))?;
        let records = self.storage.list_downloads()?;
        Ok(crate::stats::build_stats(&records, &traffic, list, offset))
    }

    /// The download list as a CSV spreadsheet or as plain links; `ids`
    /// narrows it to those downloads, in list order.
    pub fn export_downloads(&self, format: ExportFormat, ids: Option<&[String]>) -> Result<String> {
        let mut records = self.storage.list_downloads()?;
        if let Some(ids) = ids {
            records.retain(|record| ids.contains(&record.id));
        }
        Ok(match format {
            ExportFormat::Csv => crate::export::downloads_csv(
                &records,
                self.utc_offset_seconds.load(Ordering::Relaxed),
            ),
            ExportFormat::Links => crate::export::links_text(&records),
        })
    }

    /// A report for asking for help. `download_folder` and `home` let paths
    /// be shown without the user's name.
    pub async fn diagnostics_report(
        &self,
        app_version: &str,
        download_folder: &Path,
        home: Option<&str>,
    ) -> Result<String> {
        let offset = self.utc_offset_seconds.load(Ordering::Relaxed);
        let now = unix_timestamp_seconds()?;
        let records = self.storage.list_downloads()?;

        let mut statuses: Vec<(String, u32)> = Vec::new();
        for record in &records {
            let status = record.status.to_string();
            match statuses.iter_mut().find(|(name, _)| *name == status) {
                Some((_, count)) => *count += 1,
                None => statuses.push((status, 1)),
            }
        }
        statuses.sort();

        let mut troubled: Vec<&DownloadRecord> = records
            .iter()
            .filter(|record| record.error_code.is_some() || record.error_message.is_some())
            .collect();
        troubled.sort_by_key(|record| {
            std::cmp::Reverse(
                record
                    .completed_at
                    .or(record.started_at)
                    .unwrap_or(record.created_at),
            )
        });
        let problems = troubled
            .into_iter()
            .take(20)
            .map(|record| crate::diagnostics::Problem {
                at: crate::traffic::local_time(
                    record
                        .completed_at
                        .or(record.started_at)
                        .unwrap_or(record.created_at),
                    offset,
                ),
                status: record.status.to_string(),
                code: record.error_code.clone(),
                host: crate::stats::host_of(&record.source_url),
                message: record.error_message.clone().unwrap_or_default(),
            })
            .collect();

        let network = self.network_settings();
        let route = match network.mode {
            crate::network::ProxyMode::Off => "direct".to_owned(),
            crate::network::ProxyMode::System => "system proxy".to_owned(),
            crate::network::ProxyMode::Pac => "setup script (PAC)".to_owned(),
            crate::network::ProxyMode::Manual => format!(
                "manual ({})",
                network
                    .proxy_url
                    .as_deref()
                    .and_then(|url| url.split_once("://").map(|(scheme, _)| scheme.to_owned()))
                    .unwrap_or_else(|| "not set".to_owned())
            ),
        };
        let ffmpeg = match self.ffmpeg() {
            Some(ffmpeg) => Some(
                ffmpeg
                    .version()
                    .await
                    .unwrap_or_else(|| "found, version unknown".to_owned()),
            ),
            None => None,
        };
        let post = self.post_process_settings();
        let mut post_steps = Vec::new();
        if post.hash_always {
            post_steps.push("checksum");
        }
        if post.scan {
            post_steps.push("Defender scan");
        }
        if post.extract_zip {
            post_steps.push("unpack ZIP");
        }
        if post.command.is_some() {
            post_steps.push("command");
        }

        let facts = crate::diagnostics::DiagnosticsFacts {
            created_at: crate::traffic::local_time(now, offset),
            app_version: app_version.to_owned(),
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            schema_version: self.storage.schema_version()?,
            database_bytes: self.storage.database_bytes(),
            integrity: self.storage.integrity_check()?,
            statuses,
            route,
            domestic_direct: network.domestic_direct,
            direct_hosts: network.direct_hosts.len(),
            max_connections: u32::try_from(self.max_connections()).unwrap_or(u32::MAX),
            speed_limit: self.global_speed_limit(),
            auto_adopt_links: self.auto_adopt_links(),
            polite_hosts: self.polite_hosts().len(),
            ffmpeg,
            defender: crate::postprocess::defender_path().is_some(),
            post_steps,
            download_folder: crate::diagnostics::hide_home(
                &download_folder.to_string_lossy(),
                home,
            ),
            free_bytes: dm_system::disk::free_space(download_folder),
            problems,
        };
        Ok(crate::diagnostics::render_report(&facts))
    }

    /// Asks a server what a download would, through the route a download
    /// would take, and times it.
    pub async fn connection_check(&self, url: &str) -> ConnectionCheck {
        let host = crate::stats::host_of(url);
        let network = self.network_settings();
        let route = match network.mode {
            crate::network::ProxyMode::Off => "direct",
            _ if network.goes_direct(&host) => "direct",
            crate::network::ProxyMode::System => "system",
            crate::network::ProxyMode::Pac => "pac",
            crate::network::ProxyMode::Manual => "proxy",
        };
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            self.base_downloader().probe(url.trim()),
        )
        .await;
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut check = ConnectionCheck {
            host,
            route: route.to_owned(),
            elapsed_ms,
            ..ConnectionCheck::default()
        };
        match result {
            Ok(Ok(probe)) => {
                check.reachable = true;
                check.final_host = Some(crate::stats::host_of(&probe.final_url))
                    .filter(|final_host| *final_host != check.host);
                check.total_bytes = probe.total_bytes;
                check.range_supported = probe.range_supported;
                check.filename = Some(probe.filename);
            }
            Ok(Err(error)) => {
                check.error = Some(crate::diagnostics::redact_urls(&error.redacted_message()));
            }
            Err(_) => check.error = Some("the server did not answer within 30 seconds".to_owned()),
        }
        check
    }

    // -- after the download -------------------------------------------------

    /// Hears the id of every download whose after-download steps changed.
    pub fn subscribe_post_process(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.post_events.subscribe()
    }

    pub fn post_process_settings(&self) -> PostProcessSettings {
        let flag =
            |key: &str| self.storage.get_setting(key).ok().flatten().as_deref() == Some("true");
        PostProcessSettings {
            hash_always: flag(SETTING_POST_HASH),
            extract_zip: flag(SETTING_POST_EXTRACT),
            scan: flag(SETTING_POST_SCAN),
            command: self
                .storage
                .get_setting(SETTING_POST_COMMAND)
                .ok()
                .flatten()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    pub fn set_post_process_settings(&self, settings: &PostProcessSettings) -> Result<()> {
        let flag = |value: bool| if value { "true" } else { "false" };
        if let Some(command) = &settings.command
            && crate::postprocess::build_command(command, Path::new("file")).is_none()
        {
            return Err(DownloadServiceError::InvalidCommand);
        }
        self.storage
            .set_setting(SETTING_POST_HASH, flag(settings.hash_always))?;
        self.storage
            .set_setting(SETTING_POST_EXTRACT, flag(settings.extract_zip))?;
        self.storage
            .set_setting(SETTING_POST_SCAN, flag(settings.scan))?;
        self.storage.set_setting(
            SETTING_POST_COMMAND,
            settings.command.as_deref().map(str::trim).unwrap_or(""),
        )?;
        Ok(())
    }

    pub fn download_checks(&self, download_id: &str) -> Result<Option<dm_storage::DownloadChecks>> {
        Ok(self.storage.get_download_checks(download_id)?)
    }

    /// Stores the checksum a download should have (`None` forgets it). A
    /// finished download is checked straight away.
    pub async fn set_expected_checksum(
        &self,
        download_id: &str,
        text: Option<&str>,
    ) -> Result<dm_storage::DownloadChecks> {
        let parsed = match text.map(str::trim).filter(|text| !text.is_empty()) {
            Some(text) => Some(
                crate::postprocess::parse_expected_checksum(text)
                    .ok_or(DownloadServiceError::InvalidChecksum)?,
            ),
            None => None,
        };
        let task = self.get_task(download_id)?;
        let mut checks = self
            .storage
            .get_download_checks(download_id)?
            .unwrap_or_else(|| dm_storage::DownloadChecks {
                download_id: download_id.to_owned(),
                ..dm_storage::DownloadChecks::default()
            });
        checks.expected_checksum = parsed.as_ref().map(|(_, hex)| hex.clone());
        checks.algorithm = parsed
            .as_ref()
            .map(|(algorithm, _)| algorithm.as_str().to_owned());
        checks.integrity = None;
        checks.actual_checksum = None;
        checks.updated_at = unix_timestamp_seconds()?;
        self.storage.save_download_checks(&checks)?;

        if task.status == dm_common::DownloadStatus::Completed && parsed.is_some() {
            return self.verify_checksum(download_id).await;
        }
        let _ = self.post_events.send(download_id.to_owned());
        Ok(checks)
    }

    fn post_process_wanted(&self, download_id: &str) -> bool {
        let settings = self.post_process_settings();
        settings.hash_always
            || settings.extract_zip
            || settings.scan
            || settings.command.is_some()
            || self
                .storage
                .get_download_checks(download_id)
                .ok()
                .flatten()
                .is_some_and(|checks| checks.expected_checksum.is_some())
    }

    /// Hashes the finished file and compares it with the expected
    /// checksum, if there is one.
    async fn verify_checksum(&self, download_id: &str) -> Result<dm_storage::DownloadChecks> {
        let task = self.get_task(download_id)?;
        let path = PathBuf::from(task.destination_path.clone().unwrap_or_default());
        let mut checks = self
            .storage
            .get_download_checks(download_id)?
            .unwrap_or_else(|| dm_storage::DownloadChecks {
                download_id: download_id.to_owned(),
                ..dm_storage::DownloadChecks::default()
            });
        let algorithm = checks
            .algorithm
            .as_deref()
            .and_then(crate::postprocess::HashAlgorithm::parse)
            .unwrap_or(crate::postprocess::HashAlgorithm::Sha256);

        checks.state = "running".to_owned();
        self.storage.save_download_checks(&checks)?;
        let _ = self.post_events.send(download_id.to_owned());

        let hashed =
            tokio::task::spawn_blocking(move || crate::postprocess::hash_file(&path, algorithm))
                .await
                .map_err(|_| DownloadServiceError::ExecutionUnavailable)?;
        match hashed {
            Ok(actual) => {
                checks.integrity = checks.expected_checksum.as_ref().map(|expected| {
                    if *expected == actual {
                        "verified"
                    } else {
                        "mismatch"
                    }
                    .to_owned()
                });
                checks.algorithm = Some(algorithm.as_str().to_owned());
                checks.actual_checksum = Some(actual);
            }
            Err(_) => {
                checks.integrity = Some("error".to_owned());
                checks.actual_checksum = None;
            }
        }
        checks.state = "done".to_owned();
        checks.updated_at = unix_timestamp_seconds()?;
        self.storage.save_download_checks(&checks)?;
        if checks.integrity.as_deref() == Some("mismatch") {
            self.storage.record_notice(
                download_id,
                INTEGRITY_FAILED_CODE,
                "The file does not match its checksum: it may be damaged or not the file that was published.",
            )?;
        }
        let _ = self.post_events.send(download_id.to_owned());
        Ok(checks)
    }

    /// Runs every after-download step that is turned on, for a finished
    /// download. Never deletes or runs the downloaded file.
    pub async fn post_process(&self, download_id: &str) -> Result<dm_storage::DownloadChecks> {
        let task = self.get_task(download_id)?;
        if task.status != dm_common::DownloadStatus::Completed {
            return Err(DownloadServiceError::NotRunning(download_id.to_owned()));
        }
        let path = PathBuf::from(task.destination_path.clone().unwrap_or_default());
        let settings = self.post_process_settings();
        let expected = self
            .storage
            .get_download_checks(download_id)?
            .is_some_and(|checks| checks.expected_checksum.is_some());

        let mut checks = if expected || settings.hash_always {
            self.verify_checksum(download_id).await?
        } else {
            self.storage
                .get_download_checks(download_id)?
                .unwrap_or_else(|| dm_storage::DownloadChecks {
                    download_id: download_id.to_owned(),
                    ..dm_storage::DownloadChecks::default()
                })
        };
        checks.state = "running".to_owned();
        self.storage.save_download_checks(&checks)?;
        let _ = self.post_events.send(download_id.to_owned());

        if settings.scan {
            match crate::postprocess::defender_path() {
                Some(scanner) => match crate::postprocess::scan_file(&scanner, &path).await {
                    crate::postprocess::ScanResult::Clean => {
                        checks.scan = Some("clean".to_owned());
                        checks.scan_detail = None;
                    }
                    crate::postprocess::ScanResult::ThreatFound => {
                        checks.scan = Some("threat".to_owned());
                        self.storage.record_notice(
                            download_id,
                            THREAT_FOUND_CODE,
                            "Windows Defender found a threat in this file. Do not open it.",
                        )?;
                    }
                    crate::postprocess::ScanResult::Unavailable(detail) => {
                        checks.scan = Some("unavailable".to_owned());
                        checks.scan_detail = Some(detail);
                    }
                },
                None => {
                    checks.scan = Some("unavailable".to_owned());
                    checks.scan_detail = Some("Windows Defender was not found".to_owned());
                }
            }
        }

        // A file flagged by the scan is not unpacked.
        if settings.extract_zip && checks.scan.as_deref() != Some("threat") {
            let archive = path.clone();
            if tokio::task::spawn_blocking(move || crate::postprocess::is_zip(&archive))
                .await
                .unwrap_or(false)
            {
                let archive = path.clone();
                match tokio::task::spawn_blocking(move || crate::postprocess::extract_zip(&archive))
                    .await
                {
                    Ok(Ok(report)) => {
                        checks.extracted_to = Some(report.folder.to_string_lossy().into_owned());
                        checks.extract_error = None;
                    }
                    Ok(Err(error)) => checks.extract_error = Some(error.to_string()),
                    Err(_) => {
                        checks.extract_error = Some("extraction stopped unexpectedly".to_owned())
                    }
                }
            }
        }

        if let Some(command) = &settings.command
            && checks.scan.as_deref() != Some("threat")
        {
            checks.command_error = crate::postprocess::run_command(command, &path).await.err();
        }

        checks.state = "done".to_owned();
        checks.updated_at = unix_timestamp_seconds()?;
        self.storage.save_download_checks(&checks)?;
        let _ = self.post_events.send(download_id.to_owned());
        Ok(checks)
    }

    /// Highest stream quality picked automatically; `None` for the best.
    pub fn stream_max_height(&self) -> Option<u32> {
        self.storage
            .get_setting(SETTING_STREAM_MAX_HEIGHT)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
    }

    pub fn set_stream_max_height(&self, height: Option<u32>) -> Result<()> {
        self.storage.set_setting(
            SETTING_STREAM_MAX_HEIGHT,
            &height
                .filter(|value| *value > 0)
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )?;
        Ok(())
    }

    /// The qualities a stream link offers, for the user to choose from.
    pub async fn stream_variants(&self, url: &str) -> Result<Vec<crate::hls::Variant>> {
        validate_source_url(url)?;
        let downloader = self.base_downloader();
        if crate::media::classify_source(url, None) != Some(crate::media::MediaKind::Dash) {
            return Ok(crate::hls::list_variants(&downloader, url).await?);
        }
        let control = TaskControl::new();
        let bytes = downloader
            .fetch_bytes(url, None, crate::hls::MAX_PLAYLIST_BYTES, &control)
            .await?;
        let base = reqwest::Url::parse(url)
            .map_err(|error| DownloadError::InvalidUrl(error.to_string()))?;
        let manifest = crate::dash::parse_manifest(&base, &String::from_utf8_lossy(&bytes))
            .map_err(DownloadError::Stream)?;
        let separate_sound = !manifest.audio.is_empty();
        Ok(manifest
            .video
            .iter()
            .chain(&manifest.muxed)
            .map(|representation| crate::hls::Variant {
                uri: url.to_owned(),
                bandwidth: representation.bandwidth,
                width: representation.width,
                height: representation.height,
                audio_group: None,
                audio_uri: (separate_sound && representation.kind == crate::dash::TrackKind::Video)
                    .then(|| "separate".to_owned()),
            })
            .collect())
    }

    /// The addresses a segmented transfer may read from: the resolved
    /// source, then each mirror that answers with ranges for a file of the
    /// same size and, when both sides have one, the same validator. A mirror
    /// that does not prove this is skipped, never trusted.
    async fn verified_sources(
        &self,
        download_id: &str,
        downloader: &Downloader,
        probe: &crate::SourceProbe,
        total_bytes: u64,
    ) -> Result<Vec<String>> {
        let mut sources = vec![probe.final_url.clone()];
        for mirror in self.storage.list_mirrors(download_id)? {
            let Ok(found) = downloader.probe(&mirror).await else {
                continue;
            };
            let same_file = found.range_supported
                && found.total_bytes == Some(total_bytes)
                && match (&probe.etag, &found.etag) {
                    (Some(expected), Some(actual)) => expected == actual,
                    _ => true,
                };
            if same_file && !sources.contains(&found.final_url) {
                sources.push(found.final_url);
            }
        }
        Ok(sources)
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
        let polite =
            self.is_polite_host(&host) || self.is_polite_host(&normalized_host(&task.source_url)?);
        let max_connections = if polite {
            self.max_connections().min(POLITE_CONNECTIONS)
        } else {
            self.max_connections()
        };
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
            // Polite hosts get their planned ranges and nothing more.
            min_split_bytes: if polite {
                u64::MAX
            } else {
                self.min_split_bytes
            },
            healthy: vec![true],
            next_source: 0,
        };
        let downloader = self.downloader_for(&task.id)?;
        let sources = self
            .verified_sources(&task.id, &downloader, probe, total_bytes)
            .await?;
        pool.healthy = vec![true; sources.len()];
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
        let context = SegmentPoolContext {
            downloader,
            storage: Arc::clone(&self.storage),
            control: Arc::clone(&control),
            sources,
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
        // Connections lost in a row with no progress in between.
        let mut range_retries = 0_u32;
        let mut bytes_at_last_failure = downloaded;

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
                        Ok(Err(failure)) => {
                            let mirror_failed =
                                pool.healthy_sources() > 1 && is_source_failure(&failure.error);
                            // A dropped or stalled connection costs only its
                            // own range, which goes back to be picked up
                            // again, as long as the download keeps moving.
                            if downloaded > bytes_at_last_failure {
                                range_retries = 0;
                            }
                            let connection_dropped = !mirror_failed
                                && range_retries < MAX_RANGE_RETRIES
                                && matches!(&failure.error, DownloadServiceError::Download(error)
                                    if classify_failure(error) == FailureClass::Retryable);
                            if mirror_failed || connection_dropped {
                                if mirror_failed {
                                    pool.healthy[failure.source] = false;
                                } else {
                                    range_retries += 1;
                                }
                                downloaded = downloaded.saturating_sub(failure.lost_bytes);
                                bytes_at_last_failure = downloaded;
                                pool.active.remove(&failure.segment_index);
                                let requeued = self
                                    .storage
                                    .release_download_segment(&task.id, failure.segment_index)
                                    .and_then(|()| {
                                        self.storage
                                            .get_download_segment(&task.id, failure.segment_index)
                                    });
                                match requeued {
                                    Ok(Some(segment)) => pool.pending.push_front(segment),
                                    Ok(None) => {}
                                    Err(error) => break Err(error.into()),
                                }
                                if let Err(error) = pool.fill(&task.id, target_connections, &context) {
                                    break Err(error);
                                }
                            } else {
                                break Err(failure.error);
                            }
                        }
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
        let attempts = task.attempts.saturating_add(1);

        // A signed or session-bound link that stopped working is not a
        // failure a retry can fix; say what happened and how to go on.
        if link_looks_expired(&error, task) {
            self.storage.record_attempt(download_id, attempts)?;
            self.storage
                .mark_failed(download_id, LINK_EXPIRED_CODE, LINK_EXPIRED_MESSAGE)?;
            return Err(DownloadServiceError::Download(error));
        }

        let message = error.redacted_message();
        let class = classify_failure(&error);

        if class == FailureClass::Retryable
            && let Some(delay) = self.retry_policy.delay_for(attempts)
        {
            // A server that said when to come back is taken at its word.
            let asked = self
                .base_downloader()
                .take_retry_after(task.resolved_url.as_deref().unwrap_or(&task.source_url))
                .or_else(|| self.base_downloader().take_retry_after(&task.source_url));
            // A server that is refusing because of load but did not say for
            // how long gets a real pause, not a retry every few seconds.
            let delay = match asked {
                Some(asked) => asked.max(delay),
                None if matches!(error.http_status(), Some(429 | 503)) => {
                    delay.max(RATE_LIMITED_MINIMUM_DELAY)
                }
                None => delay,
            };
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

/// One piece of a stream in download order: address, byte range, key,
/// media sequence number.
type StreamPart = (
    String,
    Option<(u64, u64)>,
    Option<crate::hls::SegmentKey>,
    u64,
);

/// One track of a stream: its parts in order, and the address that
/// identifies it for resuming.
struct StreamTrack {
    identity: String,
    parts: Vec<StreamPart>,
}

/// What a stream link turned into: one track, or picture and sound.
struct StreamPlan {
    tracks: Vec<StreamTrack>,
    /// The container the parts make as they are: `ts` or `mp4`.
    extension: &'static str,
}

#[derive(Default)]
struct StreamProgress {
    written: u64,
    parts_done: usize,
    parts_total: usize,
    /// Parts already on disk when this attempt started; they tell nothing
    /// about the size of the parts still to come.
    parts_skipped: usize,
    bytes_skipped: u64,
    meter: Option<ThroughputMeter>,
}

impl StreamProgress {
    /// The total, estimated from the average size of the parts fetched.
    fn estimated_total(&self) -> Option<u64> {
        let fetched = self.parts_done.saturating_sub(self.parts_skipped);
        let bytes = self.written.saturating_sub(self.bytes_skipped);
        (fetched > 0).then(|| {
            let average = bytes / fetched as u64;
            self.written + average * self.parts_total.saturating_sub(self.parts_done) as u64
        })
    }
}

/// A quality named in the link itself: `...#rud-quality=720`. The fragment
/// never reaches the server.
fn quality_from_link(url: &str) -> Option<u32> {
    let fragment = url.split_once('#')?.1;
    fragment
        .split('&')
        .find_map(|pair| pair.strip_prefix("rud-quality="))
        .and_then(|value| value.parse().ok())
        .filter(|height| *height > 0)
}

async fn resolve_stream_plan(
    downloader: &Downloader,
    url: &str,
    kind: crate::media::MediaKind,
    max_height: Option<u32>,
    can_join: bool,
    control: &TaskControl,
) -> Result<StreamPlan> {
    let hls_parts = |playlist: &crate::hls::MediaPlaylist| -> Vec<StreamPart> {
        let mut parts: Vec<StreamPart> = Vec::new();
        if let Some((uri, range)) = &playlist.init {
            parts.push((uri.clone(), *range, None, 0));
        }
        parts.extend(playlist.segments.iter().map(|segment| {
            (
                segment.uri.clone(),
                segment.byte_range,
                segment.key.clone(),
                segment.sequence,
            )
        }));
        parts
    };

    if kind == crate::media::MediaKind::Hls {
        let stream =
            crate::hls::resolve_stream(downloader, url, max_height, can_join, control).await?;
        let mut tracks = vec![StreamTrack {
            identity: stream.media_url.clone(),
            parts: hls_parts(&stream.playlist),
        }];
        if let Some((audio_url, audio)) = &stream.audio {
            tracks.push(StreamTrack {
                identity: audio_url.clone(),
                parts: hls_parts(audio),
            });
        }
        return Ok(StreamPlan {
            tracks,
            extension: stream.playlist.extension(),
        });
    }

    let bytes = downloader
        .fetch_bytes(url, None, crate::hls::MAX_PLAYLIST_BYTES, control)
        .await?;
    let base =
        reqwest::Url::parse(url).map_err(|error| DownloadError::InvalidUrl(error.to_string()))?;
    let manifest = crate::dash::parse_manifest(&base, &String::from_utf8_lossy(&bytes))
        .map_err(DownloadError::Stream)?;
    let chosen = manifest.choose(max_height).map_err(DownloadError::Stream)?;
    if chosen.len() > 1 && !can_join {
        return Err(DownloadError::Stream(crate::hls::HlsError::NeedsMuxing).into());
    }
    let tracks = chosen
        .into_iter()
        .map(|representation| {
            let mut parts: Vec<StreamPart> = Vec::new();
            if let Some((uri, range)) = &representation.init {
                parts.push((uri.clone(), *range, None, 0));
            }
            parts.extend(
                representation
                    .segments
                    .iter()
                    .map(|(uri, range)| (uri.clone(), *range, None, 0)),
            );
            StreamTrack {
                identity: format!("{url}#representation={}", representation.id),
                parts,
            }
        })
        .collect();
    Ok(StreamPlan {
        tracks,
        extension: "mp4",
    })
}

/// Where a stream download records how far it got.
struct StreamRecord {
    media_url: String,
    parts: usize,
    done: usize,
    bytes: u64,
}

fn stream_record_path(temp_path: &Path) -> PathBuf {
    let mut name = temp_path.as_os_str().to_owned();
    name.push(".stream");
    PathBuf::from(name)
}

async fn read_stream_record(path: &Path) -> Option<StreamRecord> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    let mut lines = text.lines();
    if lines.next()? != "rud-stream-v1" {
        return None;
    }
    Some(StreamRecord {
        media_url: lines.next()?.to_owned(),
        parts: lines.next()?.parse().ok()?,
        done: lines.next()?.parse().ok()?,
        bytes: lines.next()?.parse().ok()?,
    })
}

/// Written beside, then renamed over, so a crash never leaves half a record.
async fn write_stream_record(path: &Path, record: &StreamRecord) {
    let text = format!(
        "rud-stream-v1\n{}\n{}\n{}\n{}\n",
        record.media_url, record.parts, record.done, record.bytes
    );
    let mut staging = path.as_os_str().to_owned();
    staging.push(".new");
    let staging = PathBuf::from(staging);
    if tokio::fs::write(&staging, text).await.is_ok() {
        let _ = tokio::fs::rename(&staging, path).await;
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
    /// The resolved source first, then every mirror that proved to serve
    /// the same bytes.
    sources: Vec<String>,
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

/// A connection that failed, and the source it was reading from, so a bad
/// mirror can be dropped without failing the whole download.
struct RangeFailure {
    segment_index: u32,
    source: usize,
    error: DownloadServiceError,
    /// Bytes this connection reported that were not yet durable, and so
    /// will be downloaded again: they come off the running total.
    lost_bytes: u64,
}

/// The ranges of one transfer: those waiting for a connection and those
/// being downloaded, by segment index.
struct SegmentPool {
    pending: std::collections::VecDeque<DownloadSegment>,
    active: HashMap<u32, Arc<RangeSlot>>,
    next_index: u32,
    workers: JoinSet<std::result::Result<FinishedRange, RangeFailure>>,
    min_split_bytes: u64,
    /// Which sources are still trusted, by index into the context's list.
    healthy: Vec<bool>,
    next_source: usize,
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

    fn healthy_sources(&self) -> usize {
        self.healthy.iter().filter(|healthy| **healthy).count()
    }

    /// The next trusted source, taking turns so ranges spread evenly.
    fn pick_source(&mut self) -> usize {
        let count = self.healthy.len().max(1);
        for _ in 0..count {
            let candidate = self.next_source % count;
            self.next_source = self.next_source.wrapping_add(1);
            if self.healthy.get(candidate).copied().unwrap_or(false) {
                return candidate;
            }
        }
        0
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
        let source = self.pick_source();
        let source_url = context.sources[source.min(context.sources.len() - 1)].clone();
        let temp_path = context.temp_path.clone();
        let total_bytes = context.total_bytes;
        let checkpoint_bytes = context.checkpoint_bytes;
        let checkpoint_interval = context.checkpoint_interval;
        let events = context.events.clone();

        let reported = Arc::new(std::sync::atomic::AtomicU64::new(segment.downloaded_bytes));
        let durable = Arc::new(std::sync::atomic::AtomicU64::new(segment.downloaded_bytes));

        self.workers.spawn(async move {
            let download_id = segment.download_id.clone();
            let index = segment.segment_index;
            let (reported_seen, durable_seen) = (Arc::clone(&reported), Arc::clone(&durable));
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
                            reported.fetch_add(progress.written, Ordering::Relaxed);
                            let _ = events.send(progress.written);
                        }
                        if let Some(bytes) = progress.durable_bytes {
                            storage
                                .update_download_segment_progress(&download_id, index, bytes)
                                .map_err(|error| {
                                    DownloadError::ProgressCallback(error.to_string())
                                })?;
                            durable.store(bytes, Ordering::Relaxed);
                        }
                        Ok(())
                    },
                )
                .await
                .map_err(|error| RangeFailure {
                    segment_index: index,
                    source,
                    error: error.into(),
                    lost_bytes: reported_seen
                        .load(Ordering::Relaxed)
                        .saturating_sub(durable_seen.load(Ordering::Relaxed)),
                })?;
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

/// Removes a yt-dlp working folder, and nothing that is not one.
async fn remove_work_dir(path: &Path) {
    if crate::ytdlp::is_work_dir(path) {
        let _ = tokio::fs::remove_dir_all(path).await;
    }
}

async fn remove_partial_file(temp_path: Option<&str>) {
    if let Some(path) = temp_path
        && crate::ytdlp::is_work_dir(Path::new(path))
    {
        remove_work_dir(Path::new(path)).await;
        return;
    }
    if let Some(path) = temp_path {
        let _ = tokio::fs::remove_file(stream_record_path(Path::new(path))).await;
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// The shortest wait after a 429 or 503 that came without `Retry-After`.
const RATE_LIMITED_MINIMUM_DELAY: Duration = Duration::from_secs(30);

fn error_code_for(error: &DownloadError) -> &'static str {
    match error {
        DownloadError::InvalidUrl(_) | DownloadError::UnsupportedScheme(_) => "invalid_source",
        DownloadError::Http(_) | DownloadError::HttpStatus { .. } => match error.http_status() {
            Some(429) => "rate_limited",
            Some(503) => "server_busy",
            Some(404 | 410) => "not_found",
            Some(401 | 403 | 407) => "access_denied",
            Some(status) if status >= 500 => "server_error",
            Some(_) => "http_refused",
            None => "network_error",
        },
        DownloadError::Io(_) => "filesystem_error",
        DownloadError::ProgressCallback(_) => "storage_error",
        DownloadError::IncompleteTransfer { .. } => "incomplete_transfer",
        DownloadError::InvalidRangeResponse { .. } => "invalid_range_response",
        DownloadError::SegmentOverflow { .. } => "segment_overflow",
        DownloadError::Stopped(_) => "stopped",
        DownloadError::Stream(crate::hls::HlsError::Protected) => "protected_stream",
        DownloadError::Stream(crate::hls::HlsError::Live) => "live_stream",
        DownloadError::Stream(crate::hls::HlsError::NeedsMuxing) => "needs_muxing",
        DownloadError::Stream(crate::hls::HlsError::Dash) => "unsupported_stream",
        DownloadError::Stream(_) => "stream_error",
        DownloadError::TooLarge { .. } => "stream_error",
        DownloadError::Ffmpeg(_) => "ffmpeg_failed",
        DownloadError::YtDlp { .. } => "ytdlp_failed",
        DownloadError::NeedsYtDlp => "needs_ytdlp",
    }
}

/// Errors that belong to the address being read, not to the download as a
/// whole: another mirror may well succeed where this one failed.
fn is_source_failure(error: &DownloadServiceError) -> bool {
    matches!(
        error,
        DownloadServiceError::Download(
            DownloadError::Http(_)
                | DownloadError::HttpStatus { .. }
                | DownloadError::InvalidRangeResponse { .. }
                | DownloadError::IncompleteTransfer { .. }
        )
    )
}

/// A 401/403/404/410 means an expired link when the link had worked
/// before, or when it carries the signature or expiry parameters that
/// time-limited links use.
fn link_looks_expired(error: &DownloadError, task: &DownloadRecord) -> bool {
    let status = match error {
        DownloadError::HttpStatus { status } => Some(*status),
        DownloadError::Http(error) => error.status().map(|status| status.as_u16()),
        _ => None,
    };
    matches!(status, Some(401 | 403 | 404 | 410))
        && (task.downloaded_bytes > 0 || url_looks_signed(&task.source_url))
}

/// Query parameters that time-limited download links commonly carry: S3,
/// Google Cloud, CloudFront, nginx `secure_link` and the like.
pub fn url_looks_signed(url: &str) -> bool {
    const MARKERS: &[&str] = &[
        "expires",
        "expire",
        "expiry",
        "exp",
        "e",
        "signature",
        "sig",
        "token",
        "st",
        "md5",
        "hash",
        "policy",
        "key-pair-id",
        "x-amz-signature",
        "x-amz-expires",
        "x-goog-signature",
        "x-goog-expires",
    ];
    reqwest::Url::parse(url).is_ok_and(|url| {
        url.query_pairs()
            .any(|(key, _)| MARKERS.contains(&key.to_ascii_lowercase().as_str()))
    })
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
    async fn a_download_limit_slows_only_that_download_and_changes_live() {
        let server = TestServer::start(ServerBehaviour {
            body: vec![3_u8; 200_000],
            chunk_size: 4_096,
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let free = harness
            .service
            .create_task(&server.url("free.bin"))
            .unwrap();
        let started = std::time::Instant::now();
        harness
            .service
            .start_task(&free.id, &harness.destination)
            .await
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(600));

        // 20 KB/s would take ten seconds; lifting the limit part-way lets
        // the running transfer finish at full speed.
        let limited = harness
            .service
            .create_task(&server.url("slow.bin"))
            .unwrap();
        harness
            .service
            .set_task_speed_limit(&limited.id, Some(20_000))
            .unwrap();
        assert_eq!(
            harness.service.task_speed_limit(&limited.id).unwrap(),
            Some(20_000)
        );
        let service = harness.service.clone();
        let id = limited.id.clone();
        let destination = harness.destination.clone();
        let started = std::time::Instant::now();
        let transfer = tokio::spawn(async move { service.start_task(&id, &destination).await });
        tokio::time::sleep(Duration::from_millis(700)).await;
        let partway = harness
            .service
            .get_task(&limited.id)
            .unwrap()
            .downloaded_bytes;
        assert!(partway < 100_000, "limited transfer ran ahead: {partway}");
        harness
            .service
            .set_task_speed_limit(&limited.id, None)
            .unwrap();
        let record = transfer.await.unwrap().unwrap();
        assert_eq!(record.status, DownloadStatus::Completed);
        assert!(started.elapsed() < Duration::from_secs(4));
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

        // Named for what happened, and not retried within seconds when the
        // server gave no Retry-After.
        let task = harness.service.get_task(&created.id).unwrap();
        assert_eq!(task.status, DownloadStatus::Retrying);
        assert_eq!(task.error_code.as_deref(), Some("rate_limited"));
        let wait = task.retry_at.unwrap() - unix_timestamp_seconds().unwrap();
        assert!(
            wait >= RATE_LIMITED_MINIMUM_DELAY.as_secs() as i64 - 2,
            "retry in {wait}s"
        );
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
        assert_eq!(downloads[0].error_code.as_deref(), Some("not_found"));
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

    /// Starts a slow download, pauses it part-way and returns the paused row.
    async fn paused_part_way(harness: &Harness, url: &str) -> DownloadRecord {
        let created = harness.service.create_task(url).unwrap();
        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = created.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });
        await_status(&harness.storage, &created.id, DownloadStatus::Downloading).await;
        await_partial_bytes(&harness.storage, &created.id).await;
        harness.service.pause_task(&created.id).unwrap();
        let paused = transfer.await.unwrap().unwrap();
        assert_eq!(paused.status, DownloadStatus::Paused);
        paused
    }

    #[tokio::test]
    async fn a_link_that_stops_working_part_way_is_reported_as_expired() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let paused = paused_part_way(&harness, &server.url("movie.bin")).await;

        server.update(|behaviour| behaviour.status = Some((403, "Forbidden")));
        let _ = harness
            .service
            .start_task(&paused.id, &harness.destination)
            .await;

        let failed = harness.storage.get_download(&paused.id).unwrap().unwrap();
        assert_eq!(failed.status, DownloadStatus::Failed);
        assert_eq!(failed.error_code.as_deref(), Some(LINK_EXPIRED_CODE));
        assert_eq!(
            failed.downloaded_bytes, paused.downloaded_bytes,
            "what was downloaded is kept"
        );
        assert!(
            tokio::fs::try_exists(failed.temp_path.unwrap())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_refused_signed_link_is_expired_but_a_plain_refusal_is_not() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((403, "Forbidden")),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let signed = harness
            .service
            .create_task(&server.url("file.bin?Expires=1790000000&Signature=abc"))
            .unwrap();
        let _ = harness
            .service
            .start_task(&signed.id, &harness.destination)
            .await;
        let signed = harness.storage.get_download(&signed.id).unwrap().unwrap();
        assert_eq!(signed.error_code.as_deref(), Some(LINK_EXPIRED_CODE));

        let plain = harness
            .service
            .create_task(&server.url("file.bin"))
            .unwrap();
        let _ = harness
            .service
            .start_task(&plain.id, &harness.destination)
            .await;
        let plain = harness.storage.get_download(&plain.id).unwrap().unwrap();
        assert_ne!(plain.error_code.as_deref(), Some(LINK_EXPIRED_CODE));
    }

    #[test]
    fn signed_links_are_recognised_by_their_parameters() {
        assert!(url_looks_signed(
            "https://bucket.s3.amazonaws.com/a.zip?X-Amz-Expires=300&X-Amz-Signature=f"
        ));
        assert!(url_looks_signed(
            "https://dl.example.ir/a.zip?md5=abc&e=1790000000"
        ));
        assert!(url_looks_signed("https://cdn.example.com/a.zip?token=xyz"));
        assert!(!url_looks_signed("https://example.com/a.zip?lang=fa"));
        assert!(!url_looks_signed("https://example.com/a.zip"));
    }

    #[tokio::test]
    async fn a_fresh_link_for_the_same_file_continues_the_stopped_download() {
        let old_server = TestServer::start(slow_server()).await;
        let new_server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let paused = paused_part_way(&harness, &old_server.url("movie.bin")).await;
        let fresh_url = new_server.url("movie.bin?token=new");

        let fresh = harness.service.create_task(&fresh_url).unwrap();
        let adopted = harness
            .service
            .adopt_fresh_link(&fresh.id)
            .await
            .unwrap()
            .expect("the paused download matches the new link");

        assert_eq!(adopted.id, paused.id);
        assert_eq!(adopted.source_url, fresh_url);
        assert_eq!(adopted.downloaded_bytes, paused.downloaded_bytes);
        assert_eq!(adopted.error_code.as_deref(), Some(LINK_ADOPTED_CODE));
        assert!(
            harness.storage.get_download(&fresh.id).unwrap().is_none(),
            "no second copy of the download is left behind"
        );

        let finished = harness
            .service
            .start_task(&adopted.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(finished.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
        assert!(
            new_server.ranged_request_count() >= 2,
            "the new link continued from the kept bytes"
        );
    }

    #[tokio::test]
    async fn a_new_link_for_a_different_file_is_not_adopted() {
        let old_server = TestServer::start(slow_server()).await;
        let other = TestServer::start(ServerBehaviour {
            body: b"a different and longer file than the one that was paused".to_vec(),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let paused = paused_part_way(&harness, &old_server.url("movie.bin")).await;

        let fresh = harness
            .service
            .create_task(&other.url("movie.bin"))
            .unwrap();
        assert!(harness.service.may_adopt(&fresh.id));
        assert!(
            harness
                .service
                .adopt_fresh_link(&fresh.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(harness.storage.get_download(&fresh.id).unwrap().is_some());
        let untouched = harness.storage.get_download(&paused.id).unwrap().unwrap();
        assert_eq!(untouched.source_url, paused.source_url);

        harness.service.set_auto_adopt_links(false).unwrap();
        assert!(!harness.service.may_adopt(&fresh.id));
    }

    #[tokio::test]
    async fn refreshing_a_link_by_hand_keeps_the_downloaded_bytes() {
        let server = TestServer::start(slow_server()).await;
        let harness = harness();
        let paused = paused_part_way(&harness, &server.url("movie.bin")).await;

        let refreshed = harness
            .service
            .refresh_source_url(&paused.id, &server.url("movie.bin?fresh=1"))
            .unwrap();
        assert_eq!(refreshed.downloaded_bytes, paused.downloaded_bytes);
        assert_eq!(refreshed.temp_path, paused.temp_path);

        let finished = harness
            .service
            .start_task(&paused.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            DEFAULT_BODY
        );
    }

    #[tokio::test]
    async fn a_server_that_asks_to_wait_is_given_that_long() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((503, "Service Unavailable")),
            retry_after: Some(120),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let task = harness
            .service
            .create_task(&server.url("busy.bin"))
            .unwrap();
        let before = unix_timestamp_seconds().unwrap();

        let _ = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await;

        let retrying = harness.storage.get_download(&task.id).unwrap().unwrap();
        assert_eq!(retrying.status, DownloadStatus::Retrying);
        assert!(
            retrying.retry_at.unwrap() >= before + 120,
            "the next attempt waits at least as long as the server asked"
        );
    }

    #[tokio::test]
    async fn polite_hosts_get_two_connections_and_no_splitting() {
        let behaviour = slow_body_server();
        let body = behaviour.body.clone();
        let server = TestServer::start(behaviour).await;
        let harness = harness();
        harness
            .service
            .set_polite_hosts(&["127.0.0.1".to_owned()])
            .unwrap();
        let service = harness
            .service
            .clone()
            .with_segment_connections(8)
            .with_segmented_threshold(1)
            .with_segment_sizes(1, 100)
            .with_evaluation_window(Duration::from_millis(20));
        let task = service.create_task(&server.url("gentle.bin")).unwrap();

        let finished = service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            body
        );
        assert!(server.peak_concurrent_bodies() <= POLITE_CONNECTIONS);
        // One probe plus the two planned ranges: nothing was split off.
        assert_eq!(server.ranged_request_count(), 1 + POLITE_CONNECTIONS);
    }

    fn mirrored_service(harness: &Harness) -> DownloadService {
        harness
            .service
            .clone()
            .with_segment_connections(4)
            .with_segmented_threshold(1)
            .with_segment_sizes(1, 200)
            .with_evaluation_window(Duration::from_millis(20))
    }

    #[tokio::test]
    async fn ranges_are_spread_over_mirrors_of_the_same_file() {
        let behaviour = slow_body_server();
        let body = behaviour.body.clone();
        let primary = TestServer::start(behaviour.clone()).await;
        let mirror = TestServer::start(behaviour).await;
        let harness = harness();
        let service = mirrored_service(&harness);
        let task = service.create_task(&primary.url("big.iso")).unwrap();
        service
            .set_mirrors(&task.id, &[mirror.url("mirror/big.iso")])
            .unwrap();

        let finished = service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            body
        );
        assert!(
            mirror.ranged_request_count() >= 2,
            "the mirror served ranges, not just the check"
        );
        assert!(primary.ranged_request_count() >= 2);
    }

    #[tokio::test]
    async fn a_mirror_that_breaks_is_dropped_and_its_range_finished_elsewhere() {
        let behaviour = slow_body_server();
        let body = behaviour.body.clone();
        let primary = TestServer::start(behaviour.clone()).await;
        let broken = TestServer::start(behaviour).await;
        let harness = harness();
        let service = mirrored_service(&harness);
        let task = service.create_task(&primary.url("big.iso")).unwrap();
        service
            .set_mirrors(&task.id, &[broken.url("big.iso")])
            .unwrap();
        // The mirror passes its check, then cuts every range short.
        broken.update(|behaviour| behaviour.truncate_after = Some(20));

        let finished = service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            body
        );
        assert!(
            broken.ranged_request_count() >= 2,
            "the broken mirror was really tried for a range"
        );
    }

    #[tokio::test]
    async fn a_mirror_of_a_different_file_is_never_used() {
        let behaviour = slow_body_server();
        let body = behaviour.body.clone();
        let primary = TestServer::start(behaviour).await;
        let other = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let service = mirrored_service(&harness);
        let task = service.create_task(&primary.url("big.iso")).unwrap();
        service
            .set_mirrors(&task.id, &[other.url("big.iso")])
            .unwrap();

        let finished = service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            body
        );
        assert_eq!(other.ranged_request_count(), 1, "only the check reached it");
    }

    #[test]
    fn mirrors_must_be_web_addresses() {
        let harness = harness();
        let task = harness
            .service
            .create_task("https://example.com/a.iso")
            .unwrap();
        assert!(
            harness
                .service
                .set_mirrors(&task.id, &["file:///etc/passwd".to_owned()])
                .is_err()
        );
        assert!(harness.service.mirrors(&task.id).unwrap().is_empty());
    }

    fn encrypt_part(plain: &[u8], key: [u8; 16], sequence: u64) -> Vec<u8> {
        use aes::Aes128;
        use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
        let mut iv = [0_u8; 16];
        iv[8..].copy_from_slice(&sequence.to_be_bytes());
        let mut buffer = plain.to_vec();
        buffer.resize(plain.len() + 16, 0);
        cbc::Encryptor::<Aes128>::new(&key.into(), &iv.into())
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, plain.len())
            .unwrap()
            .to_vec()
    }

    /// A small VOD stream: a master playlist, one quality with three parts,
    /// the last of them AES-128 encrypted.
    fn stream_server(chunk_delay: Option<Duration>) -> (ServerBehaviour, Vec<u8>) {
        let key = [9_u8; 16];
        let second = b"second part, ".repeat(60);
        let third = b"third and last part ".repeat(40);
        let parts: [&[u8]; 3] = [b"first part of the video ", &second, &third];
        let master = "#EXTM3U\n\
            #EXT-X-STREAM-INF:BANDWIDTH=400000,RESOLUTION=640x360\nlow/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=1200000,RESOLUTION=1280x720\nhigh/index.m3u8\n";
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:0\n\
            #EXTINF:4,\na.ts\n#EXTINF:4,\nb.ts\n\
            #EXT-X-KEY:METHOD=AES-128,URI=\"/keys/k1\"\n#EXTINF:4,\nc.ts\n#EXT-X-ENDLIST\n";
        let low = "#EXTM3U\n#EXTINF:4,\nwrong.ts\n#EXT-X-ENDLIST\n";
        let behaviour = ServerBehaviour {
            chunk_size: 16,
            chunk_delay,
            filename: None,
            routes: vec![
                (
                    "/show/episode-12/master.m3u8".to_owned(),
                    master.as_bytes().to_vec(),
                ),
                (
                    "/show/episode-12/high/index.m3u8".to_owned(),
                    media.as_bytes().to_vec(),
                ),
                (
                    "/show/episode-12/low/index.m3u8".to_owned(),
                    low.as_bytes().to_vec(),
                ),
                ("/show/episode-12/high/a.ts".to_owned(), parts[0].to_vec()),
                ("/show/episode-12/high/b.ts".to_owned(), parts[1].to_vec()),
                (
                    "/show/episode-12/high/c.ts".to_owned(),
                    encrypt_part(parts[2], key, 2),
                ),
                ("/keys/k1".to_owned(), key.to_vec()),
            ],
            ..ServerBehaviour::default()
        };
        (behaviour, parts.concat())
    }

    #[tokio::test]
    async fn an_hls_stream_is_saved_in_the_best_quality_and_decrypted() {
        let (behaviour, expected) = stream_server(None);
        let server = TestServer::start(behaviour).await;
        let harness = harness();
        // These parts are not real video, so FFmpeg must stay out of it.
        harness.service.set_stream_prefer_mp4(false).unwrap();
        let task = harness
            .service
            .create_task(&server.url("show/episode-12/master.m3u8"))
            .unwrap();

        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        let path = finished.destination_path.clone().unwrap();
        assert!(
            path.ends_with("episode-12.ts"),
            "named after the show: {path}"
        );
        assert_eq!(tokio::fs::read(&path).await.unwrap(), expected);
        assert_eq!(finished.total_bytes, Some(expected.len() as u64));
    }

    #[tokio::test]
    async fn a_paused_stream_continues_from_the_parts_it_already_has() {
        let (behaviour, expected) = stream_server(Some(Duration::from_millis(30)));
        let server = TestServer::start(behaviour).await;
        let harness = harness();
        // These parts are not real video, so FFmpeg must stay out of it.
        harness.service.set_stream_prefer_mp4(false).unwrap();
        let task = harness
            .service
            .create_task(&server.url("show/episode-12/master.m3u8"))
            .unwrap();
        let service = harness.service.clone();
        let destination = harness.destination.clone();
        let task_id = task.id.clone();
        let transfer = tokio::spawn(async move { service.start_task(&task_id, destination).await });

        // Wait until at least one part is safely in the file.
        let mut paused_at = 0;
        for _ in 0..1200 {
            let row = harness.storage.get_download(&task.id).unwrap().unwrap();
            if row.downloaded_bytes > 0 {
                paused_at = row.downloaded_bytes;
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        assert!(paused_at > 0);
        harness.service.pause_task(&task.id).unwrap();
        let paused = transfer.await.unwrap().unwrap();
        assert_eq!(paused.status, DownloadStatus::Paused);

        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read(finished.destination_path.as_ref().unwrap())
                .await
                .unwrap(),
            expected
        );
    }

    #[tokio::test]
    async fn protected_live_and_dash_streams_fail_with_a_reason() {
        let drm = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n";
        let live = "#EXTM3U\n#EXTINF:4,\na.ts\n";
        let server = TestServer::start(ServerBehaviour {
            routes: vec![
                ("/drm.m3u8".to_owned(), drm.as_bytes().to_vec()),
                ("/live.m3u8".to_owned(), live.as_bytes().to_vec()),
                ("/movie.mpd".to_owned(), b"<MPD/>".to_vec()),
            ],
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        for (path, code) in [
            ("drm.m3u8", "protected_stream"),
            ("live.m3u8", "live_stream"),
            ("movie.mpd", "stream_error"),
        ] {
            let task = harness.service.create_task(&server.url(path)).unwrap();
            let _ = harness
                .service
                .start_task(&task.id, &harness.destination)
                .await;
            let row = harness.storage.get_download(&task.id).unwrap().unwrap();
            assert_eq!(row.status, DownloadStatus::Failed, "{path}");
            assert_eq!(row.error_code.as_deref(), Some(code), "{path}");
        }
    }

    #[tokio::test]
    async fn the_qualities_of_a_stream_can_be_listed_before_downloading() {
        let (behaviour, _) = stream_server(None);
        let server = TestServer::start(behaviour).await;
        let harness = harness();
        let variants = harness
            .service
            .stream_variants(&server.url("show/episode-12/master.m3u8"))
            .await
            .unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[1].height, Some(720));

        harness.service.set_stream_max_height(Some(480)).unwrap();
        assert_eq!(harness.service.stream_max_height(), Some(480));
    }

    /// Every file in `directory`, served under `prefix`.
    fn routes_from(directory: &Path, prefix: &str) -> Vec<(String, Vec<u8>)> {
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    format!("{prefix}/{}", entry.file_name().to_string_lossy()),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    async fn run_ffmpeg(ffmpeg: &crate::ffmpeg::Ffmpeg, directory: &Path, arguments: &[&str]) {
        let status = tokio::process::Command::new(ffmpeg.path())
            .current_dir(directory)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(arguments)
            .status()
            .await
            .unwrap();
        assert!(status.success(), "ffmpeg {arguments:?}");
    }

    #[tokio::test]
    async fn an_hls_stream_with_separate_sound_is_joined_into_one_mp4() {
        let Some(ffmpeg) = crate::ffmpeg::tests::real_ffmpeg() else {
            return;
        };
        let media = tempdir().unwrap();
        run_ffmpeg(
            &ffmpeg,
            media.path(),
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=64x48:rate=10",
                "-c:v",
                "mpeg2video",
                "-f",
                "hls",
                "-hls_time",
                "1",
                "-hls_list_size",
                "0",
                "-hls_segment_filename",
                "v%d.ts",
                "video.m3u8",
            ],
        )
        .await;
        run_ffmpeg(
            &ffmpeg,
            media.path(),
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
                "-c:a",
                "aac",
                "-f",
                "hls",
                "-hls_time",
                "1",
                "-hls_list_size",
                "0",
                "-hls_segment_filename",
                "a%d.ts",
                "audio.m3u8",
            ],
        )
        .await;
        std::fs::write(
            media.path().join("master.m3u8"),
            "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"en\",DEFAULT=YES,URI=\"audio.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=300000,RESOLUTION=64x48,AUDIO=\"aud\"\nvideo.m3u8\n",
        )
        .unwrap();
        let server = TestServer::start(ServerBehaviour {
            routes: routes_from(media.path(), "/lecture-09"),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let task = harness
            .service
            .create_task(&server.url("lecture-09/master.m3u8"))
            .unwrap();

        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(finished.status, DownloadStatus::Completed);
        let path = PathBuf::from(finished.destination_path.unwrap());
        assert!(path.to_string_lossy().ends_with("lecture-09.mp4"));
        let streams = crate::ffmpeg::tests::probe_streams(&path).await;
        assert!(
            streams.contains("video") && streams.contains("audio"),
            "{streams}"
        );
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "lecture-09.mp4")
            .collect();
        assert!(
            leftovers.is_empty(),
            "track files are cleaned up: {leftovers:?}"
        );
    }

    #[tokio::test]
    async fn a_dash_stream_is_downloaded_and_joined() {
        let Some(ffmpeg) = crate::ffmpeg::tests::real_ffmpeg() else {
            return;
        };
        let media = tempdir().unwrap();
        run_ffmpeg(
            &ffmpeg,
            media.path(),
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=64x48:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=3",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-c:a",
                "aac",
                "-f",
                "dash",
                "-seg_duration",
                "1",
                "-use_timeline",
                "1",
                "-use_template",
                "1",
                "manifest.mpd",
            ],
        )
        .await;
        let server = TestServer::start(ServerBehaviour {
            routes: routes_from(media.path(), "/talk"),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let task = harness
            .service
            .create_task(&server.url("talk/manifest.mpd"))
            .unwrap();

        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(
            finished.status,
            DownloadStatus::Completed,
            "{:?}",
            finished.error_message
        );
        let path = PathBuf::from(finished.destination_path.unwrap());
        assert!(path.to_string_lossy().ends_with(".mp4"));
        let streams = crate::ffmpeg::tests::probe_streams(&path).await;
        assert!(
            streams.contains("video") && streams.contains("audio"),
            "{streams}"
        );
    }

    #[tokio::test]
    async fn a_transport_stream_is_rewrapped_as_mp4_when_asked() {
        let Some(ffmpeg) = crate::ffmpeg::tests::real_ffmpeg() else {
            return;
        };
        let media = tempdir().unwrap();
        run_ffmpeg(
            &ffmpeg,
            media.path(),
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=64x48:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
                "-c:v",
                "mpeg2video",
                "-c:a",
                "aac",
                "-f",
                "hls",
                "-hls_time",
                "1",
                "-hls_list_size",
                "0",
                "-hls_segment_filename",
                "s%d.ts",
                "index.m3u8",
            ],
        )
        .await;
        let server = TestServer::start(ServerBehaviour {
            routes: routes_from(media.path(), "/clip"),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();

        let task = harness
            .service
            .create_task(&server.url("clip/index.m3u8"))
            .unwrap();
        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        let path = PathBuf::from(finished.destination_path.unwrap());
        assert!(
            path.to_string_lossy().ends_with("clip.mp4"),
            "{}",
            path.display()
        );
        let streams = crate::ffmpeg::tests::probe_streams(&path).await;
        assert!(streams.contains("video") && streams.contains("audio"));

        harness.service.set_stream_prefer_mp4(false).unwrap();
        let task = harness
            .service
            .create_task(&server.url("clip/index.m3u8"))
            .unwrap();
        let finished = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert!(finished.destination_path.unwrap().ends_with(".ts"));
    }

    #[tokio::test]
    async fn a_video_page_without_ytdlp_fails_with_a_reason() {
        let harness = harness();
        harness
            .storage
            .set_setting(SETTING_YTDLP_PATH, "/nowhere/yt-dlp")
            .unwrap();
        assert!(harness.service.ytdlp().is_none());
        let task = harness
            .service
            .create_task("https://www.youtube.com/watch?v=abc")
            .unwrap();
        let _ = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await;
        let row = harness.storage.get_download(&task.id).unwrap().unwrap();
        assert_eq!(row.status, DownloadStatus::Failed);
        assert_eq!(row.error_code.as_deref(), Some("needs_ytdlp"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_video_page_is_downloaded_with_ytdlp_and_moved_out_of_its_folder() {
        use std::os::unix::fs::PermissionsExt;
        let harness = harness();
        let tools = tempfile::tempdir().unwrap();
        let script = tools.path().join("yt-dlp");
        // A stand-in for yt-dlp that writes where it is told (`--paths`).
        std::fs::write(
            &script,
            "#!/bin/sh
while [ \"$1\" != \"--paths\" ]; do shift; done
w=\"$2\"
mkdir -p \"$w\"
\
             echo 'RATATOSK|4|8|NA|2.0|2|My clip'\nprintf abcdefgh > \"$w/My clip [abc].mp4\"\n\
             echo 'RATATOSK|8|8|NA|NA|0|My clip'\necho \"RATATOSK_FILE|$w/My clip [abc].mp4\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        harness.service.set_ytdlp_path(Some(&script)).unwrap();

        let task = harness.service.create_task("https://youtu.be/abc").unwrap();
        let record = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await
            .unwrap();
        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(record.filename.as_deref(), Some("My clip [abc].mp4"));
        let path = PathBuf::from(record.destination_path.unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"abcdefgh");
        assert_eq!(path.parent().unwrap(), harness.destination.as_path());
        assert!(
            !harness
                .destination
                .join(format!("{}{}", crate::ytdlp::WORK_DIR_PREFIX, task.id))
                .exists(),
            "the working folder is removed"
        );
    }

    #[tokio::test]
    async fn without_ffmpeg_separate_sound_is_refused_with_a_reason() {
        let media = "#EXTM3U\n#EXTINF:1,\nv0.ts\n#EXT-X-ENDLIST\n";
        let master = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"en\",URI=\"audio.m3u8\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=1,AUDIO=\"a\"\nvideo.m3u8\n";
        let server = TestServer::start(ServerBehaviour {
            routes: vec![
                ("/s/master.m3u8".to_owned(), master.as_bytes().to_vec()),
                ("/s/video.m3u8".to_owned(), media.as_bytes().to_vec()),
                ("/s/audio.m3u8".to_owned(), media.as_bytes().to_vec()),
            ],
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        // A configured path that does not exist means "no FFmpeg".
        harness
            .storage
            .set_setting(SETTING_FFMPEG_PATH, "/nowhere/ffmpeg")
            .unwrap();
        assert!(harness.service.ffmpeg().is_none());

        let task = harness
            .service
            .create_task(&server.url("s/master.m3u8"))
            .unwrap();
        let _ = harness
            .service
            .start_task(&task.id, &harness.destination)
            .await;
        let row = harness.storage.get_download(&task.id).unwrap().unwrap();
        assert_eq!(row.error_code.as_deref(), Some("needs_muxing"));
    }

    #[test]
    fn a_quality_can_be_named_in_the_link() {
        assert_eq!(
            quality_from_link("https://x.test/master.m3u8#rud-quality=720"),
            Some(720)
        );
        assert_eq!(quality_from_link("https://x.test/master.m3u8"), None);
        assert_eq!(
            quality_from_link("https://x.test/a.mpd#other&rud-quality=0"),
            None
        );
    }

    #[test]
    fn an_ffmpeg_path_must_be_an_existing_file() {
        let harness = harness();
        assert!(
            harness
                .service
                .set_ffmpeg_path(Some(Path::new("relative/ffmpeg")))
                .is_err()
        );
        assert!(
            harness
                .service
                .set_ffmpeg_path(Some(Path::new("/nowhere/ffmpeg")))
                .is_err()
        );
        harness.service.set_ffmpeg_path(None).unwrap();
        assert_eq!(harness.service.ffmpeg_path_setting(), None);
    }

    fn sha256_hex(body: &[u8]) -> String {
        let directory = tempdir().unwrap();
        let path = directory.path().join("body");
        std::fs::write(&path, body).unwrap();
        crate::postprocess::hash_file(&path, crate::postprocess::HashAlgorithm::Sha256).unwrap()
    }

    /// Waits for the background after-download steps to finish.
    async fn await_checks_done(storage: &Storage, id: &str) -> dm_storage::DownloadChecks {
        for _ in 0..400 {
            if let Some(checks) = storage.get_download_checks(id).unwrap()
                && checks.state == "done"
            {
                return checks;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("after-download steps never finished");
    }

    #[tokio::test]
    async fn a_download_is_verified_against_its_expected_checksum() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let created = harness
            .service
            .create_task(&server.url("file.bin"))
            .unwrap();
        let expected = sha256_hex(DEFAULT_BODY);
        harness
            .service
            .set_expected_checksum(
                &created.id,
                Some(&format!("SHA256: {}", expected.to_uppercase())),
            )
            .await
            .unwrap();

        harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        let checks = await_checks_done(&harness.storage, &created.id).await;
        assert_eq!(checks.integrity.as_deref(), Some("verified"));
        assert_eq!(checks.actual_checksum.as_deref(), Some(expected.as_str()));
        assert_eq!(checks.algorithm.as_deref(), Some("sha256"));
    }

    #[tokio::test]
    async fn a_wrong_checksum_leaves_a_notice_and_keeps_the_file() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let created = harness
            .service
            .create_task(&server.url("file.bin"))
            .unwrap();
        let record = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();
        let mut events = harness.service.subscribe_post_process();

        // Set after the download finished: checked straight away.
        let checks = harness
            .service
            .set_expected_checksum(&created.id, Some(&"0".repeat(32)))
            .await
            .unwrap();

        assert_eq!(checks.integrity.as_deref(), Some("mismatch"));
        assert_eq!(checks.algorithm.as_deref(), Some("md5"));
        assert_eq!(events.recv().await.unwrap(), created.id);
        let task = harness.storage.get_download(&created.id).unwrap().unwrap();
        assert_eq!(task.status, DownloadStatus::Completed);
        assert_eq!(task.error_code.as_deref(), Some(INTEGRITY_FAILED_CODE));
        assert!(Path::new(record.destination_path.as_ref().unwrap()).exists());
    }

    #[tokio::test]
    async fn a_checksum_that_is_not_one_is_refused() {
        let harness = harness();
        let created = harness
            .service
            .create_task("https://example.com/a.bin")
            .unwrap();
        assert!(matches!(
            harness
                .service
                .set_expected_checksum(&created.id, Some("not a hash"))
                .await,
            Err(DownloadServiceError::InvalidChecksum)
        ));
        let cleared = harness
            .service
            .set_expected_checksum(&created.id, None)
            .await
            .unwrap();
        assert_eq!(cleared.expected_checksum, None);
    }

    #[tokio::test]
    async fn finished_archives_are_unpacked_when_turned_on() {
        let scratch = tempdir().unwrap();
        let archive = scratch.path().join("bundle.zip");
        crate::postprocess::tests::zip_with(&archive, &[("docs/readme.txt", b"hello")]);
        let server = TestServer::start(ServerBehaviour {
            body: std::fs::read(&archive).unwrap(),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        harness
            .service
            .set_post_process_settings(&PostProcessSettings {
                extract_zip: true,
                ..PostProcessSettings::default()
            })
            .unwrap();
        let created = harness
            .service
            .create_task(&server.url("bundle.zip"))
            .unwrap();
        let record = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        let checks = await_checks_done(&harness.storage, &created.id).await;
        let folder = PathBuf::from(checks.extracted_to.expect("unpacked"));
        assert_eq!(
            std::fs::read(folder.join("docs").join("readme.txt")).unwrap(),
            b"hello"
        );
        assert!(Path::new(record.destination_path.as_ref().unwrap()).exists());
        assert_eq!(checks.integrity, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_users_command_runs_on_the_finished_file() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        harness
            .service
            .set_post_process_settings(&PostProcessSettings {
                hash_always: true,
                command: Some("cp {file} {folder}/copy-of-{name}".to_owned()),
                ..PostProcessSettings::default()
            })
            .unwrap();
        let created = harness
            .service
            .create_task(&server.url("file.bin"))
            .unwrap();
        let record = harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        let checks = await_checks_done(&harness.storage, &created.id).await;
        assert_eq!(checks.command_error, None);
        assert_eq!(checks.integrity, None);
        assert_eq!(checks.actual_checksum, Some(sha256_hex(DEFAULT_BODY)));
        let file = PathBuf::from(record.destination_path.unwrap());
        let copy = file.parent().unwrap().join(format!(
            "copy-of-{}",
            file.file_name().unwrap().to_string_lossy()
        ));
        assert_eq!(std::fs::read(copy).unwrap(), DEFAULT_BODY);
    }

    #[test]
    fn after_download_settings_round_trip_and_refuse_broken_commands() {
        let harness = harness();
        assert_eq!(
            harness.service.post_process_settings(),
            PostProcessSettings::default()
        );
        let settings = PostProcessSettings {
            hash_always: true,
            extract_zip: true,
            scan: true,
            command: Some("\"C:\\Tools\\check.exe\" {file}".to_owned()),
        };
        harness
            .service
            .set_post_process_settings(&settings)
            .unwrap();
        assert_eq!(harness.service.post_process_settings(), settings);
        assert!(matches!(
            harness
                .service
                .set_post_process_settings(&PostProcessSettings {
                    command: Some("\"unclosed {file}".to_owned()),
                    ..PostProcessSettings::default()
                }),
            Err(DownloadServiceError::InvalidCommand)
        ));
    }

    #[tokio::test]
    async fn statistics_count_todays_download_and_its_traffic() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let created = harness
            .service
            .create_task(&server.url("stats.bin"))
            .unwrap();
        harness
            .service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        let stats = harness.service.download_stats(7).unwrap();

        assert_eq!(stats.days.len(), 7);
        let today = stats.days.last().unwrap();
        assert_eq!(today.completed, 1);
        assert_eq!(
            today.domestic_bytes + today.international_bytes,
            DEFAULT_BODY.len() as u64
        );
        assert_eq!(stats.all_completed, 1);
        assert_eq!(stats.top_hosts[0].name, "127.0.0.1");
        assert_eq!(stats.extensions[0].name, "bin");
    }

    #[tokio::test]
    async fn the_list_exports_as_links_or_a_spreadsheet() {
        let harness = harness();
        let first = harness
            .service
            .create_task("https://a.example/one.iso")
            .unwrap();
        harness
            .service
            .create_task("https://b.example/two.zip")
            .unwrap();

        let links = harness
            .service
            .export_downloads(ExportFormat::Links, None)
            .unwrap();
        assert_eq!(links.lines().count(), 2);
        let only = harness
            .service
            .export_downloads(ExportFormat::Csv, Some(std::slice::from_ref(&first.id)))
            .unwrap();
        assert_eq!(only.lines().count(), 2);
        assert!(only.contains("https://a.example/one.iso"));
    }

    #[tokio::test]
    async fn the_diagnostics_report_hides_links_and_the_proxy() {
        let harness = harness();
        let created = harness
            .service
            .create_task("https://files.example/secret.iso?token=abc")
            .unwrap();
        harness
            .storage
            .record_notice(
                &created.id,
                "http_403",
                "403 at https://files.example/secret.iso?token=abc",
            )
            .unwrap();
        let mut network = harness.service.network_settings();
        network.mode = crate::network::ProxyMode::Manual;
        network.proxy_url = Some("socks5://127.0.0.1:10808".to_owned());
        harness.service.set_network_settings(&network).unwrap();

        let report = harness
            .service
            .diagnostics_report(
                "1.2.3",
                Path::new("/home/someone/Downloads"),
                Some("/home/someone"),
            )
            .await
            .unwrap();

        assert!(report.contains("Application: 1.2.3"));
        assert!(report.contains("integrity ok"));
        assert!(report.contains("manual (socks5)"));
        assert!(report.contains("[http_403] files.example: 403 at https://files.example/…"));
        assert!(report.contains("Download folder: ~/Downloads"));
        assert!(!report.contains("token=abc"));
        assert!(!report.contains("10808"));
        assert!(!report.contains("someone"));
    }

    #[tokio::test]
    async fn a_connection_check_reports_what_a_download_would_find() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let harness = harness();
        let mut network = harness.service.network_settings();
        network.mode = crate::network::ProxyMode::Off;
        harness.service.set_network_settings(&network).unwrap();

        let check = harness
            .service
            .connection_check(&server.url("probe.bin"))
            .await;
        assert!(check.reachable, "{check:?}");
        assert_eq!(check.route, "direct");
        assert_eq!(check.total_bytes, Some(DEFAULT_BODY.len() as u64));
        assert!(check.range_supported);

        let failed = harness
            .service
            .connection_check("http://127.0.0.1:9/nothing?key=secret")
            .await;
        assert!(!failed.reachable);
        assert!(!failed.error.unwrap_or_default().contains("secret"));
    }

    fn patterned_body(length: usize) -> Vec<u8> {
        (0..length).map(|index| (index % 251) as u8).collect()
    }

    fn fault_tolerant_service(harness: &Harness) -> DownloadService {
        harness
            .service
            .clone()
            .with_segment_connections(4)
            .with_segmented_threshold(1)
            .with_segment_sizes(64 * 1024, 32 * 1024)
            .with_stall_timeout(Duration::from_millis(300))
            .unwrap()
    }

    #[tokio::test]
    async fn a_connection_that_goes_silent_is_replaced_instead_of_hanging() {
        let body = patterned_body(512 * 1024);
        let server = TestServer::start(ServerBehaviour {
            body: body.clone(),
            chunk_size: 16 * 1024,
            stall_first_bodies: 1,
            stall_for: Duration::from_secs(120),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = fault_tolerant_service(&harness);
        let created = service.create_task(&server.url("silent.bin")).unwrap();

        let record = tokio::time::timeout(
            Duration::from_secs(20),
            service.start_task(&created.id, &harness.destination),
        )
        .await
        .expect("the download hung on a silent connection")
        .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(
            std::fs::read(record.destination_path.unwrap()).unwrap(),
            body
        );
    }

    #[tokio::test]
    async fn a_dropped_connection_costs_only_its_own_range() {
        let body = patterned_body(512 * 1024);
        let server = TestServer::start(ServerBehaviour {
            body: body.clone(),
            chunk_size: 16 * 1024,
            drop_first_bodies: 3,
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = fault_tolerant_service(&harness);
        let created = service.create_task(&server.url("flaky.bin")).unwrap();

        let record = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        // Finished in the same run: no whole-download retry was needed.
        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(record.attempts, 0);
        assert_eq!(
            std::fs::read(record.destination_path.unwrap()).unwrap(),
            body
        );
        assert_eq!(
            harness
                .storage
                .get_download(&created.id)
                .unwrap()
                .unwrap()
                .downloaded_bytes,
            body.len() as u64
        );
    }

    #[tokio::test]
    async fn a_connection_that_keeps_delivering_something_is_kept_going() {
        // Every connection drops after one chunk, but each chunk is kept, so
        // the download is still moving and should finish.
        let body = patterned_body(256 * 1024);
        let server = TestServer::start(ServerBehaviour {
            body: body.clone(),
            chunk_size: 16 * 1024,
            drop_first_bodies: usize::MAX,
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = fault_tolerant_service(&harness);
        let created = service.create_task(&server.url("trickle.bin")).unwrap();

        let record = service
            .start_task(&created.id, &harness.destination)
            .await
            .unwrap();

        assert_eq!(record.status, DownloadStatus::Completed);
        assert_eq!(
            std::fs::read(record.destination_path.unwrap()).unwrap(),
            body
        );
    }

    #[tokio::test]
    async fn a_server_that_sends_nothing_is_not_retried_forever() {
        let server = TestServer::start(ServerBehaviour {
            body: patterned_body(512 * 1024),
            chunk_size: 16 * 1024,
            truncate_after: Some(0),
            ..ServerBehaviour::default()
        })
        .await;
        let harness = harness();
        let service = fault_tolerant_service(&harness);
        let created = service.create_task(&server.url("broken.bin")).unwrap();

        let result = tokio::time::timeout(
            Duration::from_secs(20),
            service.start_task(&created.id, &harness.destination),
        )
        .await
        .expect("kept retrying a broken server");

        let status = match result {
            Ok(record) => record.status,
            Err(_) => {
                harness
                    .storage
                    .get_download(&created.id)
                    .unwrap()
                    .unwrap()
                    .status
            }
        };
        assert_ne!(status, DownloadStatus::Completed);
        assert!(
            server.request_count() < 40,
            "{} requests",
            server.request_count()
        );
    }
}
