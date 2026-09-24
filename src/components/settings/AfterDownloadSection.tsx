import { Save } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type { PostProcessSettings } from "../../types/download";
import { Switch } from "./Switch";

/** Checksum, virus scan, unpacking and a command for every finished file. */
export function AfterDownloadSection({
  onError,
  onSaved,
}: {
  onError: (message: string) => void;
  onSaved: (message: string) => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<PostProcessSettings | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<PostProcessSettings>("get_post_process_settings")
      .then((settings) => {
        if (!cancelled) setDraft(settings);
      })
      .catch((reason: unknown) => {
        if (!cancelled) onError(String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [onError]);

  if (!draft) return null;

  async function save(next: PostProcessSettings, announce: boolean) {
    try {
      setDraft(await invoke<PostProcessSettings>("set_post_process_settings", { settings: next }));
      if (announce) onSaved(t("post.saved"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("post.title")}</h2>
        <p>{t("post.hint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("post.hash")}</strong>
            <span>{t("post.hashHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <Switch
              id="post-hash"
              checked={draft.hashAlways}
              label={t("post.hash")}
              onChange={(hashAlways) => void save({ ...draft, hashAlways }, false)}
            />
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("post.scan")}</strong>
            <span>{draft.scanAvailable ? t("post.scanHint") : t("post.scanMissing")}</span>
          </div>
          <div className="settings-page__row-control">
            <Switch
              id="post-scan"
              checked={draft.scan}
              disabled={!draft.scanAvailable && !draft.scan}
              label={t("post.scan")}
              onChange={(scan) => void save({ ...draft, scan }, false)}
            />
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("post.extract")}</strong>
            <span>{t("post.extractHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <Switch
              id="post-extract"
              checked={draft.extractZip}
              label={t("post.extract")}
              onChange={(extractZip) => void save({ ...draft, extractZip }, false)}
            />
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <label htmlFor="post-command">
              <strong>{t("post.command")}</strong>
            </label>
            <span>{t("post.commandHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <input
              id="post-command"
              className="settings-page__text-input"
              dir="ltr"
              spellCheck={false}
              placeholder={'"C:\\Tools\\check.exe" {file}'}
              value={draft.command}
              onChange={(event) => setDraft({ ...draft, command: event.target.value })}
            />
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={() => void save(draft, true)}
            >
              <Save size={14} /> {t("post.save")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
