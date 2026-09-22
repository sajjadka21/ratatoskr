import {
  Archive,
  File,
  Image,
  Music,
  MoreHorizontal,
  Video,
} from "lucide-react";

import type {
  DownloadListItem,
  TransferMetrics,
} from "../../types/download";

import {
  formatBytes,
  formatDuration,
  formatHost,
  formatRate,
} from "../../utils/format";

import "./DownloadRow.css";

type DownloadRowProps = {
  item: DownloadListItem;
  metrics?: TransferMetrics;
  queueName?: string;
  selected?: boolean;
  onSelect?: () => void;
  onContextMenu?: (
    item: DownloadListItem,
    x: number,
    y: number,
  ) => void;
};

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
  metrics,
  queueName,
  selected = false,
  onSelect,
  onContextMenu,
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

  const domain = formatHost(
    item.resolvedUrl ?? item.sourceUrl,
  );

  const isTransferring = status === "downloading";

  const rate = isTransferring
    ? formatRate(metrics?.bytesPerSecond ?? null)
    : null;

  const remaining = isTransferring
    ? formatDuration(metrics?.etaSeconds ?? null)
    : null;

  return (
    <article
      className={`download-row download-row--${status} ${
        selected ? "download-row--selected" : ""
      }`}
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onContextMenu={(event) => {
        event.preventDefault();

        onContextMenu?.(
          item,
          event.clientX,
          event.clientY,
        );
      }}
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
            {rate ? (
              <span className="download-row__rate">
                {rate}
              </span>
            ) : null}

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

          {remaining ? (
            <span className="download-row__eta">
              {remaining} left
            </span>
          ) : null}

          {item.queueId ? (
            <span className="download-row__queue-hint">
              {queueName ?? item.queueId} · {item.priority.replace("_", " ")}
            </span>
          ) : null}

          {item.errorMessage ? (
            <span
              className={`download-row__message download-row__message--${
                status === "failed" ? "error" : "notice"
              }`}
            >
              {item.errorMessage}
            </span>
          ) : null}
        </div>
      </div>

      <div className="download-row__status-area">
        <button
          type="button"
          className="download-row__menu-button"
          aria-label="Download actions"
          onClick={(event) => {
            event.stopPropagation();

            const rect =
              event.currentTarget.getBoundingClientRect();

            onContextMenu?.(
              item,
              rect.right,
              rect.bottom + 6,
            );
          }}
        >
          <MoreHorizontal
            size={17}
            strokeWidth={2}
          />
        </button>

        <span
          className={`download-row__status download-row__status--${status}`}
        >
          {statusLabel(status)}
        </span>
      </div>
    </article>
  );
}





