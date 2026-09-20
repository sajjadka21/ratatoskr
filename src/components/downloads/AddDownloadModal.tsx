import { Download, Link2, X } from "lucide-react";
import { useEffect, useRef } from "react";

import "./AddDownloadModal.css";

type AddDownloadModalProps = {
  open: boolean;
  url: string;
  downloading: boolean;
  engineReady: boolean;
  error: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  onUrlChange: (value: string) => void;
  onClose: () => void;
  onDownload: () => void;
};

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

export function AddDownloadModal({
  open,
  url,
  downloading,
  engineReady,
  error,
  downloadedBytes,
  totalBytes,
  onUrlChange,
  onClose,
  onDownload,
}: AddDownloadModalProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  const percent =
    totalBytes && totalBytes > 0
      ? Math.min(100, (downloadedBytes / totalBytes) * 100)
      : null;

  useEffect(() => {
    if (!open) return;

    const timer = window.setTimeout(() => {
      inputRef.current?.focus();
    }, 60);

    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !downloading) {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open, downloading, onClose]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="add-download-modal__backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (
          event.target === event.currentTarget &&
          !downloading
        ) {
          onClose();
        }
      }}
    >
      <section
        className="add-download-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-download-title"
      >
        <header className="add-download-modal__header">
          <div className="add-download-modal__heading">
            <div className="add-download-modal__icon">
              <Link2 size={20} />
            </div>

            <div>
              <h2 id="add-download-title">
                Add Download
              </h2>
              <p>
                Paste a direct link and we'll handle the rest.
              </p>
            </div>
          </div>

          <button
            type="button"
            className="add-download-modal__close"
            onClick={onClose}
            disabled={downloading}
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </header>

        <div className="add-download-modal__body">
          <label className="add-download-modal__label">
            URL
          </label>

          <div className="add-download-modal__input-wrap">
            <Link2 size={16} />

            <input
              ref={inputRef}
              value={url}
              onChange={(event) =>
                onUrlChange(event.target.value)
              }
              onKeyDown={(event) => {
                if (
                  event.key === "Enter" &&
                  !downloading &&
                  engineReady &&
                  url.trim()
                ) {
                  onDownload();
                }
              }}
              disabled={downloading}
              placeholder="https://example.com/file.zip"
            />
          </div>

          <div className="add-download-modal__destination">
            <span>Destination</span>
            <strong>System Downloads folder</strong>
          </div>

          {downloading ? (
            <div className="add-download-modal__progress">
              <div className="add-download-modal__progress-track">
                <div
                  className="add-download-modal__progress-fill"
                  style={{
                    width: `${percent ?? 100}%`,
                    opacity: percent === null ? 0.45 : 1,
                  }}
                />
              </div>

              <div className="add-download-modal__progress-meta">
                <span>{formatBytes(downloadedBytes)}</span>

                <span>
                  {totalBytes
                    ? `${formatBytes(totalBytes)} · ${percent?.toFixed(1)}%`
                    : "Downloading..."}
                </span>
              </div>
            </div>
          ) : null}

          {error ? (
            <div className="add-download-modal__error">
              {error}
            </div>
          ) : null}

          <button
            type="button"
            className="add-download-modal__advanced"
            disabled
          >
            Advanced options
            <span>Coming later</span>
          </button>
        </div>

        <footer className="add-download-modal__footer">
          <button
            type="button"
            className="add-download-modal__cancel"
            onClick={onClose}
            disabled={downloading}
          >
            Cancel
          </button>

          <button
            type="button"
            className="add-download-modal__submit"
            onClick={onDownload}
            disabled={
              downloading ||
              !engineReady ||
              !url.trim()
            }
          >
            <Download size={16} />

            {downloading
              ? "Downloading..."
              : "Download"}
          </button>
        </footer>
      </section>
    </div>
  );
}
