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
  errorCode: string | null;
  errorMessage: string | null;
};

export type DownloadPriority = DownloadListItem["priority"];

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
