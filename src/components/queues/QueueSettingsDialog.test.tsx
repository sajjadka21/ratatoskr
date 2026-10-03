// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { I18nProvider } from "../../i18n/I18n";
import type { DownloadQueue } from "../../types/download";
import { QueueSettingsDialog } from "./QueueSettingsDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { document.body.replaceChildren(); vi.clearAllMocks(); });
it("loads the selected queue schedule in a modal and offers a close action", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(invoke).mockResolvedValue([]);
  document.body.innerHTML = '<div id="mount"></div>';
  const root = createRoot(document.querySelector("#mount")!);
  const close = vi.fn();
  const queue = { id: "series", name: "Series", enabled: true, maxConcurrent: 1, maxConcurrentPerHost: 1 } as DownloadQueue;
  try {
    await act(async () => root.render(<I18nProvider language="en"><QueueSettingsDialog queue={queue} onClose={close} onLimits={vi.fn()} onEnabled={vi.fn()} /></I18nProvider>));
    expect(document.querySelector("dialog")?.open).toBe(true);
    expect(invoke).toHaveBeenCalledWith("list_queue_schedules");
    expect(document.querySelector(".settings-page__schedule-editor")).not.toBeNull();
    await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!.click());
    expect(close).toHaveBeenCalledOnce();
  } finally { await act(async () => root.unmount()); }
});
