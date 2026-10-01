// @vitest-environment happy-dom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createTranslator, I18nProvider } from "../../i18n/I18n";
import type { Language } from "../../i18n/messages";
import { BrowserSection } from "./BrowserSection";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: async () => {} }));

type Connection = {
  hostFound: boolean; registered: string[]; connected?: string[];
  extensionFolder: string | null; chromiumExtensionId: string; firefoxPackage: boolean;
};
const base: Connection = {
  hostFound: true, registered: ["chrome", "edge", "firefox"], connected: [],
  extensionFolder: "C:\\fixture-extension", chromiumExtensionId: "fixture-id", firefoxPackage: false,
};

describe("browser connector recent-contact status", () => {
  let root: Root;
  const errors = vi.fn();

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-01T12:00:00Z"));
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    document.body.innerHTML = '<div id="mount"></div>';
    root = createRoot(document.querySelector("#mount")!);
    errors.mockClear();
    native.invoke.mockReset();
    native.invoke.mockResolvedValue(structuredClone(base));
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    vi.useRealTimers();
    document.body.replaceChildren();
  });

  async function render(language: Language = "en") {
    await act(async () => {
      root.render(<I18nProvider language={language}><BrowserSection onError={errors} /></I18nProvider>);
    });
  }

  function status() {
    const element = document.querySelector<HTMLElement>(".settings-page__ffmpeg");
    expect(element, "visible browser contact status").not.toBeNull();
    return element!;
  }

  function expectNoContact(language: Language = "en") {
    expect(status().classList.contains("settings-page__ffmpeg--found")).toBe(false);
    expect(status().textContent).toBe(createTranslator(language)("browser.notRegistered"));
  }

  it.each<Language>(["en", "fa"])("installed connector registration alone never becomes a green connection (%s)", async (language) => {
    await render(language);
    expectNoContact(language);
    expect(native.invoke).toHaveBeenCalledWith("get_browser_connection");
  });

  it("legacy native replies without a connected field remain unconfirmed", async () => {
    const legacy = { ...base };
    delete legacy.connected;
    native.invoke.mockResolvedValue(legacy);
    await render();
    expectNoContact();
  });

  it("shows only browsers with recent contact, not every registered connector", async () => {
    native.invoke.mockResolvedValue({ ...base, connected: ["chrome", "firefox"] });
    await render();
    expect(status().classList.contains("settings-page__ffmpeg--found")).toBe(true);
    expect(status().textContent).toBe(createTranslator("en")("browser.ready", { browsers: "Chrome, Firefox" }));
    expect(status().textContent).not.toContain("Edge");
  });

  it("a missing host cannot show a green connection from an old heartbeat", async () => {
    native.invoke.mockResolvedValue({ ...base, hostFound: false, connected: ["chrome"] });
    await render();
    expect(status().classList.contains("settings-page__ffmpeg--found")).toBe(false);
    expect(status().textContent).toBe(createTranslator("en")("browser.hostMissing"));
  });

  it("refreshes contact state and removes green status after the heartbeat window expires", async () => {
    const pingAt = Date.now();
    native.invoke.mockImplementation(async (command: string) => {
      expect(command).toBe("get_browser_connection");
      return { ...base, connected: Date.now() - pingAt <= 150_000 ? ["chrome"] : [] };
    });
    await render();
    expect(status().classList.contains("settings-page__ffmpeg--found")).toBe(true);

    await act(async () => { await vi.advanceTimersByTimeAsync(150_000); });
    expect(status().classList.contains("settings-page__ffmpeg--found")).toBe(true);
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });

    expectNoContact();
    expect(native.invoke.mock.calls.length).toBeGreaterThan(2);
  });

  it("cannot keep reporting a recent connection indefinitely when native refresh fails", async () => {
    native.invoke.mockResolvedValueOnce({ ...base, connected: ["chrome"] });
    native.invoke.mockRejectedValue(new Error("fixture connector status unavailable"));
    await render();
    expect(status().classList.contains("settings-page__ffmpeg--found")).toBe(true);

    await act(async () => { await vi.advanceTimersByTimeAsync(155_000); });

    expectNoContact();
  });

  it("an older delayed reply cannot replace a newer no-contact result", async () => {
    let resolveOld!: (connection: Connection) => void;
    const olderRequest = new Promise<Connection>((resolve) => { resolveOld = resolve; });
    native.invoke.mockReturnValueOnce(olderRequest).mockResolvedValue({ ...base, connected: [] });
    await render();
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    expectNoContact();

    await act(async () => { resolveOld({ ...base, connected: ["chrome"] }); });

    expectNoContact();
  });

  it("a pending automatic poll cannot overwrite the newer manual reconnect result", async () => {
    let resolvePoll!: (connection: Connection) => void;
    const pendingPoll = new Promise<Connection>((resolve) => { resolvePoll = resolve; });
    native.invoke.mockResolvedValueOnce({ ...base, connected: ["chrome"] })
      .mockReturnValueOnce(pendingPoll)
      .mockResolvedValue({ ...base, connected: [] });
    await render();
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    const label = createTranslator("en")("browser.reconnect");
    const reconnect = [...document.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.trim() === label);
    expect(reconnect).toBeDefined();

    await act(async () => { reconnect!.click(); });
    expect(native.invoke).toHaveBeenCalledWith("connect_browsers");
    expectNoContact();
    await act(async () => { resolvePoll({ ...base, connected: ["chrome"] }); });

    expectNoContact();
    // A genuinely newer automatic refresh remains authoritative.
    native.invoke.mockResolvedValue({ ...base, connected: ["firefox"] });
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    expect(status().textContent).toBe(createTranslator("en")("browser.ready", { browsers: "Firefox" }));
  });

  it("stops polling after the section is unmounted", async () => {
    await render();
    const calls = native.invoke.mock.calls.length;
    await act(async () => root.render(null));
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(native.invoke).toHaveBeenCalledTimes(calls);
  });
});
