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

  const name =
    item.filename ??
    item.resolvedUrl ??
    item.sourceUrl;

  const canDeleteFile =
    Boolean(item.destinationPath);

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
              Remove from History?
            </h2>

            <p>
              Remove this download from the app.
            </p>
          </div>

          <button
            type="button"
            className="remove-history-dialog__close"
            onClick={onCancel}
            disabled={removing}
            aria-label="Close"
          >
            <X size={17} />
          </button>
        </header>

        <div className="remove-history-dialog__body">
          <div className="remove-history-dialog__file">
            <span>Download</span>

            <strong title={name}>
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
                <strong>
                  Also delete the downloaded file
                </strong>

                <span>
                  Permanently remove the file from disk.
                </span>
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
              ? "The file will be permanently deleted from your computer. This cannot be undone."
              : "The downloaded file will remain on your computer."}
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
            Cancel
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
              ? "Removing..."
              : deleteFile
                ? "Remove & Delete File"
                : "Remove from History"}
          </button>
        </footer>
      </section>
    </div>
  );
}
