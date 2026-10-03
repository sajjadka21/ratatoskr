// @vitest-environment happy-dom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n/I18n";
import { AddDownloadModal } from "./AddDownloadModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

// Extractor probes are unrelated to keyboard navigation and require the
// native bridge. Keep the real modal, labels, inputs and action menu.
vi.mock("./StreamQualityPicker", () => ({ StreamQualityPicker: () => null }));
vi.mock("./VideoQualityPicker", () => ({ VideoQualityPicker: () => null }));

describe("add-download modal keyboard navigation", () => {
  let root: Root;
  let trigger: HTMLButtonElement;
  let outside: HTMLButtonElement;

  beforeEach(() => {
    vi.mocked(invoke).mockReset().mockResolvedValue({ matches: [], partial: false });
    vi.useFakeTimers();
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean })
      .IS_REACT_ACT_ENVIRONMENT = true;
    document.body.innerHTML = '<button id="trigger">Add link</button><div id="mount"></div><button id="outside">Settings</button>';
    trigger = document.querySelector<HTMLButtonElement>("#trigger")!;
    outside = document.querySelector<HTMLButtonElement>("#outside")!;
    trigger.focus();
    root = createRoot(document.querySelector("#mount")!);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    vi.useRealTimers();
    document.body.replaceChildren();
  });

  const submitted = vi.fn();
  async function renderModal(options: { open?: boolean; ready?: boolean; submitting?: boolean; duplicate?: boolean; url?: string; linkCount?: number } = {}) {
    await act(async () => {
      root.render(
        <I18nProvider language="en">
          <AddDownloadModal
            open={options.open ?? true}
            url={options.url ?? (options.ready ? "https://example.com/file.zip" : "")}
            submitting={options.submitting ?? false}
            engineReady={options.ready ?? false}
            error={null}
            linkCount={options.linkCount ?? (options.ready ? 1 : 0)}
            queues={[]}
            duplicateCount={options.duplicate ? 1 : 0}
            completedDuplicateCount={options.duplicate ? 1 : 0}
            duplicateNames={options.duplicate ? ["Series episode 03.mp4"] : []}
            onUrlChange={() => {}}
            onClose={() => {}}
            onSubmit={submitted}
          />
        </I18nProvider>,
      );
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(80);
    });
  }

  function dialog() {
    return document.querySelector<HTMLElement>('[role="dialog"]')!;
  }
  it("shows an actual folder match without history or blocking download", async () => {
    vi.mocked(invoke).mockResolvedValue({ matches: [{ sourceName: "file.zip", name: "file (1).zip", folder: "D:/Downloads", exact: false }], partial: false });
    await renderModal({ ready: true });
    await act(async () => { await vi.advanceTimersByTimeAsync(450); });
    expect(dialog().querySelector(".add-download-modal__folder-inspection")!.textContent).toContain("file (1).zip");
    expect(dialog().querySelector<HTMLButtonElement>(".add-download-modal__submit")!.disabled).toBe(false);
    expect(dialog().querySelector('input[type="checkbox"]')).toBeNull();
  });
  it("reports an unavailable folder inspection without claiming absence", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("access unavailable"));
    await renderModal({ ready: true });
    await act(async () => { await vi.advanceTimersByTimeAsync(450); });
    const hint = dialog().querySelector(".add-download-modal__folder-inspection")!.textContent;
    expect(hint?.length).toBeGreaterThan(0);
    expect(hint).not.toContain("No files");
    expect(dialog().querySelector<HTMLButtonElement>(".add-download-modal__submit")!.disabled).toBe(false);
  });
  it("keeps download later visible and puts only queue actions in the queue menu", async () => {
    await renderModal({ ready: true });
    const later = [...dialog().querySelectorAll<HTMLButtonElement>("footer button")].find(b => b.textContent?.includes("Download later"))!;
    expect(later).toBeDefined();
    submitted.mockClear();
    await act(async () => later.click());
    expect(submitted).toHaveBeenCalledWith({ kind: "download-later" }, "https://example.com/file.zip", null, "single-copy");
    await act(async () => dialog().querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')!.click());
    const menu = dialog().querySelector('[role="menu"]')!;
    expect(menu.textContent).not.toContain("Download later");
    expect(menu.textContent).not.toContain("Start now");
  });

  it("names a previously completed file and requires explicit consent before another copy", async () => {
    await renderModal({ ready: true, duplicate: true });
    expect(dialog().querySelector('[role="status"]')!.textContent).toContain("Series episode 03.mp4");
    const start = dialog().querySelector<HTMLButtonElement>(".add-download-modal__submit")!;
    expect(start.disabled).toBe(true);
    await act(async () => dialog().querySelector<HTMLInputElement>('input[type="checkbox"]')!.click());
    expect(start.disabled).toBe(false);
    submitted.mockClear();
    await act(async () => start.click());
    expect(submitted).toHaveBeenCalledTimes(1);
    await renderModal({ ready: true, duplicate: true, url: "https://example.com/another.zip" });
    expect(start.disabled).toBe(true);
  });

  it("requires an explicit all-or-new-only choice for duplicate batches", async () => {
    const batch = "https://example.com/episode-1.mkv\nhttps://example.com/episode-2.mkv";
    await renderModal({ ready: true, duplicate: true, linkCount: 2, url: batch });
    const start = dialog().querySelector<HTMLButtonElement>(".add-download-modal__submit")!;
    expect(start.disabled).toBe(true);
    const all = [...dialog().querySelectorAll<HTMLButtonElement>(".add-download-modal__duplicate-actions button")]
      .find((button) => button.textContent?.includes("Add all"))!;
    await act(async () => all.click());
    expect(start.disabled).toBe(false);
    submitted.mockClear();
    await act(async () => start.click());
    expect(submitted).toHaveBeenCalledWith({ kind: "start-now" }, batch, null, "all");
  });

  function enabledButtons() {
    return [...dialog().querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
  }

  function pressTab(target: HTMLElement, shiftKey = false) {
    const event = new KeyboardEvent("keydown", { key: "Tab", shiftKey, bubbles: true, cancelable: true });
    target.dispatchEvent(event);
    return event;
  }

  it("starts in the URL field rather than leaving focus on the background trigger", async () => {
    await renderModal();
    expect(document.activeElement).toBe(document.querySelector("#add-download-links"));
  });

  it("wraps Tab from the last enabled action to the first control", async () => {
    await renderModal();
    const buttons = enabledButtons();
    const last = buttons[buttons.length - 1]!;
    last.focus();
    expect(pressTab(last).defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("wraps Shift+Tab from the first control without reaching the background", async () => {
    await renderModal();
    const buttons = enabledButtons();
    buttons[0]!.focus();
    expect(pressTab(buttons[0]!, true).defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(buttons[buttons.length - 1]);
  });

  it("redirects programmatic focus back inside the open dialog", async () => {
    await renderModal();
    outside.focus();
    expect(dialog().contains(document.activeElement)).toBe(true);
  });

  it("restores focus to the original opener when the dialog closes", async () => {
    await renderModal();
    await renderModal({ open: false });
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
    outside.focus();
    expect(document.activeElement).toBe(outside);
  });

  it("recomputes focusable controls when download actions become enabled", async () => {
    await renderModal();
    await renderModal({ ready: true });
    const buttons = enabledButtons();
    const last = buttons[buttons.length - 1]!;
    expect(last.getAttribute("aria-haspopup")).toBe("menu");
    last.focus();
    expect(pressTab(last).defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("keeps focus on the dialog itself while all controls are disabled", async () => {
    await renderModal({ submitting: true });
    outside.focus();
    expect(document.activeElement).toBe(dialog());
    expect(pressTab(dialog()).defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(dialog());
  });
});
