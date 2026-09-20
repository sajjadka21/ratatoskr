import {
  Check,
  Clock3,
  Copy,
  ExternalLink,
  File,
  FolderOpen,
  Link2,
  X,
} from "lucide-react";

import {
  openPath,
  revealItemInDir,
} from "@tauri-apps/plugin-opener";

import {
  useEffect,
  useState,
} from "react";

import type { DownloadListItem } from "../../types/download";

import "./DownloadDetailsPanel.css";

type DownloadDetailsPanelProps = {
  item: DownloadListItem | null;
  onClose: () => void;
};

type CopiedField =
  | "url"
  | "path"
  | null;

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";

  const units = ["B", "KB", "MB", "GB", "TB"];

  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );

  const value = bytes / 1024 ** index;

  return `${value.toFixed(index === 0 ? 0 : 2)} ${units[index]}`;
}

function formatDate(value: number | null): string {
  if (!value) return "—";

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value * 1000));
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

export function DownloadDetailsPanel({
  item,
  onClose,
}: DownloadDetailsPanelProps) {
  const [copiedField, setCopiedField] =
    useState<CopiedField>(null);

  const [actionError, setActionError] =
    useState<string | null>(null);

  useEffect(() => {
    setCopiedField(null);
    setActionError(null);
  }, [item?.id]);

  if (!item) {
    return null;
  }

  const destinationPath = item.destinationPath;
  const status = item.status.toLowerCase();

  const hasFile = Boolean(
    item.destinationPath &&
    status === "completed",
  );

  const percent =
    item.totalBytes && item.totalBytes > 0
      ? Math.min(
          100,
          (item.downloadedBytes / item.totalBytes) * 100,
        )
      : status === "completed"
        ? 100
        : 0;

  async function copyText(
    value: string,
    field: Exclude<CopiedField, null>,
  ) {
    try {
      await navigator.clipboard.writeText(value);

      setActionError(null);
      setCopiedField(field);

      window.setTimeout(() => {
        setCopiedField((current) =>
          current === field ? null : current,
        );
      }, 1400);
    } catch (reason) {
      setActionError(
        `Could not copy: ${String(reason)}`,
      );
    }
  }

  async function handleOpenFile() {
    if (!destinationPath) return;

    try {
      setActionError(null);
      await openPath(destinationPath);
    } catch (reason) {
      setActionError(
        `Could not open file: ${String(reason)}`,
      );
    }
  }

  async function handleRevealFile() {
    if (!destinationPath) return;

    try {
      setActionError(null);
      await revealItemInDir(
        destinationPath,
      );
    } catch (reason) {
      setActionError(
        `Could not show file in folder: ${String(reason)}`,
      );
    }
  }

  return (
    <aside className="download-details">
      <header className="download-details__header">
        <div className="download-details__file-icon">
          <File size={20} />
        </div>

        <div className="download-details__identity">
          <strong>
            {item.filename ?? "Download"}
          </strong>

          <span>
            {item.mimeType ?? "Unknown file type"}
          </span>
        </div>

        <button
          type="button"
          className="download-details__close"
          onClick={onClose}
          aria-label="Close details"
        >
          <X size={17} />
        </button>
      </header>

      <div className="download-details__body">
        <div className="download-details__section-title">
          Details
        </div>

        <div className="download-details__progress">
          <div className="download-details__progress-top">
            <span>{statusLabel(status)}</span>
            <strong>{percent.toFixed(0)}%</strong>
          </div>

          <div className="download-details__track">
            <div
              className={`download-details__fill download-details__fill--${status}`}
              style={{ width: `${percent}%` }}
            />
          </div>

          <div className="download-details__bytes">
            {formatBytes(item.downloadedBytes)}
            {item.totalBytes
              ? ` of ${formatBytes(item.totalBytes)}`
              : ""}
          </div>
        </div>

        <div className="download-details__actions">
          <button
            type="button"
            onClick={() =>
              void handleOpenFile()
            }
            disabled={!hasFile}
          >
            <ExternalLink size={14} />
            Open File
          </button>

          <button
            type="button"
            onClick={() =>
              void handleRevealFile()
            }
            disabled={!hasFile}
          >
            <FolderOpen size={14} />
            Show in Folder
          </button>
        </div>

        <dl className="download-details__properties">
          <div>
            <dt>
              <Link2 size={14} />
              Source
            </dt>

            <dd className="download-details__property-row">
              <span title={item.sourceUrl}>
                {item.sourceUrl}
              </span>

              <button
                type="button"
                onClick={() =>
                  void copyText(
                    item.sourceUrl,
                    "url",
                  )
                }
                aria-label="Copy source URL"
              >
                {copiedField === "url" ? (
                  <Check size={13} />
                ) : (
                  <Copy size={13} />
                )}
              </button>
            </dd>
          </div>

          <div>
            <dt>
              <FolderOpen size={14} />
              Destination
            </dt>

            <dd className="download-details__property-row">
              <span
                title={
                  item.destinationPath ?? ""
                }
              >
                {item.destinationPath ?? "—"}
              </span>

              {item.destinationPath ? (
                <button
                  type="button"
                  onClick={() =>
                    void copyText(
                      item.destinationPath!,
                      "path",
                    )
                  }
                  aria-label="Copy destination path"
                >
                  {copiedField === "path" ? (
                    <Check size={13} />
                  ) : (
                    <Copy size={13} />
                  )}
                </button>
              ) : null}
            </dd>
          </div>

          <div>
            <dt>
              <File size={14} />
              Size
            </dt>

            <dd>
              {item.totalBytes
                ? formatBytes(item.totalBytes)
                : "Unknown"}
            </dd>
          </div>

          <div>
            <dt>
              <Clock3 size={14} />
              Added
            </dt>

            <dd>{formatDate(item.createdAt)}</dd>
          </div>

          <div>
            <dt>Completed</dt>
            <dd>{formatDate(item.completedAt)}</dd>
          </div>
        </dl>

        {actionError ? (
          <div className="download-details__action-error">
            {actionError}
          </div>
        ) : null}

        {status === "failed" &&
        item.errorMessage ? (
          <div className="download-details__failure">
            <strong>
              {item.errorCode ??
                "Download failed"}
            </strong>

            <span>{item.errorMessage}</span>
          </div>
        ) : null}
      </div>
    </aside>
  );
}

