import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Clipboard,
  FolderOpen,
  Globe2,
  Keyboard,
  MousePointerClick,
  PanelBottom,
  Plug,
  Sparkles,
  Wifi,
} from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type {
  ConnectionCheck,
  DownloadSettings,
  NetworkSettings,
  UiPreferences,
} from "../../types/download";
import { BrowserSection } from "../settings/BrowserSection";
import { BackendSwitchRow, ClipboardWatchRow, IntakeWindowRow } from "../settings/SettingsPage";

import "../settings/SettingsPage.css";
import "./Welcome.css";

const STEPS = ["hello", "folder", "browser", "network", "ready"] as const;
type Step = (typeof STEPS)[number];

/** The HTTP port v2rayN and similar programs usually offer. */
const COMMON_LOCAL_PROXY = "http://127.0.0.1:10809";

/**
 * The first-run guide: language and look, where files go, connecting the
 * browser, and how to reach the internet. Every choice here is also in
 * Settings, and the guide can be skipped at any step.
 */
export function Welcome({
  preferences,
  onPreferencesChange,
  onError,
  onDone,
}: {
  preferences: UiPreferences;
  onPreferencesChange: (next: UiPreferences) => void;
  onError: (message: string) => void;
  onDone: () => void;
}) {
  const { t, fmt } = useI18n();
  const [step, setStep] = useState<Step>("hello");
  const index = STEPS.indexOf(step);
  const rtl = preferences.language === "fa";
  const Back = rtl ? ArrowRight : ArrowLeft;
  const Next = rtl ? ArrowLeft : ArrowRight;

  const finish = () => {
    void invoke("set_onboarding_done", { done: true }).catch(() => {});
    onDone();
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") finish();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="welcome__backdrop" role="presentation">
      <section className="welcome" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
        <ol className="welcome__steps" aria-label={t("welcome.progress")}>
          {STEPS.map((name, position) => (
            <li
              key={name}
              className={
                position === index
                  ? "welcome__dot welcome__dot--current"
                  : position < index
                    ? "welcome__dot welcome__dot--done"
                    : "welcome__dot"
              }
              aria-current={position === index ? "step" : undefined}
            >
              <span className="sr-only">
                {t("welcome.stepOf", { step: fmt.number(position + 1), total: fmt.number(STEPS.length) })}
              </span>
            </li>
          ))}
        </ol>

        <div className="welcome__content settings-page">
          {step === "hello" ? (
            <HelloStep preferences={preferences} onPreferencesChange={onPreferencesChange} />
          ) : step === "folder" ? (
            <FolderStep onError={onError} />
          ) : step === "browser" ? (
            <BrowserStep onError={onError} />
          ) : step === "network" ? (
            <NetworkStep onError={onError} />
          ) : (
            <ReadyStep />
          )}
        </div>

        <footer className="welcome__footer">
          {index > 0 ? (
            <button type="button" className="welcome__secondary" onClick={() => setStep(STEPS[index - 1])}>
              <Back size={15} />
              {t("welcome.back")}
            </button>
          ) : (
            <button type="button" className="welcome__ghost" onClick={finish}>
              {t("welcome.skip")}
            </button>
          )}
          <span className="welcome__spacer" />
          {index < STEPS.length - 1 ? (
            <button type="button" className="welcome__primary" onClick={() => setStep(STEPS[index + 1])}>
              {t("welcome.next")}
              <Next size={15} />
            </button>
          ) : (
            <button type="button" className="welcome__primary" onClick={finish}>
              <CheckCircle2 size={15} />
              {t("welcome.start")}
            </button>
          )}
        </footer>
      </section>
    </div>
  );
}

function StepHeading({ icon, title, hint }: { icon: React.ReactNode; title: string; hint: string }) {
  return (
    <header className="welcome__heading">
      <div className="welcome__icon">{icon}</div>
      <div>
        <h2 id="welcome-title">{title}</h2>
        <p>{hint}</p>
      </div>
    </header>
  );
}

