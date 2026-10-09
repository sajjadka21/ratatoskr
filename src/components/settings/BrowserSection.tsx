import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, ExternalLink, FolderOpen, Plug, RotateCcw } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useI18n } from "../../i18n/I18n";

type BrowserConnection = {
  hostFound: boolean;
  registered: string[];
  connected?: string[];
  extensionFolder: string | null;
  chromiumExtensionId: string;
  firefoxPackage: boolean;
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
  const requestSequence = useRef(0);

  useEffect(() => {
    let cancelled = false;
    const refresh = () => {
      const request = ++requestSequence.current;
      return invoke<BrowserConnection>("get_browser_connection")
      .then((value) => {
        if (!cancelled && request === requestSequence.current) setConnection(value);
      })
      .catch(() => {
        if (!cancelled && request === requestSequence.current) setConnection(current => current ? { ...current, connected: [] } : current);
      });
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  if (!connection) return null;

  const run = (command: string, args?: Record<string, unknown>) =>
    invoke(command, args).catch((reason) => onError(String(reason)));

  async function setupChromium(browser: "chrome" | "edge" | "brave") {
    const request = ++requestSequence.current;
    const extensionFolder = connection?.extensionFolder;
    let registrationError: unknown;
    try {
      await invoke("connect_browsers");
      const value = await invoke<BrowserConnection>("get_browser_connection");
      if (request === requestSequence.current) setConnection(value);
    } catch (reason) {
      registrationError = reason;
    }
    // Still open the extension page and folder when registry setup fails so
    // the user can finish loading the extension and see the host error clearly.
    try {
      await invoke("open_browser_extensions_page", { browser });
      if (extensionFolder) await invoke("reveal_extension_folder");
    } catch (reason) {
      onError(String(reason));
    }
    if (registrationError) onError(String(registrationError));
  }

  async function reconnect() {
    const request = ++requestSequence.current;
    try {
      const value = await invoke<BrowserConnection>("connect_browsers");
      if (request === requestSequence.current) setConnection(value);
    } catch (reason) {
      if (request === requestSequence.current) setConnection(current => current ? { ...current, connected: [] } : current);
      onError(String(reason));
    }
  }

  const connected = connection.hostFound ? connection.connected ?? [] : [];
  const registered = connected.map((name) => NAMES[name] ?? name).join(fmt.language === "fa" ? "، " : ", ");
  const registeredNames = (connection.registered ?? []).map((name) => NAMES[name] ?? name)
    .join(fmt.language === "fa" ? "، " : ", ");
  const connectionState = !connection.hostFound
    ? t("browser.hostMissing")
    : !connection.registered?.length
      ? t("browser.registrationMissing")
      : connected.length
        ? t("browser.ready", { browsers: registered })
        : t("browser.extensionNotResponding");

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
            <span className={`settings-page__ffmpeg ${connected.length ? "settings-page__ffmpeg--found" : ""}`}>
              {connected.length ? <CheckCircle2 size={14} aria-hidden="true" /> : <Plug size={14} aria-hidden="true" />}
              <span>
                {connectionState}
              </span>
            </span>
            {connection.hostFound && connection.registered?.length ? (
              <span>{t("browser.registeredFor", { browsers: registeredNames })}</span>
            ) : null}
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
                    className="settings-page__primary-button"
                    disabled={!connection.extensionFolder}
                    onClick={() => void setupChromium("chrome")}
                  >
                    <ExternalLink size={14} /> {t("browser.setupIn", { browser: "Chrome" })}
                  </button>
                  {(["edge", "brave"] as const).map((browser) => (
                    <button
                      key={browser}
                      type="button"
                      className="settings-page__secondary-button"
                      disabled={!connection.extensionFolder}
                      onClick={() => void setupChromium(browser)}
                    >
                      <ExternalLink size={14} /> {t("browser.setupIn", { browser: NAMES[browser] })}
                    </button>
                  ))}
                  <button type="button" className="settings-page__secondary-button" disabled={!connection.extensionFolder} onClick={() => void run("reveal_extension_folder")}>
                    <FolderOpen size={14} /> {t("browser.showFolder")}
                  </button>
                </>
              ) : null}
            </span>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>Firefox</strong>
            <span>
              {STORE_PAGES.firefox || connection.firefoxPackage ? t("browser.firefoxReady") : t("browser.firefoxHint")}
            </span>
          </div>
          <div className="settings-page__row-control">
            <span className="settings-page__button-row">
              {connection.firefoxPackage ? (
                <button type="button" className="settings-page__primary-button" onClick={() => void run("install_firefox_extension")}>
                  <ExternalLink size={14} /> {t("browser.addTo", { browser: "Firefox" })}
                </button>
              ) : STORE_PAGES.firefox ? (
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
