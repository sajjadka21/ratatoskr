// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { I18nProvider } from "../../i18n/I18n";
import { ThroughputBand } from "./ThroughputBand";

let root: ReturnType<typeof createRoot>;
afterEach(async () => { await act(async () => root.unmount()); document.body.replaceChildren(); });

async function renderBand(bytesPerSecond: number, activeCount: number) {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  document.body.innerHTML = '<div id="mount"></div>';
  root = createRoot(document.querySelector("#mount")!);
  await act(async () => root.render(<I18nProvider language="fa"><ThroughputBand
    bytesPerSecond={bytesPerSecond} history={[]} activeCount={activeCount} queuedCount={0} etaSeconds={null}
    resumableCount={0} speedLimit={null} onSetSpeedLimit={() => {}} onPauseAll={() => {}} onResumeAll={() => {}} /></I18nProvider>));
  return document.querySelector(".throughput-band__value")!.textContent;
}

it("shows a dash until the first bytes arrive, then the measured speed", async () => {
  expect(await renderBand(0, 1)).toBe("—");
  await act(async () => root.unmount());
  expect(await renderBand(2 * 1024 * 1024, 1)).toContain("مگابایت");
  await act(async () => root.unmount());
  expect(await renderBand(0, 0)).toBe("دانلود فعالی نیست");
});
