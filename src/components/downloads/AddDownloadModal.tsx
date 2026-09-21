import {
  Download,
  Layers3,
  Link2,
  X,
} from "lucide-react";

import {
  useEffect,
  useRef,
} from "react";

import "./AddDownloadModal.css";

type AddDownloadModalProps = {
  open: boolean;
  url: string;
  downloading: boolean;
  engineReady: boolean;
  error: string | null;

  downloadedBytes: number;
  totalBytes: number | null;

  linkCount: number;
  currentIndex: number;

  onUrlChange: (value: string) => void;
  onClose: () => void;
  onDownload: () => void;
};

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";

  const units = [
    "B",
    "KB",
    "MB",
    "GB",
    "TB",
  ];

  const index = Math.min(
    Math.floor(
      Math.log(bytes) / Math.log(1024),
    ),
    units.length - 1,
  );

  const value =
    bytes / 1024 ** index;

  return `${value.toFixed(
    index === 0 ? 0 : 2,
  )} ${units[index]}`;
}

export function AddDownloadModal({
  open,
  url,
  downloading,
  engineReady,
  error,
  downloadedBytes,
  totalBytes,
  linkCount,
  currentIndex,
  onUrlChange,
  onClose,
  onDownload,
}: AddDownloadModalProps) {
  const inputRef =
    useRef<HTMLTextAreaElement>(null);

  const isBatch =
    linkCount > 1;

  const percent =
    totalBytes && totalBytes > 0
      ? Math.min(
          100,
          (
            downloadedBytes /
            totalBytes
          ) * 100,
        )
      : null;

  useEffect(() => {
    if (!open) return;

    const timer =
      window.setTimeout(() => {
        inputRef.current?.focus();

        const length =
          inputRef.current?.value.length ?? 0;

        inputRef.current?.setSelectionRange(
          length,
          length,
        );
      }, 60);

    return () =>
      window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;

    function handleKeyDown(
      event: KeyboardEvent,
    ) {
      if (
        event.key === "Escape" &&
        !downloading
      ) {
        onClose();
      }
    }

    window.addEventListener(
      "keydown",
      handleKeyDown,
    );

    return () => {
      window.removeEventListener(
        "keydown",
        handleKeyDown,
      );
    };
  }, [
    open,
    downloading,
    onClose,
  ]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="add-download-modal__backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (
          event.target ===
            event.currentTarget &&
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
              {isBatch ? (
                <Layers3 size={20} />
              ) : (
                <Link2 size={20} />
              )}
            </div>

            <div>
              <div className="add-download-modal__title-row">
                <h2 id="add-download-title">
                  {isBatch
                    ? "Add Batch"
                    : "Add Download"}
                </h2>

                {isBatch ? (
                  <span className="add-download-modal__batch-badge">
                    {linkCount} links
                  </span>
                ) : null}
              </div>

              <p>
                {isBatch
                  ? `${linkCount} unique download links detected.`
                  : "Paste a direct link and we'll handle the rest."}
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
            {isBatch
              ? "Download links"
              : "URL"}
          </label>

          <div
            className={`add-download-modal__input-wrap ${
              isBatch
                ? "add-download-modal__input-wrap--batch"
                : ""
            }`}
          >
            <Link2 size={16} />

            <textarea
              ref={inputRef}
              value={url}
              rows={
                isBatch
                  ? Math.min(
                      Math.max(linkCount, 3),
                      7,
                    )
                  : 1
              }
              onChange={(event) =>
                onUrlChange(
                  event.target.value,
                )
              }
              onKeyDown={(event) => {
                if (
                  event.key === "Enter" &&
                  !event.shiftKey &&
                  !isBatch &&
                  linkCount === 1 &&
                  !downloading &&
                  engineReady
                ) {
                  event.preventDefault();
                  onDownload();
                }
              }}
              disabled={downloading}
              placeholder={
                isBatch
                  ? "One download link per line"
                  : "https://example.com/file.zip"
              }
            />
          </div>

          {linkCount > 0 ? (
            <div className="add-download-modal__detection">
              {isBatch ? (
                <>
                  <Layers3 size={13} />
                  <span>
                    Batch detected ·{" "}
                    {linkCount} unique links
                  </span>
                </>
              ) : (
                <>
                  <Link2 size={13} />
                  <span>
                    1 valid download link
                  </span>
                </>
              )}
            </div>
          ) : url.trim() ? (
            <div className="add-download-modal__detection add-download-modal__detection--warning">
              No valid HTTP or HTTPS link detected.
            </div>
          ) : null}

          <div className="add-download-modal__destination">
            <span>Destination</span>

            <strong>
              System Downloads folder
            </strong>
          </div>

          {downloading ? (
            <div className="add-download-modal__progress">
              {isBatch ? (
                <div className="add-download-modal__batch-progress">
                  Downloading{" "}
                  {Math.max(
                    currentIndex,
                    1,
                  )}{" "}
                  of {linkCount}
                </div>
              ) : null}

              <div className="add-download-modal__progress-track">
                <div
                  className="add-download-modal__progress-fill"
                  style={{
                    width: `${
                      percent ?? 100
                    }%`,
                    opacity:
                      percent === null
                        ? 0.45
                        : 1,
                  }}
                />
              </div>

              <div className="add-download-modal__progress-meta">
                <span>
                  {formatBytes(
                    downloadedBytes,
                  )}
                </span>

                <span>
                  {totalBytes
                    ? `${formatBytes(
                        totalBytes,
                      )} · ${percent?.toFixed(
                        1,
                      )}%`
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
              linkCount === 0
            }
          >
            {isBatch ? (
              <Layers3 size={16} />
            ) : (
              <Download size={16} />
            )}

            {downloading
              ? isBatch
                ? `Downloading ${Math.max(
                    currentIndex,
                    1,
                  )} of ${linkCount}`
                : "Downloading..."
              : isBatch
                ? `Download ${linkCount} Files`
                : "Download"}
          </button>
        </footer>
      </section>
    </div>
  );
}
