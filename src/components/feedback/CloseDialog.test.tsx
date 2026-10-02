// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18n";
import { CloseDialog } from "./CloseDialog";

const bridge = vi.hoisted(() => ({ callback: null as null | (() => void), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_: string, callback: () => void) => { bridge.callback = callback; return () => {}; }) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: bridge.invoke }));
let root: ReturnType<typeof createRoot>;
afterEach(async () => { await act(async () => root.unmount()); document.body.replaceChildren(); vi.clearAllMocks(); });

it("uses a native modal so keyboard navigation cannot reach downloads behind the close confirmation", async () => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  document.body.innerHTML = '<button id="opener">Behind</button><div id="mount"></div>';
  root = createRoot(document.querySelector("#mount")!);
  const opener = document.querySelector<HTMLButtonElement>("#opener")!;
  opener.focus();
  await act(async () => root.render(<I18nProvider language="en"><CloseDialog /></I18nProvider>));
  await act(async () => bridge.callback!());
  const dialog = document.querySelector<HTMLDialogElement>("dialog");
  expect(dialog).not.toBeNull();
  expect(dialog!.open).toBe(true);
  expect(dialog!.getAttribute("aria-describedby")).toBe("close-dialog-body");
  await act(async () => dialog!.dispatchEvent(new Event("cancel", { cancelable: true })));
  expect(document.querySelector("dialog")).toBeNull();
  expect(document.activeElement).toBe(opener);
  expect(bridge.invoke).not.toHaveBeenCalled();
});
