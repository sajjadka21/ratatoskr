export type DownloadListItem = {
  id: string;
  sourceUrl: string;
  resolvedUrl: string | null;
  filename: string | null;
  destinationPath: string | null;
  mimeType: string | null;
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
  | "retry";

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
