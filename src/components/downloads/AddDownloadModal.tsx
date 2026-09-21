import {
  ChevronDown,
  Clock3,
  Download,
  Layers3,
  Link2,
  X,
} from "lucide-react";

import {
  useEffect,
  useRef,
  useState,
} from "react";

import "./AddDownloadModal.css";

type AddDownloadAction =
  | "start-now"
  | "download-later";

type AddDownloadModalProps = {
  open: boolean;
  url: string;
  submitting: boolean;
  engineReady: boolean;
  error: string | null;
  linkCount: number;
  onUrlChange: (value: string) => void;
  onClose: () => void;
  onSubmit: (action: AddDownloadAction) => void;
};

export function AddDownloadModal({
  open,
  url,
  submitting,
  engineReady,
  error,
  linkCount,
  onUrlChange,
  onClose,
  onSubmit,
}: AddDownloadModalProps) {
  const inputRef =
    useRef<HTMLTextAreaElement>(null);
  const actionRef =
    useRef<HTMLDivElement>(null);
  const [actionMenuOpen, setActionMenuOpen] =
    useState(false);

  const isBatch = linkCount > 1;
  const actionsDisabled =
    submitting || !engineReady || linkCount === 0;

  useEffect(() => {
    if (!open) return;

    const timer = window.setTimeout(() => {
      inputRef.current?.focus();

      const length =
        inputRef.current?.value.length ?? 0;

      inputRef.current?.setSelectionRange(
        length,
        length,
      );
    }, 60);

    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape" || submitting) {
        return;
      }

      if (actionMenuOpen) {
        setActionMenuOpen(false);
        return;
      }

      onClose();
    }

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [actionMenuOpen, onClose, open, submitting]);

  useEffect(() => {
    if (!actionMenuOpen) return;

    function handlePointerDown(event: PointerEvent) {
      if (
        event.target instanceof Node &&
        !actionRef.current?.contains(event.target)
      ) {
        setActionMenuOpen(false);
      }
    }

    window.addEventListener("pointerdown", handlePointerDown);

    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [actionMenuOpen]);

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
          !submitting
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
                  {isBatch ? "Add Batch" : "Add Download"}
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
            disabled={submitting}
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </header>

        <div className="add-download-modal__body">
          <label className="add-download-modal__label">
            {isBatch ? "Download links" : "URL"}
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
              rows={isBatch ? Math.min(Math.max(linkCount, 3), 7) : 1}
              onChange={(event) => onUrlChange(event.target.value)}
              onKeyDown={(event) => {
                if (
                  event.key === "Enter" &&
                  !event.shiftKey &&
                  !isBatch &&
                  linkCount === 1 &&
                  !actionsDisabled
                ) {
                  event.preventDefault();
                  onSubmit("start-now");
                }
              }}
              disabled={submitting}
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
                  <span>Batch detected · {linkCount} unique links</span>
                </>
              ) : (
                <>
                  <Link2 size={13} />
                  <span>1 valid download link</span>
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
            <strong>System Downloads folder</strong>
          </div>

          {error ? (
            <div className="add-download-modal__error">
              {error}
            </div>
          ) : null}
        </div>

        <footer className="add-download-modal__footer">
          <button
            type="button"
            className="add-download-modal__cancel"
            onClick={onClose}
            disabled={submitting}
          >
            Cancel
          </button>

          <div
            ref={actionRef}
            className="add-download-modal__split"
          >
            <button
              type="button"
              className="add-download-modal__submit"
              onClick={() => onSubmit("start-now")}
              disabled={actionsDisabled}
            >
              {isBatch ? (
                <Layers3 size={16} />
              ) : (
                <Download size={16} />
              )}

              {submitting
                ? "Creating..."
                : isBatch
                  ? `Start ${linkCount} Downloads`
                  : "Start Download"}
            </button>

            <button
              type="button"
              className="add-download-modal__dropdown-toggle"
              aria-label="Choose download action"
              aria-haspopup="menu"
              aria-expanded={actionMenuOpen}
              onClick={() => setActionMenuOpen((current) => !current)}
              disabled={actionsDisabled}
            >
              <ChevronDown size={15} />
            </button>

            {actionMenuOpen ? (
              <div
                className="add-download-modal__action-menu"
                role="menu"
              >
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setActionMenuOpen(false);
                    onSubmit("start-now");
                  }}
                >
                  <Download size={15} />
                  <span>
                    <strong>Start Now</strong>
                    <small>Create tasks and run them in the background</small>
                  </span>
                </button>

                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setActionMenuOpen(false);
                    onSubmit("download-later");
                  }}
                >
                  <Clock3 size={15} />
                  <span>
                    <strong>Download Later</strong>
                    <small>Create tasks without network activity</small>
                  </span>
                </button>
              </div>
            ) : null}
          </div>
        </footer>
      </section>
    </div>
  );
}
