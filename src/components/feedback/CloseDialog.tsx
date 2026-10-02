import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

import { useI18n } from "../../i18n/I18n";

import "./CloseDialog.css";

const CLOSE_REQUESTED_EVENT = "close-requested";

/**
 * Pressing X is not the same as minimising: minimising keeps everything running without asking. When X
 * would stop a download or a schedule, ask once, and offer to remember the answer.
 */
export function CloseDialog({ onOpenChange }: { onOpenChange?: (open: boolean) => void }) {
  const { t } = useI18n();
  const openerRef = useRef<HTMLElement | null>(null);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [pending, setPending] = useState(false);
  const [open, setOpen] = useState(false);
  const [remember, setRemember] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const subscription = listen(CLOSE_REQUESTED_EVENT, () => {
      openerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setError(null);
      setOpen(true);
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    onOpenChange?.(open);
    if (!open) return;
    const opener = openerRef.current;
    const dialog = dialogRef.current;
    dialog?.showModal();
    return () => { dialog?.close(); if (opener?.isConnected) opener.focus(); };
  }, [open, onOpenChange]);

  if (!open) return null;

  const choose = (action: "tray" | "quit") => {
    if (pending) return;
    setPending(true);
    invoke("resolve_close", { action, remember })
      .then(() => setOpen(false))
      .catch((reason) => setError(String(reason)))
      .finally(() => setPending(false));
  };

  return (
    <dialog ref={dialogRef} className="close-dialog" role="alertdialog" aria-labelledby="close-dialog-title" aria-describedby="close-dialog-body"
      onCancel={(event) => { event.preventDefault(); if (!pending) setOpen(false); }}>
        <h2 id="close-dialog-title">{t("close.title")}</h2>
        <p id="close-dialog-body">{t("close.body")}</p>
        <div className="close-dialog__actions">
          <button type="button" disabled={pending} className="close-dialog__primary" onClick={() => choose("tray")} autoFocus>
            {t("close.tray")}
          </button>
          <button type="button" disabled={pending} onClick={() => choose("quit")}>
            {t("close.quit")}
          </button>
          <button type="button" disabled={pending} onClick={() => setOpen(false)}>
            {t("close.stay")}
          </button>
        </div>
        <label className="close-dialog__remember">
          <input type="checkbox" disabled={pending} checked={remember} onChange={(event) => setRemember(event.target.checked)} />
          {t("close.remember")}
        </label>
        {error ? <p role="alert" className="close-dialog__error">{error}</p> : null}
    </dialog>
  );
}