function HelloStep({
  preferences,
  onPreferencesChange,
}: {
  preferences: UiPreferences;
  onPreferencesChange: (next: UiPreferences) => void;
}) {
  const { t } = useI18n();
  const themes = [
    { value: "system", label: t("settings.themeSystem") },
    { value: "light", label: t("settings.themeLight") },
    { value: "dark", label: t("settings.themeDark") },
  ] as const;
  return (
    <>
      <StepHeading icon={<Sparkles size={20} />} title={t("welcome.helloTitle")} hint={t("welcome.helloHint")} />
      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.language")}</strong>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__segmented" role="group" aria-label={t("settings.language")}>
              {(["fa", "en"] as const).map((language) => (
                <button
                  key={language}
                  type="button"
                  aria-pressed={preferences.language === language}
                  className={
                    preferences.language === language
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() => onPreferencesChange({ ...preferences, language })}
                >
                  {language === "fa" ? "فارسی" : "English"}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.theme")}</strong>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__segmented" role="group" aria-label={t("settings.theme")}>
              {themes.map((theme) => (
                <button
                  key={theme.value}
                  type="button"
                  aria-pressed={preferences.theme === theme.value}
                  className={
                    preferences.theme === theme.value
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() => onPreferencesChange({ ...preferences, theme: theme.value })}
                >
                  {theme.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

function FolderStep({ onError }: { onError: (message: string) => void }) {
  const { t } = useI18n();
  const [settings, setSettings] = useState<DownloadSettings | null>(null);

  useEffect(() => {
    invoke<DownloadSettings>("get_download_settings")
      .then(setSettings)
      .catch(() => {});
  }, []);

  async function choose() {
    try {
      const folder = await openDialog({
        directory: true,
        multiple: false,
        title: t("welcome.chooseFolder"),
        defaultPath: settings?.defaultDirectory ?? settings?.systemDirectory ?? undefined,
      });
      if (typeof folder === "string" && folder) {
        setSettings(await invoke<DownloadSettings>("set_default_download_directory", { directory: folder }));
      }
    } catch (reason) {
      onError(String(reason));
    }
  }

  const folder = settings?.defaultDirectory ?? settings?.systemDirectory ?? null;
  return (
    <>
      <StepHeading icon={<FolderOpen size={20} />} title={t("welcome.folderTitle")} hint={t("welcome.folderHint")} />
      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("add.destination")}</strong>
            <span className={folder ? "ltr welcome__path" : undefined} title={folder ?? undefined}>
              {folder ?? t("add.systemDownloads")}
            </span>
          </div>
          <div className="settings-page__row-control">
            <button type="button" className="settings-page__secondary-button" onClick={() => void choose()}>
              <FolderOpen size={14} /> {t("add.changeFolder")}
            </button>
          </div>
        </div>
        <IntakeWindowRow onError={onError} />
        <BackendSwitchRow
          id="welcome-start-with-windows"
          label={t("settings.startWithWindows")}
          hint={t("settings.startWithWindowsHint")}
          read="get_start_with_windows"
          write="set_start_with_windows"
          onError={onError}
        />
      </div>
    </>
  );
}

function BrowserStep({ onError }: { onError: (message: string) => void }) {
  const { t } = useI18n();
  return (
    <>
      <StepHeading icon={<Plug size={20} />} title={t("welcome.browserTitle")} hint={t("welcome.browserHint")} />
      <div className="welcome__embedded">
        <BrowserSection onError={onError} />
      </div>
      <div className="settings-page__group settings-page__rows">
        <ClipboardWatchRow onError={onError} />
      </div>
    </>
  );
}

type Route = "system" | "manual" | "off";

function NetworkStep({ onError }: { onError: (message: string) => void }) {
  const { t, fmt } = useI18n();
  const [settings, setSettings] = useState<NetworkSettings | null>(null);
  const [address, setAddress] = useState(COMMON_LOCAL_PROXY);
  const [saving, setSaving] = useState(false);
  const [test, setTest] = useState<ConnectionCheck | "testing" | null>(null);

  useEffect(() => {
    invoke<NetworkSettings>("get_network_settings")
      .then((loaded) => {
        setSettings(loaded);
        if (loaded.proxyUrl) setAddress(loaded.proxyUrl);
      })
      .catch(() => {});
  }, []);

  if (!settings) return null;

  const route: Route =
    settings.mode === "manual" ? "manual" : settings.mode === "off" ? "off" : "system";

  async function save(next: Route, proxy = address) {
    if (!settings) return;
    setSaving(true);
    setTest(null);
    try {
      const mode = next === "system" ? (settings.mode === "pac" ? "pac" : "system") : next;
      setSettings(
        await invoke<NetworkSettings>("set_network_settings", {
          settings: { ...settings, mode, proxyUrl: next === "manual" ? proxy.trim() : settings.proxyUrl },
        }),
      );
    } catch (reason) {
      onError(String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function runTest() {
    setTest("testing");
    try {
      setTest(await invoke<ConnectionCheck>("check_connection", { url: "https://www.youtube.com/" }));
    } catch {
      setTest(null);
    }
  }

  const options: { value: Route; title: string; hint: string }[] = [
    { value: "system", title: t("welcome.routeSystem"), hint: t("welcome.routeSystemHint") },
    { value: "manual", title: t("welcome.routeManual"), hint: t("welcome.routeManualHint") },
    { value: "off", title: t("welcome.routeOff"), hint: t("welcome.routeOffHint") },
  ];

  return (
    <>
      <StepHeading icon={<Globe2 size={20} />} title={t("welcome.networkTitle")} hint={t("welcome.networkHint")} />
      <div className="welcome__choices" role="radiogroup" aria-label={t("welcome.networkTitle")}>
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={route === option.value}
            disabled={saving}
            className={route === option.value ? "welcome__choice welcome__choice--selected" : "welcome__choice"}
            onClick={() => void save(option.value)}
          >
            <strong>{option.title}</strong>
            <span>{option.hint}</span>
          </button>
        ))}
      </div>
      {route === "manual" ? (
        <div className="welcome__proxy">
          <input
            dir="ltr"
            value={address}
            onChange={(event) => setAddress(event.target.value)}
            aria-label={t("welcome.routeManual")}
            spellCheck={false}
          />
          <button type="button" className="settings-page__secondary-button" disabled={saving} onClick={() => void save("manual")}>
            {t("welcome.saveProxy")}
          </button>
        </div>
      ) : null}
      <div className="welcome__test">
        <button type="button" className="settings-page__secondary-button" disabled={test === "testing"} onClick={() => void runTest()}>
          <Wifi size={14} /> {test === "testing" ? t("diag.testing") : t("welcome.test")}
        </button>
        {test && test !== "testing" ? (
          <span className={test.reachable ? "welcome__ok" : "welcome__bad"}>
            {test.reachable
              ? t("welcome.testOk", { time: fmt.number(test.elapsedMs) })
              : t("welcome.testFailed")}
          </span>
        ) : null}
      </div>
    </>
  );
}

function ReadyStep() {
  const { t } = useI18n();
  const tips = [
    { icon: <Clipboard size={16} />, text: t("welcome.tipCopy") },
    { icon: <MousePointerClick size={16} />, text: t("welcome.tipBrowser") },
    { icon: <Keyboard size={16} />, text: t("welcome.tipKeys") },
    { icon: <PanelBottom size={16} />, text: t("welcome.tipTray") },
  ];
  return (
    <>
      <StepHeading icon={<CheckCircle2 size={20} />} title={t("welcome.readyTitle")} hint={t("welcome.readyHint")} />
      <ul className="welcome__tips">
        {tips.map((tip) => (
          <li key={tip.text}>
            <span className="welcome__tip-icon">{tip.icon}</span>
            <span>{tip.text}</span>
          </li>
        ))}
      </ul>
    </>
  );
}
