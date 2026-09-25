import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, ExternalLink, FolderOpen, Plug, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";

type BrowserConnection = {
  hostFound: boolean;
  registered: string[];
  extensionFolder: string | null;
  chromiumExtensionId: string;
};

/**
 * Store pages of the published extension. While a browser has none, the
 * extension is loaded from the folder that ships with the app.
 */
const STORE_PAGES: Record<"chrome" | "edge" | "firefox", string | null> = {
  chrome: null,
  edge: null,
  firefox: null,
};

const NAMES: Record<string, string> = {
  chrome: "Chrome",
  edge: "Edge",
  brave: "Brave",
  chromium: "Chromium",
  firefox: "Firefox",
};

/** Connecting the browser extension: what is done and what is left. */
export function BrowserSection({ onError }: { onError: (message: string) => void }) {
  const { t, fmt } = useI18n();
  const [connection, setConnection] = useState<BrowserConnection | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<BrowserConnection>("get_browser_connection")
      .then((value) => {
        if (!cancelled) setConnection(value);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  if (!connection) return null;

  const run = (command: string, args?: Record<string, unknown>) =>
    invoke(command, args).catch((reason) => onError(String(reason)));

  async function reconnect() {
    try {
      setConnection(await invoke<BrowserConnection>("connect_browsers"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  const registered = connection.registered.map((name) => NAMES[name] ?? name).join(fmt.language === "fa" ? "، " : ", ");

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("browser.title")}</h2>
        <p>{t("browser.hint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("browser.connection")}</strong>
            <span>{t("browser.connectionHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <span className={`settings-page__ffmpeg ${connection.registered.length ? "settings-page__ffmpeg--found" : ""}`}>
              {connection.registered.length ? <CheckCircle2 size={14} aria-hidden="true" /> : <Plug size={14} aria-hidden="true" />}
              <span>
                {!connection.hostFound
                  ? t("browser.hostMissing")
                  : connection.registered.length
                    ? t("browser.ready", { browsers: registered })
                    : t("browser.notRegistered")}
              </span>
            </span>
            {connection.hostFound ? (
              <span className="settings-page__button-row">
                <button type="button" className="settings-page__secondary-button" onClick={() => void reconnect()}>
                  <RotateCcw size={14} /> {t("browser.reconnect")}
                </button>
              </span>
            ) : null}
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("browser.chromium")}</strong>
            <span>{STORE_PAGES.chrome || STORE_PAGES.edge ? t("browser.storeHint") : t("browser.chromiumHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <span className="settings-page__button-row">
              {STORE_PAGES.chrome ? (
                <button type="button" className="settings-page__primary-button" onClick={() => void openUrl(STORE_PAGES.chrome!)}>
                  <ExternalLink size={14} /> {t("browser.addTo", { browser: "Chrome" })}
                </button>
              ) : null}
              {STORE_PAGES.edge ? (
                <button type="button" className="settings-page__primary-button" onClick={() => void openUrl(STORE_PAGES.edge!)}>
                  <ExternalLink size={14} /> {t("browser.addTo", { browser: "Edge" })}
                </button>
              ) : null}
              {!STORE_PAGES.chrome || !STORE_PAGES.edge ? (
                <>
                  <button
                    type="button"
                    className="settings-page__secondary-button"
                    disabled={!connection.extensionFolder}
                    onClick={() => void run("reveal_extension_folder")}
                  >
                    <FolderOpen size={14} /> {t("browser.showFolder")}
                  </button>
                  {(["chrome", "edge", "brave"] as const).map((browser) => (
                    <button
                      key={browser}
                      type="button"
                      className="settings-page__secondary-button"
                      onClick={() => void run("open_browser_extensions_page", { browser })}
                    >
                      <ExternalLink size={14} /> {NAMES[browser]}
                    </button>
                  ))}
                </>
              ) : null}
            </span>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>Firefox</strong>
            <span>{STORE_PAGES.firefox ? t("browser.storeHint") : t("browser.firefoxHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <span className="settings-page__button-row">
              {STORE_PAGES.firefox ? (
                <button type="button" className="settings-page__primary-button" onClick={() => void openUrl(STORE_PAGES.firefox!)}>
                  <ExternalLink size={14} /> {t("browser.addTo", { browser: "Firefox" })}
                </button>
              ) : (
                <button
                  type="button"
                  className="settings-page__secondary-button"
                  onClick={() => void run("open_browser_extensions_page", { browser: "firefox" })}
                >
                  <ExternalLink size={14} /> Firefox
                </button>
              )}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
