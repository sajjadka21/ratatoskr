// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { I18nProvider } from "../../i18n/I18n";
import { VideoQualityPicker } from "./VideoQualityPicker";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
let root: Root;
const replace = vi.fn();
const playlist = vi.fn();
beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  vi.mocked(invoke).mockReset(); replace.mockClear(); playlist.mockClear();
  document.body.innerHTML = '<div id="mount"></div>';
  root = createRoot(document.querySelector("#mount")!);
});
afterEach(async () => { await act(async () => root.unmount()); vi.useRealTimers(); document.body.replaceChildren(); });
const probe = { title: "Test list", isPlaylist: true, entries: [{ url: "https://youtu.be/one", title: "One" }, { url: "https://youtu.be/two", title: "Two" }], qualities: [], audioBytes: null, durationSeconds: null };
async function render(text: string) {
  await act(async () => root.render(<I18nProvider language="en"><VideoQualityPicker text={text} quality={720} onQualityChange={() => {}} onReplaceLinks={replace} onPlaylist={playlist} /></I18nProvider>));
  await act(async () => { await vi.advanceTimersByTimeAsync(500); });
}
it("keeps a video single until the user explicitly chooses its complete playlist", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => command === "probe_video" ? { ...probe, isPlaylist: false, entries: [] } : {});
  await render("Watch this: https://www.youtube.com/watch?v=test&list=PL_test");
  const action = [...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent?.includes("entire YouTube playlist"))!;
  expect(action).toBeDefined();
  expect(replace).not.toHaveBeenCalled();
  await act(async () => action.click());
  expect(replace).toHaveBeenCalledWith("https://www.youtube.com/playlist?list=PL_test");
});
it("offers every accessible playlist entry as a separate selectable link", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => command === "probe_video" ? probe : {});
  await render("https://www.youtube.com/playlist?list=PL_two");
  expect(playlist).toHaveBeenLastCalledWith(["https://youtu.be/one", "https://youtu.be/two"]);
  const action = [...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent?.includes("Select and download 2 videos"))!;
  await act(async () => action.click());
  expect(replace).toHaveBeenCalledWith("https://youtu.be/one\nhttps://youtu.be/two");
});
it("allows a failed network lookup to be retried without editing the link", async () => {
  let attempts = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command !== "probe_video") return {};
    if (++attempts === 1) throw new Error("temporary network failure");
    return { ...probe, isPlaylist: false, entries: [] };
  });
  await render("https://youtu.be/retry_test");
  const action = [...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent?.includes("Try looking up"))!;
  await act(async () => action.click());
  await act(async () => { await vi.advanceTimersByTimeAsync(500); });
  expect(attempts).toBe(2);
  expect(document.body.textContent).toContain("Test list");
});
