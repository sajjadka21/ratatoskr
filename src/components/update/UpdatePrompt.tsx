import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Download } from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import { shouldAsk, skipVersion, skippedVersion, type OfferedUpdate } from "./updatePrompt";

import "./UpdatePrompt.css";

const UPDATE_AVAILABLE_EVENT = "update-available";
const UPDATE_PROGRESS_EVENT = "update-progress";

type Progress = { downloaded: number; total: number | null };

/**
 * Asks before anything is downloaded: "Update now", "Later" or "Skip this
 * version". Installing is still the backend's job, and it only verifies and
 * installs a file signed with the project's key.
 */
export function UpdatePrompt() {
  const { t, fmt } = useI18n();
  const [offer, setOffer] = useState<OfferedUpdate | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const offered = listen<OfferedUpdate>(UPDATE_AVAILABLE_EVENT, ({ payload }) => {
      if (shouldAsk(payload, skippedVersion(window.localStorage))) setOffer(payload);
    });
    const moving = listen<Progress>(UPDATE_PROGRESS_EVENT, ({ payload }) => setProgress(payload));
    return () => {
      void offered.then((unlisten) => unlisten());
      void moving.then((unlisten) => unlisten());
    };
  }, []);

  if (!offer) return null;

  const install = () => {
    setInstalling(true);
    setError(null);
    invoke("install_update").catch((reason) => {
      setInstalling(false);
      setError(String(reason));
    });
  };

  return (
    <div className="update-prompt" role="dialog" aria-label={t("update.title")}>
      <Download size={18} />
      <div className="update-prompt__text">
        <strong>{t("update.available", { version: offer.version })}</strong>
        {offer.notes ? <p>{offer.notes.slice(0, 280)}</p> : null}
        {installing ? (
          <span>
            {progress
              ? t("update.downloading", {
                  done: fmt.bytes(progress.downloaded),
                  total: progress.total ? fmt.bytes(progress.total) : "?",
                })
              : t("update.preparing")}
          </span>
        ) : (
          <span>{t("update.consent")}</span>
        )}
        {error ? <span className="update-prompt__error">{error}</span> : null}
      </div>
      {installing ? null : (
        <div className="update-prompt__actions">
          <button type="button" className="update-prompt__primary" onClick={install}>
            {t("update.now")}
          </button>
          <button type="button" onClick={() => setOffer(null)}>
            {t("update.later")}
          </button>
          <button
            type="button"
            onClick={() => {
              skipVersion(window.localStorage, offer.version);
              setOffer(null);
            }}
          >
            {t("update.skip")}
          </button>
        </div>
      )}
    </div>
  );
}
