export type DownloadListItem = {
  id: string;
  sourceUrl: string;
  resolvedUrl: string | null;
  filename: string | null;
  destinationPath: string | null;
  mimeType: string | null;
  etag: string | null;
  lastModified: string | null;
  rangeSupported: boolean | null;
  totalBytes: number | null;
  downloadedBytes: number;
  status: string;
  queueId: string | null;
  priority: "low" | "normal" | "high" | "very_high";
  queuePosition: number | null;
  createdAt: number;
  startedAt: number | null;
  completedAt: number | null;
  attempts: number;
  retryAt: number | null;
  errorCode: string | null;
  errorMessage: string | null;
};

/// What a task can be asked to do right now. Derived from the same rules the
/// backend enforces, so the UI never offers an action that would be refused.
export type TaskAction =
  | "start"
  | "pause"
  | "resume"
  | "cancel"
  | "retry"
  | "restart";

export type DownloadPriority = DownloadListItem["priority"];

/// Live transfer measurements. These come from the engine with every progress
/// event and are deliberately not persisted: they describe a transfer that is
/// happening right now, not the stored history row.
export type TransferMetrics = {
  bytesPerSecond: number | null;
  etaSeconds: number | null;
  activeConnections: number | null;
  maxConnections: number | null;
  adaptiveReason: string | null;
};

export type TransferMetricsMap = Record<string, TransferMetrics>;

export type DownloadQueue = {
  id: string;
  name: string;
  enabled: boolean;
  state: "running" | "stopped";
  sortOrder: number;
  maxConcurrent: number;
  maxConcurrentPerHost: number | null;
  defaultPriority: DownloadPriority;
  createdAt: number;
  updatedAt: number;
};

export type QueueSchedule = {
  queueId: string;
  enabled: boolean;
  kind: "once" | "daily" | "weekdays" | "repeating";
  startAt: number;
  stopAt: number | null;
  weekdaysMask: number;
  intervalSeconds: number | null;
  completionAction:
    | "none"
    | "notify"
    | "exit_app"
    | "sleep"
    | "hibernate"
    | "shutdown";
  preventSleep: boolean;
  updatedAt: number;
  /// Local wall-clock window in minutes after midnight (daily/weekdays).
  windowStartMinute: number | null;
  windowEndMinute: number | null;
};

export type CompletionAction = QueueSchedule["completionAction"];

/// Engine-wide preferences shown in Settings.
export type DownloadSettings = {
  defaultDirectory: string | null;
  systemDirectory: string | null;
  /// Bytes per second for all downloads together; null is unlimited.
  globalSpeedLimit: number | null;
  preventSleep: boolean;
  /// Most connections one download may open.
  maxConnections: number;
};

export type ProxyMode = "off" | "system" | "manual" | "pac";

/// How downloads reach the network. Host lists are one domain per line.
export type NetworkSettings = {
  mode: ProxyMode;
  proxyUrl: string | null;
  /// The setup script (PAC), for the `pac` mode.
  pacUrl: string | null;
  /// The setup script Windows itself uses, if any. Read only.
  systemPacUrl: string | null;
  directHosts: string;
  domesticDirect: boolean;
  domesticHosts: string;
};

/// Domestic and international traffic, in bytes.
export type TrafficSummary = {
  periodStart: string;
  explicitPeriod: boolean;
  periodDomesticBytes: number;
  periodInternationalBytes: number;
  todayDomesticBytes: number;
  todayInternationalBytes: number;
  monthDomesticBytes: number;
  monthInternationalBytes: number;
  internationalQuota: number | null;
};

/// A power or exit action a finished queue scheduled, with its cancel window.
export type CompletionActionEvent = {
  id: number;
  queueName: string;
  action: string;
  dueAt: number;
  state: "pending" | "cancelled" | "skipped" | "running";
  message: string | null;
};

export type DownloadCategory = {
  id: string;
  name: string;
  extensions: string[];
  mimePatterns: string[];
  defaultDirectory: string | null;
  hostPatterns: string[];
  priority: DownloadPriority;
  queueId: string | null;
};

export type DownloadRule = {
  id: string;
  name: string;
  enabled: boolean;
  sortOrder: number;
  domain: string | null;
  urlPattern: string | null;
  extension: string | null;
  mimePattern: string | null;
  minSize: number | null;
  maxSize: number | null;
  categoryId: string | null;
  destinationDirectory: string | null;
  queueId: string | null;
  priority: DownloadPriority | null;
  maxConnections: number | null;
  maxHostConcurrency: number | null;
  speedCap: number | null;
  browserTakeoverAllowed: boolean | null;
};

/// Appearance and window behaviour, stored by the backend.
export type UiPreferences = {
  language: "fa" | "en";
  theme: "dark" | "light" | "system";
  closeToTray: boolean;
};

/// A quality an HLS stream offers.
export type StreamVariant = {
  uri: string;
  bandwidth: number | null;
  width: number | null;
  height: number | null;
  /// Its sound is a separate track, which this version cannot join.
  needsMuxing: boolean;
};

/// What a video page holds, looked up with yt-dlp before adding it.
export type VideoProbe = {
  title: string | null;
  durationSeconds: number | null;
  /// Highest first; empty for a playlist.
  qualities: { height: number; bytes: number | null }[];
  audioBytes: number | null;
  entries: { url: string; title: string | null }[];
  isPlaylist: boolean;
};

/// Engine behaviour: fresh links continue stopped downloads, gentle hosts,
/// automatic stream quality.
export type EngineSettings = {
  autoAdoptLinks: boolean;
  politeHosts: string;
  streamMaxHeight: number | null;
  /// Rewrap .ts streams as .mp4 when FFmpeg is available.
  streamPreferMp4: boolean;
};

/// Where FFmpeg is, as far as the engine can tell.
export type FfmpegStatus = {
  configuredPath: string | null;
  foundPath: string | null;
  version: string | null;
};

/** What happened after a download finished. */
export type DownloadChecks = {
  state: "idle" | "running" | "done";
  expectedChecksum: string | null;
  algorithm: "md5" | "sha1" | "sha256" | null;
  actualChecksum: string | null;
  integrity: "verified" | "mismatch" | "error" | null;
  scan: "clean" | "threat" | "unavailable" | null;
  scanDetail: string | null;
  extractedTo: string | null;
  extractError: string | null;
  commandError: string | null;
};

/** After-download steps for every finished download. */
export type PostProcessSettings = {
  hashAlways: boolean;
  extractZip: boolean;
  scan: boolean;
  command: string;
  scanAvailable: boolean;
};

export type ActivityDay = {
  day: string;
  domesticBytes: number;
  internationalBytes: number;
  completed: number;
};

export type NamedTotal = { name: string; count: number; bytes: number };

export type DownloadStats = {
  days: ActivityDay[];
  periodCompleted: number;
  periodDomesticBytes: number;
  periodInternationalBytes: number;
  allCompleted: number;
  allCompletedBytes: number;
  failed: number;
  active: number;
  topHosts: NamedTotal[];
  extensions: NamedTotal[];
  largest: NamedTotal | null;
};

export type BackupInfo = { schemaVersion: number; downloads: number; queues: number; bytes: number };

export type RestoreOutcome = { restored: boolean; keptCopy: string | null; reason: string | null };

export type ConnectionCheck = {
  host: string;
  route: "direct" | "proxy" | "system";
  reachable: boolean;
  elapsedMs: number;
  finalHost: string | null;
  filename: string | null;
  totalBytes: number | null;
  rangeSupported: boolean;
  error: string | null;
};
