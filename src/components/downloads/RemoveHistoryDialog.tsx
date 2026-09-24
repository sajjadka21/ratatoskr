import {
  AlertTriangle,
  Trash2,
  X,
} from "lucide-react";

import {
  useEffect,
  useState,
} from "react";

import type { DownloadListItem } from "../../types/download";

import { useI18n } from "../../i18n/I18n";
import { displayName } from "../../utils/fileKind";

import "./RemoveHistoryDialog.css";

type RemoveHistoryDialogProps = {
  item: DownloadListItem | null;
  removing: boolean;
  error: string | null;
  onCancel: () => void;
  onConfirm: (deleteFile: boolean) => void;
};

export function RemoveHistoryDialog({
  item,
  removing,
  error,
  onCancel,
  onConfirm,
}: RemoveHistoryDialogProps) {
  const { t } = useI18n();
  const [deleteFile, setDeleteFile] =
    useState(false);

  useEffect(() => {
    setDeleteFile(false);
  }, [item?.id]);

  useEffect(() => {
    if (!item) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (
        event.key === "Escape" &&
        !removing
      ) {
        onCancel();
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
  }, [item, removing, onCancel]);

  if (!item) {
    return null;
  }

  const name = displayName(item);

  const canDeleteFile =
    Boolean(item.destinationPath);

  // A task that never transferred anything has no file to keep or delete, so
  // the dialog says what will actually happen instead of mentioning a file.
  const hasFile = canDeleteFile;

  return (
    <div
      className="remove-history-dialog__backdrop"
      onMouseDown={(event) => {
        if (
          event.target === event.currentTarget &&
          !removing
        ) {
          onCancel();
        }
      }}
    >
      <section
        className="remove-history-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="remove-history-title"
      >
        <header className="remove-history-dialog__header">
          <div className="remove-history-dialog__warning">
            <AlertTriangle size={19} />
          </div>

          <div className="remove-history-dialog__heading">
            <h2 id="remove-history-title">
              {hasFile ? t("remove.titleHistory") : t("remove.titleTask")}
            </h2>

            <p>
              {hasFile ? t("remove.subtitleHistory") : t("remove.subtitleTask")}
            </p>
          </div>

          <button
            type="button"
            className="remove-history-dialog__close"
            onClick={onCancel}
            disabled={removing}
            aria-label={t("remove.close")}
          >
            <X size={17} />
          </button>
        </header>

        <div className="remove-history-dialog__body">
          <div className="remove-history-dialog__file">
            <span>{hasFile ? t("remove.download") : t("remove.task")}</span>
            <strong title={name} className="ltr">
              {name}
            </strong>
          </div>

          {canDeleteFile ? (
            <label className="remove-history-dialog__delete-option">
              <input
                type="checkbox"
                checked={deleteFile}
                disabled={removing}
                onChange={(event) =>
                  setDeleteFile(
                    event.target.checked,
                  )
                }
              />

              <span className="remove-history-dialog__checkbox" />

              <div>
                <strong>{t("remove.alsoDelete")}</strong>
                <span>{t("remove.alsoDeleteHint")}</span>
              </div>
            </label>
          ) : null}

          <div
            className={`remove-history-dialog__notice ${
              deleteFile
                ? "remove-history-dialog__notice--danger"
                : ""
            }`}
          >
            {deleteFile
              ? t("remove.noticeDelete")
              : hasFile
                ? t("remove.noticeKeep")
                : t("remove.noticeNothing")}
          </div>

          {error ? (
            <div className="remove-history-dialog__error">
              {error}
            </div>
          ) : null}
        </div>

        <footer className="remove-history-dialog__footer">
          <button
            type="button"
            className="remove-history-dialog__cancel"
            onClick={onCancel}
            disabled={removing}
          >
            {t("remove.cancel")}
          </button>

          <button
            type="button"
            className={`remove-history-dialog__remove ${
              deleteFile
                ? "remove-history-dialog__remove--danger"
                : ""
            }`}
            onClick={() =>
              onConfirm(deleteFile)
            }
            disabled={removing}
          >
            {deleteFile ? (
              <Trash2 size={14} />
            ) : null}

            {removing
              ? t("remove.removing")
              : deleteFile
                ? t("remove.removeAndDelete")
                : hasFile
                  ? t("remove.removeHistory")
                  : t("remove.removeTask")}
          </button>
        </footer>
      </section>
    </div>
  );
}
