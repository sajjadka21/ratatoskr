// @vitest-environment happy-dom
import { act } from "react";
import type { Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { UiPreferences } from "./types/download";

const bridge = vi.hoisted(() => ({ root: null as Root | null,
  saves: [] as Array<{ next: UiPreferences; resolve: (value: UiPreferences) => void }> }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme: async () => {} }) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (command: string, args?: { preferences: UiPreferences }) => {
  if (command === "get_ui_preferences") return Promise.resolve({ language: "en", theme: "ember-forge", appearanceMode: "dark", closeToTray: true });
  return new Promise<UiPreferences>((resolve) => bridge.saves.push({ next: args!.preferences, resolve }));
} }));
vi.mock("react-dom/client", async (original) => {
  const actual = await original<typeof import("react-dom/client")>();
  const createRoot = (...args: Parameters<typeof actual.createRoot>) => { bridge.root = actual.createRoot(...args); return bridge.root; };
  return { ...actual, createRoot, default: { ...actual, createRoot } };
});
vi.mock("./App", () => ({ default: ({ preferences, onPreferencesChange }: { preferences: UiPreferences; onPreferencesChange: (next: UiPreferences) => void }) => <>
  <output>{preferences.theme}</output>
  <button onClick={() => onPreferencesChange({ ...preferences, theme: "forest-rune" })}>Forest</button>
  <button onClick={() => onPreferencesChange({ ...preferences, theme: "frost-byte" })}>Frost</button>
</> }));
afterEach(async () => { await act(async () => bridge.root?.unmount()); bridge.root = null; bridge.saves = []; document.body.replaceChildren(); vi.resetModules(); });

it("keeps the latest chosen theme when older save responses arrive afterward", async () => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  document.body.innerHTML = '<div id="root"></div>';
  await act(async () => { await import("./main"); });
  await act(async () => document.querySelectorAll<HTMLButtonElement>("button")[0].click());
  await act(async () => document.querySelectorAll<HTMLButtonElement>("button")[1].click());
  const [older, newer] = bridge.saves;
  await act(async () => newer.resolve(newer.next));
  await act(async () => older.resolve(older.next));
  expect(document.querySelector("output")?.textContent).toBe("frost-byte");
  expect(document.documentElement.dataset.brandTheme).toBe("frost-byte");
});
