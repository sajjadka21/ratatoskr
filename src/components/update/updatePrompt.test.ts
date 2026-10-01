import { describe, expect, it } from "vitest";

import { shouldAsk, skipVersion, skippedVersion } from "./updatePrompt";

function memory() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

describe("update prompt", () => {
  it("asks when nothing was skipped", () => {
    expect(shouldAsk({ version: "1.2.0" }, null)).toBe(true);
  });

  it("stays quiet for a skipped version but asks again for a newer one", () => {
    const store = memory();
    skipVersion(store, "1.2.0");
    expect(shouldAsk({ version: "1.2.0" }, skippedVersion(store))).toBe(false);
    expect(shouldAsk({ version: "1.3.0" }, skippedVersion(store))).toBe(true);
  });

  it("never asks about an empty version", () => {
    expect(shouldAsk({ version: " " }, null)).toBe(false);
  });

  it("survives storage that throws", () => {
    const broken = { getItem: () => { throw new Error("no"); }, setItem: () => { throw new Error("no"); } };
    expect(skippedVersion(broken)).toBeNull();
    expect(() => skipVersion(broken, "1.0.0")).not.toThrow();
  });
});
