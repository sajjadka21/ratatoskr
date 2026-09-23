import {
  Archive,
  File,
  Image,
  Music,
  MoreHorizontal,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  Video,
} from "lucide-react";

import type {
  DownloadListItem,
  TaskAction,
  TransferMetrics,
} from "../../types/download";
import { memo, type SyntheticEvent } from "react";

import {
  ACTION_LABELS,
  primaryAction,
} from "../../utils/taskActions";

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
  /// Current wall clock in seconds, supplied by the page so a retry
  /// countdown ticks without every row owning a timer.
  nowSeconds?: number;
  selected?: boolean;
  /// Position in the visible list, handed back on selection so the parent
  /// can pass one stable handler to every row.
  index: number;
  onAction?: (
    item: DownloadListItem,
    action: TaskAction,
  ) => void;
  onSelect?: (
    item: DownloadListItem,
    index: number,
    event: SyntheticEvent,
  ) => void;
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

const ACTION_ICONS = {
  start: Play,
  resume: Play,
  pause: Pause,
  retry: RotateCcw,
  cancel: RotateCcw,
  restart: RefreshCw,
} as const;

/// Memoised: a progress event replaces only the row that moved, so the rest
/// of a long list skips rendering as long as the parent's handlers are stable.
export const DownloadRow = memo(function DownloadRow({
  item,
  metrics,
  queueName,
  nowSeconds,
  selected = false,
  index,
  onAction,
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

  const retryIn =
    status === "retrying" &&
    item.retryAt !== null &&
    nowSeconds !== undefined
      ? formatDuration(
          Math.max(0, item.retryAt - nowSeconds),
        )
      : null;

  const action = primaryAction(item);
  const ActionIcon = action ? ACTION_ICONS[action] : null;

  return (
    <article
      className={`download-row download-row--${status} ${
        selected ? "download-row--selected" : ""
      }`}
      role="button"
      tabIndex={0}
      onClick={(event) => onSelect?.(item, index, event)}
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
          onSelect?.(item, index, event);
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

          {retryIn ? (
            <span className="download-row__eta">
              Attempt {item.attempts + 1} in {retryIn}
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
        {action && ActionIcon ? (
          <button
            type="button"
            className="download-row__action"
            aria-label={`${ACTION_LABELS[action]} download`}
            title={ACTION_LABELS[action]}
            onClick={(event) => {
              event.stopPropagation();
              onAction?.(item, action);
            }}
          >
            <ActionIcon size={15} strokeWidth={2.2} />
          </button>
        ) : null}

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
});
