// @vitest-environment happy-dom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n/I18n";
import { AddDownloadModal } from "./AddDownloadModal";

// Extractor probes are unrelated to keyboard navigation and require the
// native bridge. Keep the real modal, labels, inputs and action menu.
vi.mock("./StreamQualityPicker", () => ({ StreamQualityPicker: () => null }));
vi.mock("./VideoQualityPicker", () => ({ VideoQualityPicker: () => null }));

describe("add-download modal keyboard navigation", () => {
  let root: Root;
  let trigger: HTMLButtonElement;
  let outside: HTMLButtonElement;

  beforeEach(() => {
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

  async function renderModal(options: { open?: boolean; ready?: boolean; submitting?: boolean } = {}) {
    await act(async () => {
      root.render(
        <I18nProvider language="en">
          <AddDownloadModal
            open={options.open ?? true}
            url={options.ready ? "https://example.com/file.zip" : ""}
            submitting={options.submitting ?? false}
            engineReady={options.ready ?? false}
            error={null}
            linkCount={options.ready ? 1 : 0}
            queues={[]}
            onUrlChange={() => {}}
            onClose={() => {}}
            onSubmit={() => {}}
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
