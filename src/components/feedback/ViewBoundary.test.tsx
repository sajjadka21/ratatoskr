// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18n";
import { ViewBoundary } from "./ViewBoundary";

let root: ReturnType<typeof createRoot>;
afterEach(async () => { await act(async () => root.unmount()); document.body.replaceChildren(); vi.restoreAllMocks(); });

function Broken(): never {
  throw new Error("render failed");
}

it("replaces a view that throws with a way back instead of a blank window", async () => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.spyOn(console, "error").mockImplementation(() => {});
  document.body.innerHTML = '<div id="mount"></div>';
  root = createRoot(document.querySelector("#mount")!);
  const onBack = vi.fn();
  const render = (page: string, child: React.ReactNode) =>
    root.render(<I18nProvider language="en"><ViewBoundary resetKey={page} onBack={onBack}>{child}</ViewBoundary></I18nProvider>);

  await act(async () => render("settings", <Broken />));
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("This view stopped working");
  const back = [...document.querySelectorAll("button")].find((button) => button.textContent === "Back to downloads");
  await act(async () => back!.click());
  expect(onBack).toHaveBeenCalledTimes(1);

  await act(async () => render("downloads", <p>list</p>));
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(document.body.textContent).toContain("list");
});
