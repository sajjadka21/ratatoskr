// @vitest-environment happy-dom

import { computeAccessibleName, isInaccessible } from "dom-accessibility-api";
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createTranslator, I18nProvider } from "../../i18n/I18n";
import type { Language } from "../../i18n/messages";
import type { UiPreferences } from "../../types/download";
import { SettingsPage, type AddDownloadInputMode } from "./SettingsPage";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "1.0.2" }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null, save: async () => null }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: async () => {} }));

const network = { mode: "manual", proxyUrl: "socks5://127.0.0.1:10808", pacUrl: null,
  systemPacUrl: null, directHosts: "", domesticDirect: false, domesticHosts: "" };
const downloads = { defaultDirectory: null, systemDirectory: "C:\\Downloads",
  globalSpeedLimit: null, preventSleep: false, maxConnections: 4 };
const errors = vi.fn();
const saved = vi.fn();

function Fixture({ language }: { language: Language }) {
  const [inputMode, setInputMode] = useState<AddDownloadInputMode>("clipboard");
  const [preferences, setPreferences] = useState<UiPreferences>({
    language, theme: "ember-forge", appearanceMode: "dark", closeToTray: true,
  });
  return <I18nProvider language={preferences.language}>
    <SettingsPage inputMode={inputMode} saving={false} error={null}
      onInputModeChange={setInputMode} queues={[]} downloadSettings={downloads}
      onDownloadSettingsChange={() => {}} onError={errors} onSaved={saved}
      uiPreferences={preferences} onUiPreferencesChange={setPreferences} />
  </I18nProvider>;
}

describe("settings navigation and search", () => {
  let root: Root;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    document.body.innerHTML = '<div id="mount"></div>';
    root = createRoot(document.querySelector("#mount")!);
    errors.mockClear();
    saved.mockClear();
    native.invoke.mockReset();
    native.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case "list_queue_schedules": case "list_categories": case "list_download_rules": return [];
        case "get_network_settings": return structuredClone(network);
        case "get_engine_settings": return { autoAdoptLinks: false, politeHosts: "", streamMaxHeight: null, streamPreferMp4: true };
        case "get_ffmpeg_status": case "get_ytdlp_status": return { configuredPath: null, foundPath: null, version: null };
        case "get_traffic_summary": return { periodStart: "2026-10-01", explicitPeriod: false,
          periodDomesticBytes: 0, periodInternationalBytes: 0, todayDomesticBytes: 0,
          todayInternationalBytes: 0, monthDomesticBytes: 0, monthInternationalBytes: 0, internationalQuota: null };
        case "get_post_process_settings": return { hashAlways: false, extractZip: false, scan: false, command: "", scanAvailable: true };
        case "get_browser_connection": return { hostFound: true, registered: [], extensionFolder: "C:\\extension",
          chromiumExtensionId: "example", firefoxPackage: false };
        case "get_pending_restore": return null;
        case "get_intake_window": return "main";
        case "set_network_settings": return args?.settings;
        default:
          if (command.startsWith("get_")) return false;
          throw new Error(`Unexpected native command in settings test: ${command}`);
      }
    });
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    document.body.replaceChildren();
  });

  async function render(language: Language = "en") {
    await act(async () => { root.render(<Fixture language={language} />); });
  }

  function visible(element: Element) { return !isInaccessible(element); }
  function tabs() { return [...document.querySelectorAll<HTMLButtonElement>('[role="tab"]')]; }
  function tab(name: string) {
    const found = tabs().find((element) => computeAccessibleName(element) === name);
    expect(found, `tab named ${name}`).toBeDefined();
    return found!;
  }
  function search() {
    const input = document.querySelector<HTMLInputElement>('input[type="search"], input[role="searchbox"]');
    expect(input, "search field").not.toBeNull();
    return input!;
  }
  function headings() {
    return [...document.querySelectorAll("h2")].filter(visible).map((heading) => heading.textContent);
  }
  async function click(element: HTMLElement) {
    await act(async () => { element.click(); });
  }
  async function enter(input: HTMLInputElement, value: string) {
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
  }
  function button(name: string, startsWith = false) {
    const found = [...document.querySelectorAll<HTMLButtonElement>("button")].filter(visible)
      .find((element) => startsWith ? computeAccessibleName(element).startsWith(name) : computeAccessibleName(element) === name);
    expect(found, `visible button named ${name}`).toBeDefined();
    return found!;
  }

  it.each<Language>(["en", "fa"])("starts with one accessible General panel and localized navigation (%s)", async (language) => {
    await render(language);
    const names = language === "fa" ? ["عمومی", "دانلودها", "شبکه", "مرورگر", "سیستم"]
      : ["General", "Downloads", "Network", "Browser", "System"];
    expect(document.querySelector('[role="tablist"]')).not.toBeNull();
    expect(tabs().map((element) => computeAccessibleName(element))).toEqual(names);
    expect(tab(names[0]!).getAttribute("aria-selected")).toBe("true");
    expect([...document.querySelectorAll('[role="tabpanel"]')].filter(visible)).toHaveLength(1);
    expect(computeAccessibleName(search())).toBe(language === "fa" ? "جست‌وجوی تنظیمات" : "Search settings");
    const t = createTranslator(language);
    expect(headings()).toContain(t("settings.appearance"));
    expect(headings()).not.toContain(t("network.title"));
    await click(tab(names[2]!));
    expect(headings()).toContain(t("network.title"));
    expect(headings()).not.toContain(t("settings.appearance"));
    const proxy = document.querySelector<HTMLInputElement>("#proxy-url")!;
    expect(visible(proxy)).toBe(true);
    proxy.focus();
    expect(document.activeElement).toBe(proxy);
  });

  it("searches proxy, browser, and appearance across groups while hiding unrelated controls", async () => {
    await render();
    const t = createTranslator("en");
    for (const [query, expected] of [["proxy", "network.title"], ["browser", "browser.title"],
      ["appearance", "settings.appearance"]] as const) {
      await enter(search(), query);
      expect(headings()).toContain(t(expected));
      if (query !== "appearance") expect(headings()).not.toContain(t("settings.appearance"));
      const proxy = document.querySelector("#proxy-url");
      if (proxy) expect(visible(proxy)).toBe(query === "proxy");
    }
    expect(errors).not.toHaveBeenCalled();
  });

  it("keeps an unsaved proxy draft when moving between groups", async () => {
    await render();
    await click(tab("Network"));
    await enter(document.querySelector<HTMLInputElement>("#proxy-url")!, "http://draft.example:8080");
    await click(tab("General"));
    expect(headings()).not.toContain(createTranslator("en")("network.title"));
    await click(tab("Network"));
    expect(document.querySelector<HTMLInputElement>("#proxy-url")!.value).toBe("http://draft.example:8080");
    expect(native.invoke.mock.calls.some(([command]) => command === "set_network_settings")).toBe(false);
  });

  it("clearing cross-group search restores the selected group without losing user choices", async () => {
    await render();
    await click(button("Light"));
    await click(button("Manual", true));
    await click(tab("Network"));
    await enter(search(), "appearance");
    await click(button("Forest Rune"));
    await enter(search(), "");
    expect(tab("Network").getAttribute("aria-selected")).toBe("true");
    expect(headings()).toContain(createTranslator("en")("network.title"));
    expect(headings()).not.toContain(createTranslator("en")("settings.appearance"));
    await click(tab("General"));
    expect(button("Light").getAttribute("aria-pressed")).toBe("true");
    expect(button("Forest Rune").getAttribute("aria-pressed")).toBe("true");
    expect(button("Manual", true).getAttribute("aria-pressed")).toBe("true");
  });
});
