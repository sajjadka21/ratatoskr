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
