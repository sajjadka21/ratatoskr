import { describe, expect, it } from "vitest";

import { chartScale, kindTotals, labelledDays } from "./stats";

const day = (bytes: number) => ({ day: "2026-09-24", domesticBytes: bytes, internationalBytes: 0, completed: 0 });

describe("statistics helpers", () => {
  it("folds extensions into the interface's file kinds", () => {
    expect(
      kindTotals([
        { name: "mp4", count: 2, bytes: 900 },
        { name: "zip", count: 1, bytes: 300 },
        { name: "mkv", count: 1, bytes: 200 },
        { name: "", count: 3, bytes: 10 },
      ]),
    ).toEqual([
      { name: "video", count: 3, bytes: 1_100 },
      { name: "archive", count: 1, bytes: 300 },
      { name: "other", count: 3, bytes: 10 },
    ]);
  });

  it("rounds the chart's top in the unit of the largest day", () => {
    expect(chartScale([day(0)])).toEqual({ max: 0, ticks: [] });
    const gb = 1024 ** 3;
    const scale = chartScale([day(3.4 * gb), day(1 * gb)]);
    expect(scale.max).toBe(4 * gb);
    expect(scale.ticks).toEqual([1 * gb, 2 * gb, 3 * gb, 4 * gb]);
    const small = chartScale([day(730 * 1024 * 1024)]);
    expect(small.max).toBe(800 * 1024 * 1024);
    expect(small.ticks.length).toBeLessThanOrEqual(4);
  });

  it("labels at most about seven days, always including today", () => {
    expect([...labelledDays(7)].length).toBe(7);
    const month = labelledDays(30);
    expect(month.has(29)).toBe(true);
    expect(month.size).toBeLessThanOrEqual(8);
  });
});
