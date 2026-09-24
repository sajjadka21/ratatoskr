import { RefreshCw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type { DownloadListItem } from "../../types/download";
import { displayName } from "../../utils/fileKind";

import "./RefreshLinkDialog.css";

type RefreshLinkDialogProps = {
  item: DownloadListItem | null;
  onCancel: () => void;
  onSubmit: (item: DownloadListItem, url: string) => Promise<void>;
};

/** Replaces an expired or changed link while keeping the task and its bytes. */
export function RefreshLinkDialog({ item, onCancel, onSubmit }: RefreshLinkDialogProps) {
  const { t } = useI18n();
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setValue(item?.sourceUrl ?? "");
    setError(null);
    setSaving(false);
    if (item) window.setTimeout(() => inputRef.current?.select(), 30);
  }, [item]);

  if (!item) return null;
  const current = item;

  async function submit() {
    const url = value.trim();
    if (!/^https?:\/\/\S+$/i.test(url)) {
      setError(t("refresh.invalid"));
      return;
    }
    if (url === current.sourceUrl.trim()) {
      onCancel();
      return;
    }
    setSaving(true);
    try {
      await onSubmit(current, url);
    } catch (reason) {
      setError(String(reason));
      setSaving(false);
    }
  }

  return (
    <div
      className="refresh-dialog__backdrop"
      onMouseDown={(event) => event.target === event.currentTarget && !saving && onCancel()}
      onKeyDown={(event) => event.key === "Escape" && !saving && onCancel()}
    >
      <form
        className="refresh-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="refresh-dialog-title"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <header>
          <RefreshCw size={18} aria-hidden="true" />
          <div>
            <h2 id="refresh-dialog-title">{t("refresh.title")}</h2>
            <span className="ltr">{displayName(current)}</span>
          </div>
          <button type="button" onClick={onCancel} aria-label={t("add.close")} disabled={saving}>
            <X size={16} />
          </button>
        </header>
        <p>{t("refresh.subtitle")}</p>
        <label htmlFor="refresh-link-input">{t("refresh.label")}</label>
        <input
          id="refresh-link-input"
          ref={inputRef}
          dir="ltr"
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            setError(null);
          }}
          disabled={saving}
        />
        {error ? <div className="refresh-dialog__error">{error}</div> : null}
        <footer>
          <button type="button" className="refresh-dialog__secondary" onClick={onCancel} disabled={saving}>
            {t("add.cancel")}
          </button>
          <button type="submit" className="refresh-dialog__primary" disabled={saving}>
            {t("refresh.save")}
          </button>
        </footer>
      </form>
    </div>
  );
}
