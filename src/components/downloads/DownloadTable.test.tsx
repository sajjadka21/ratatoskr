// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18n";
import type { DownloadListItem } from "../../types/download";
import { DownloadTable, downloadDate } from "./DownloadTable";

const item: DownloadListItem = {
  id: "movie", sourceUrl: "https://example.com/movie.mp4", resolvedUrl: null,
  filename: "movie.mp4", destinationPath: null, mimeType: "video/mp4", etag: null,
  lastModified: null, rangeSupported: true, totalBytes: 100, downloadedBytes: 100,
  status: "completed", queueId: null, priority: "normal", queuePosition: null,
  createdAt: 1700000000, startedAt: 1700000100, completedAt: 1700000200,
  attempts: 0, retryAt: null, errorCode: null, errorMessage: null,
};

describe("download table selection, details and date", () => {
  let root: Root;
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    document.body.innerHTML = '<div id="mount"></div>';
    root = createRoot(document.querySelector("#mount")!);
  });
  afterEach(async () => { await act(async () => root.unmount()); document.body.replaceChildren(); });

  async function renderTable() {
    const callbacks = { onSelect: vi.fn(), onDetails: vi.fn(), onAction: vi.fn(), onContextMenu: vi.fn(), onSort: vi.fn() };
    await act(async () => root.render(<I18nProvider language="en"><DownloadTable
      items={[item]} metrics={{}} history={{}} selectedIds={new Set()} focusedId={null}
      queueNames={new Map()} nowSeconds={1700000200} sort={{key: "date", descending: true}} empty={null}
      {...callbacks} /></I18nProvider>));
    return callbacks;
  }

  it("selects with a row click, opens details with the dots, and keeps actions on right-click", async () => {
    const callbacks = await renderTable();
    const row = document.querySelector<HTMLElement>(".download-table__row")!;
    await act(async () => row.click());
    expect(callbacks.onSelect).toHaveBeenCalledOnce();
    expect(callbacks.onDetails).not.toHaveBeenCalled();
    await act(async () => document.querySelector<HTMLButtonElement>('[aria-label="Show download details"]')!.click());
    expect(callbacks.onDetails).toHaveBeenCalledWith(item);
    expect(callbacks.onSelect).toHaveBeenCalledOnce();
    expect(callbacks.onContextMenu).not.toHaveBeenCalled();
    await act(async () => row.dispatchEvent(new MouseEvent("contextmenu", {bubbles: true, clientX: 30, clientY: 50})));
    expect(callbacks.onContextMenu).toHaveBeenCalledWith(item, 30, 50);
  });

  it("sorts by the visible download date and exposes details to the keyboard", async () => {
    const callbacks = await renderTable();
    const button = document.querySelector<HTMLButtonElement>('[aria-label="Sort by Download date"]')!;
    expect(button.closest('[role="columnheader"]')?.getAttribute("aria-sort")).toBe("descending");
    await act(async () => button.click());
    expect(callbacks.onSort).toHaveBeenCalledWith("date");
    await act(async () => document.querySelector(".download-table__row")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
    expect(callbacks.onDetails).toHaveBeenCalledWith(item);
    expect(document.querySelector(".download-table__cell--date")!.textContent).not.toBe("");
  });

  it("uses completion, then start, then addition for the displayed date", () => {
    expect(downloadDate(item)).toBe(item.completedAt);
    expect(downloadDate({...item, completedAt: null})).toBe(item.startedAt);
    expect(downloadDate({...item, completedAt: null, startedAt: null})).toBe(item.createdAt);
  });
});
