import {
  Archive,
  File,
  Image,
  Music,
  Video,
} from "lucide-react";

import type { DownloadListItem } from "../../types/download";

import "./DownloadRow.css";

type DownloadRowProps = {
  item: DownloadListItem;
  selected?: boolean;
  onSelect?: () => void;
};

function formatBytes(bytes: number): string {
  if (bytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];

  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );

  const value = bytes / 1024 ** index;

  return `${value.toFixed(index === 0 ? 0 : 2)} ${units[index]}`;
}

function getDomain(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

function getFileIcon(item: DownloadListItem) {
  const mime = item.mimeType?.toLowerCase() ?? "";
  const filename = item.filename?.toLowerCase() ?? "";

  if (
    mime.startsWith("image/") ||
    /\.(png|jpg|jpeg|gif|webp|svg)$/.test(filename)
  ) {
    return Image;
  }

  if (
    mime.startsWith("video/") ||
    /\.(mp4|mkv|mov|avi|webm)$/.test(filename)
  ) {
    return Video;
  }

  if (
    mime.startsWith("audio/") ||
    /\.(mp3|wav|flac|aac|m4a|ogg)$/.test(filename)
  ) {
    return Music;
  }

  if (
    /\.(zip|rar|7z|tar|gz|bz2|xz)$/.test(filename)
  ) {
    return Archive;
  }

  return File;
}

function statusLabel(status: string): string {
  const labels: Record<string, string> = {
    created: "Created",
    probing: "Checking",
    queued: "Queued",
    downloading: "Downloading",
    paused: "Paused",
    retrying: "Retrying",
    finalizing: "Finalizing",
    completed: "Completed",
    failed: "Failed",
    cancelled: "Cancelled",
  };

  return labels[status.toLowerCase()] ?? status;
}

export function DownloadRow({
  item,
  selected = false,
  onSelect,
}: DownloadRowProps) {
  const status = item.status.toLowerCase();

  const FileIcon = getFileIcon(item);

  const percent =
    item.totalBytes && item.totalBytes > 0
      ? Math.min(
          100,
          (item.downloadedBytes / item.totalBytes) * 100,
        )
      : status === "completed"
        ? 100
        : null;

  const name =
    item.filename ??
    item.resolvedUrl ??
    item.sourceUrl;

  const domain = getDomain(
    item.resolvedUrl ?? item.sourceUrl,
  );

  return (
    <article
      className={`download-row download-row--${status} ${
        selected ? "download-row--selected" : ""
      }`}
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (
          event.key === "Enter" ||
          event.key === " "
        ) {
          event.preventDefault();
          onSelect?.();
        }
      }}
    >
      <div className="download-row__icon">
        <FileIcon size={21} strokeWidth={1.7} />
      </div>

      <div className="download-row__main">
        <div className="download-row__top">
          <div className="download-row__identity">
            <div className="download-row__name">
              {name}
            </div>

            <div className="download-row__domain">
              {domain}
            </div>
          </div>

          <div className="download-row__numbers">
            {percent !== null && (
              <strong>
                {percent.toFixed(
                  percent === 100 ? 0 : 1,
                )}
                %
              </strong>
            )}
          </div>
        </div>

        <div className="download-row__progress">
          <div
            className="download-row__progress-fill"
            style={{
              width: `${percent ?? 0}%`,
            }}
          />
        </div>

        <div className="download-row__bottom">
          <span>
            {formatBytes(item.downloadedBytes)}
            {item.totalBytes
              ? ` of ${formatBytes(item.totalBytes)}`
              : ""}
          </span>

          {status === "failed" &&
          item.errorMessage ? (
            <span className="download-row__error">
              {item.errorMessage}
            </span>
          ) : null}
        </div>
      </div>

      <div className="download-row__status-area">
        <span
          className={`download-row__status download-row__status--${status}`}
        >
          {statusLabel(status)}
        </span>
      </div>
    </article>
  );
}

