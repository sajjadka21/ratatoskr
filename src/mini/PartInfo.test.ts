import { describe, expect, it } from "vitest";

import { partFraction, partShares, type DownloadPart } from "./PartInfo";

const part = (index: number, total: number, downloaded: number, status = "downloading"): DownloadPart => ({
  index,
  start: 0,
  total,
  downloaded,
  status,
});

describe("part info", () => {
  it("fills a part by its own progress and never beyond", () => {
    expect(partFraction(part(0, 100, 25))).toBe(0.25);
    expect(partFraction(part(0, 100, 250))).toBe(1);
    expect(partFraction(part(0, 0, 10))).toBe(0);
  });

  it("sizes each block by its share of the file", () => {
    const shares = partShares([part(0, 100, 0), part(1, 300, 0)]);
    expect(shares[0]).toBeCloseTo(0.25);
    expect(shares[1]).toBeCloseTo(0.75);
  });

  it("falls back to equal blocks when sizes are unknown", () => {
    expect(partShares([part(0, 0, 0), part(1, 0, 0)])).toEqual([0.5, 0.5]);
  });
});
