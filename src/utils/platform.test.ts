import { expect, it } from "vitest";

import { isLinuxUserAgent } from "./platform";

it("recognises the Linux web view and nothing else", () => {
  expect(isLinuxUserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)")).toBe(true);
  expect(isLinuxUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0 Edg/130.0")).toBe(false);
  expect(isLinuxUserAgent("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36")).toBe(false);
});
