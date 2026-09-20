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
  createdAt: number;
  startedAt: number | null;
  completedAt: number | null;
  errorCode: string | null;
  errorMessage: string | null;
};
