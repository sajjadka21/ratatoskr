import { StrictMode, useCallback, useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import App from "./App";
import { MiniWindow } from "./mini/MiniWindow";
import { I18nProvider } from "./i18n/I18n";
import type { UiPreferences } from "./types/download";
import { resolveAppearance } from "./utils/appearance";

import "./styles/global.css";

/** The small download windows load the same page with `?view=mini`. */
const IS_MINI = new URLSearchParams(window.location.search).get("view") === "mini";

const DEFAULT_PREFERENCES: UiPreferences = { language: "fa", theme: "ember-forge", appearanceMode: "dark", closeToTray: true };

/** Applies language, direction and theme to the whole document. */
function applyToDocument(preferences: UiPreferences) {
  const root = document.documentElement;
  root.lang = preferences.language;
  root.dir = preferences.language === "fa" ? "rtl" : "ltr";
  const appearance = resolveAppearance(preferences, window.matchMedia("(prefers-color-scheme: dark)").matches);
  root.dataset.theme = appearance.theme;
  root.dataset.brandTheme = appearance.brand;
  root.dataset.appearanceMode = appearance.mode;
  if (!IS_MINI) document.title = preferences.language === "fa" ? "راتاتوسک" : "Ratatoskr";
  // The Windows title bar follows the chosen theme too.
  try {
    void getCurrentWindow()
      .setTheme(
        appearance.mode === "system" ? null : appearance.dark ? "dark" : "light",
      )
      .catch(() => {});
  } catch {
    // Outside Tauri (tests, a plain browser) there is no window to theme.
  }
}

function Root() {
  const [preferences, setPreferences] = useState<UiPreferences | null>(null);
  useEffect(() => {
    if (!preferences) return;
    const apply = () => applyToDocument(preferences);
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    apply(); media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [preferences]);

  useEffect(() => {
    void invoke<UiPreferences>("get_ui_preferences")
      .catch(() => DEFAULT_PREFERENCES)
      .then((loaded) => {
        applyToDocument(loaded);
        setPreferences(loaded);
      });
  }, []);

  const change = useCallback(async (next: UiPreferences) => {
    applyToDocument(next);
    setPreferences(next);
    try {
      const saved = await invoke<UiPreferences>("set_ui_preferences", { preferences: next });
      setPreferences(saved);
    } catch (reason) {
      console.error("Could not save preferences:", reason);
    }
  }, []);

  // Nothing is shown until the language is known, so the window never
  // flashes in the wrong language or direction.
  if (!preferences) return null;

  return (
    <I18nProvider language={preferences.language}>
      {IS_MINI ? (
        <MiniWindow />
      ) : (
        <App preferences={preferences} onPreferencesChange={(next) => void change(next)} />
      )}
    </I18nProvider>
  );
}

applyToDocument(DEFAULT_PREFERENCES);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
