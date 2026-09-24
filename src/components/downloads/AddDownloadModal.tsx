import {
  ChevronDown,
  Clock3,
  Download,
  Layers3,
  Link2,
  Plus,
  X,
} from "lucide-react";

import {
  useEffect,
  useRef,
  useState,
} from "react";

import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";

import { StreamQualityPicker } from "./StreamQualityPicker";

import "./AddDownloadModal.css";
import type { DownloadQueue } from "../../types/download";

export type AddDownloadAction =
  | { kind: "start-now" }
  | { kind: "download-later" }
  | { kind: "queue"; queueId: string }
  | { kind: "create-queue"; queueName: string };

type AddDownloadModalProps = {
  open: boolean;
  url: string;
  submitting: boolean;
  engineReady: boolean;
  error: string | null;
  linkCount: number;
  queues: DownloadQueue[];
  /// Folder used when no category or rule names one; null is the system
  /// Downloads folder.
  defaultDirectory?: string | null;
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
  queues,
  defaultDirectory = null,
  onUrlChange,
  onClose,
  onSubmit,
}: AddDownloadModalProps) {
  const { t, fmt } = useI18n();
  const inputRef =
    useRef<HTMLTextAreaElement>(null);
  const actionRef =
    useRef<HTMLDivElement>(null);
  const [actionMenuOpen, setActionMenuOpen] =
    useState(false);
  const [newQueueName, setNewQueueName] = useState("");

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
                  {isBatch ? t("add.titleBatch", { count: fmt.number(linkCount) }) : t("add.title")}
                </h2>


              </div>

              <p>
                {isBatch
                  ? t("add.subtitleBatch", { count: fmt.number(linkCount) })
                  : t("add.subtitle")}
              </p>
            </div>
          </div>

          <button
            type="button"
            className="add-download-modal__close"
            onClick={onClose}
            disabled={submitting}
            aria-label={t("add.close")}
          >
            <X size={18} />
          </button>
        </header>

        <div className="add-download-modal__body">
          <label className="add-download-modal__label" htmlFor="add-download-links">
            {isBatch ? t("add.links") : t("add.url")}
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
              id="add-download-links"
              dir="ltr"
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
                  onSubmit({ kind: "start-now" });
                }
              }}
              disabled={submitting}
              placeholder={
                isBatch ? t("add.placeholderBatch") : t("add.placeholder")
              }
            />
          </div>

          {linkCount > 0 ? (
            <div className="add-download-modal__detection">
              {isBatch ? (
                <>
                  <Layers3 size={13} />
                  <span>{t("add.detectedBatch", { count: fmt.number(linkCount) })}</span>
                </>
              ) : (
                <>
                  <Link2 size={13} />
                  <span>{t("add.detectedOne")}</span>
                </>
              )}
            </div>
          ) : url.trim() ? (
            <div className="add-download-modal__detection add-download-modal__detection--warning">
              {t("add.noneValid")}
            </div>
          ) : null}

          {linkCount === 1 ? (
            <StreamQualityPicker url={url} onChoose={onUrlChange} />
          ) : null}

          <div className="add-download-modal__destination">
            <span>{t("add.destination")}</span>
            <strong title={defaultDirectory ?? undefined} className={defaultDirectory ? "ltr" : undefined}>
              {defaultDirectory ?? t("add.systemDownloads")}
            </strong>
            <small>{t("add.destinationHint")}</small>
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
            {t("add.cancel")}
          </button>

          <div
            ref={actionRef}
            className="add-download-modal__split"
          >
            <button
              type="button"
              className="add-download-modal__submit"
              onClick={() => onSubmit({ kind: "start-now" })}
              disabled={actionsDisabled}
            >
              {isBatch ? (
                <Layers3 size={16} />
              ) : (
                <Download size={16} />
              )}

              {submitting
                ? t("add.creating")
                : isBatch
                  ? t("add.startBatch", { count: fmt.number(linkCount) })
                  : t("add.start")}
            </button>

            <button
              type="button"
              className="add-download-modal__dropdown-toggle"
              aria-label={t("add.chooseAction")}
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
                    onSubmit({ kind: "start-now" });
                  }}
                >
                  <Download size={15} />
                  <span>
                    <strong>{t("add.startNow")}</strong>
                    <small>{t("add.startNowHint")}</small>
                  </span>
                </button>

                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setActionMenuOpen(false);
                    onSubmit({ kind: "download-later" });
                  }}
                >
                  <Clock3 size={15} />
                  <span>
                    <strong>{t("add.later")}</strong>
                    <small>{t("add.laterHint")}</small>
                  </span>
                </button>

                <div className="add-download-modal__menu-divider" />

                {queues.map((queue) => (
                  <button
                    key={queue.id}
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setActionMenuOpen(false);
                      onSubmit({ kind: "queue", queueId: queue.id });
                    }}
                  >
                    <Layers3 size={15} />
                    <span>
                      <strong>{t("add.toQueue", { name: queue.name })}</strong>
                      <small>
                        {t("add.toQueueHint", {
                          priority: t(`priority.${queue.defaultPriority}` as MessageKey),
                        })}
                      </small>
                    </span>
                  </button>
                ))}

                <div className="add-download-modal__create-queue">
                  <input
                    value={newQueueName}
                    onChange={(event) => setNewQueueName(event.target.value)}
                    onKeyDown={(event) => event.stopPropagation()}
                    placeholder={t("add.newQueue")}
                    aria-label={t("add.newQueue")}
                  />
                  <button
                    type="button"
                    disabled={!newQueueName.trim()}
                    onClick={() => {
                      const queueName = newQueueName.trim();
                      if (!queueName) return;
                      setActionMenuOpen(false);
                      setNewQueueName("");
                      onSubmit({ kind: "create-queue", queueName });
                    }}
                  >
                    <Plus size={14} />
                    {t("add.createQueue")}
                  </button>
                </div>
              </div>
            ) : null}
          </div>
        </footer>
      </section>
    </div>
  );
}
