import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { Download, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import { Switch } from "./Switch";

const UPDATE_PROGRESS_EVENT = "update-progress";

type UpdateInfo = { version: string; currentVersion: string; notes: string | null };
type UpdateProgress = { downloaded: number; total: number | null };

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: UpdateInfo }
  | { kind: "installing"; update: UpdateInfo; progress: UpdateProgress | null }
  | { kind: "notConfigured" }
  | { kind: "failed"; reason: string };

/**
 * Updates of the app itself. Every release is signed; only files that
 * match the key built into the app are installed, and only when asked.
 */
export function UpdateSection({ onError }: { onError: (message: string) => void }) {
  const { t, fmt } = useI18n();
  const [version, setVersion] = useState<string | null>(null);
  const [autoCheck, setAutoCheck] = useState<boolean | null>(null);
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    void getVersion().then(setVersion).catch(() => setVersion(null));
    void invoke<boolean>("get_auto_update_check").then(setAutoCheck).catch(() => setAutoCheck(null));
    const subscription = listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, ({ payload }) =>
      setState((current) => (current.kind === "installing" ? { ...current, progress: payload } : current)),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  async function check() {
    setState({ kind: "checking" });
    try {
      const update = await invoke<UpdateInfo | null>("check_for_update");
      setState(update ? { kind: "available", update } : { kind: "latest" });
    } catch (reason) {
      const text = String(reason);
      setState(text === "not_configured" ? { kind: "notConfigured" } : { kind: "failed", reason: text });
    }
  }

  async function install(update: UpdateInfo) {
    setState({ kind: "installing", update, progress: null });
    try {
      await invoke("install_update");
    } catch (reason) {
      setState({ kind: "failed", reason: String(reason) });
    }
  }

  const status = (() => {
    switch (state.kind) {
      case "checking":
        return t("update.checking");
      case "latest":
        return t("update.latest");
      case "available":
        return t("update.available", { version: state.update.version });
      case "installing":
        return state.progress?.total
          ? t("update.downloading", {
              done: fmt.bytes(state.progress.downloaded),
              total: fmt.bytes(state.progress.total),
            })
          : t("update.preparing");
      case "notConfigured":
        return t("update.notConfigured");
      case "failed":
        return t("update.failed", { reason: state.reason });
      default:
        return null;
    }
  })();

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("update.title")}</h2>
        <p>{t("update.hint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("update.version")}</strong>
            <span className="ltr num">{version ?? "—"}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <span className="settings-page__button-row">
              {state.kind === "available" ? (
                <button type="button" className="settings-page__primary-button" onClick={() => void install(state.update)}>
                  <Download size={14} /> {t("update.install")}
                </button>
              ) : (
                <button
                  type="button"
                  className="settings-page__secondary-button"
                  disabled={state.kind === "checking" || state.kind === "installing"}
                  onClick={() => void check()}
                >
                  <RefreshCw size={14} /> {t("update.check")}
                </button>
              )}
            </span>
            {status ? <p className="settings-page__hint-inline" role="status">{status}</p> : null}
          </div>
        </div>

        {autoCheck !== null ? (
          <div className="settings-page__row">
            <div className="settings-page__row-label">
              <strong>{t("update.auto")}</strong>
              <span>{t("update.autoHint")}</span>
            </div>
            <div className="settings-page__row-control">
              <Switch
                id="auto-update-check"
                checked={autoCheck}
                label={t("update.auto")}
                onChange={(next) => {
                  setAutoCheck(next);
                  invoke<boolean>("set_auto_update_check", { enabled: next })
                    .then(setAutoCheck)
                    .catch((reason) => {
                      setAutoCheck(!next);
                      onError(String(reason));
                    });
                }}
              />
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
