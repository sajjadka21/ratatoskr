import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";

import "./CloseDialog.css";

const CLOSE_REQUESTED_EVENT = "close-requested";

/**
 * Pressing X is not the same as minimising: minimising keeps everything running without asking. When X
 * would stop a download or a schedule, ask once, and offer to remember the answer.
 */
export function CloseDialog() {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [remember, setRemember] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const subscription = listen(CLOSE_REQUESTED_EVENT, () => {
      setError(null);
      setOpen(true);
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open) return null;

  const choose = (action: "tray" | "quit") => {
    invoke("resolve_close", { action, remember })
      .then(() => setOpen(false))
      .catch((reason) => setError(String(reason)));
  };

  return (
    <div className="close-dialog__backdrop" role="presentation">
      <div className="close-dialog" role="alertdialog" aria-modal="true" aria-labelledby="close-dialog-title">
        <h2 id="close-dialog-title">{t("close.title")}</h2>
        <p>{t("close.body")}</p>
        <div className="close-dialog__actions">
          <button type="button" className="close-dialog__primary" onClick={() => choose("tray")} autoFocus>
            {t("close.tray")}
          </button>
          <button type="button" onClick={() => choose("quit")}>
            {t("close.quit")}
          </button>
          <button type="button" onClick={() => setOpen(false)}>
            {t("close.stay")}
          </button>
        </div>
        <label className="close-dialog__remember">
          <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />
          {t("close.remember")}
        </label>
        {error ? <p className="close-dialog__error">{error}</p> : null}
      </div>
    </div>
  );
}
