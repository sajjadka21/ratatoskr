// @vitest-environment happy-dom
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it } from "vitest";
import { I18nProvider } from "../../i18n/I18n";
import type { DownloadQueue } from "../../types/download";
import { QuickQueueControls } from "./QuickQueueControls";

afterEach(() => document.body.replaceChildren());
it.each(["running", "idle"])("offers only the valid queue action while %s", (state) => {
  const queue = { id: "default", name: "Queue", enabled: true, state } as DownloadQueue;
  document.body.innerHTML = renderToStaticMarkup(<I18nProvider language="en"><QuickQueueControls queues={[queue]} selected={queue} busy={false} ready onSelect={() => {}} onToggle={() => {}} onManage={() => {}} onSchedule={() => {}} /></I18nProvider>);
  expect(document.querySelector("details")?.open).toBe(false);
  const buttons = [...document.querySelectorAll("button")];
  expect(buttons[0].disabled).toBe(state === "running");
  expect(buttons[1].disabled).toBe(state !== "running");
  expect(document.querySelector("summary")?.textContent).toBe("Queue controls");
});
