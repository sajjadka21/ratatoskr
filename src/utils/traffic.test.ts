import { describe, expect, it } from "vitest";

import {
  GIB,
  bytesToGigabytes,
  domesticShare,
  gigabytesToBytes,
  localDay,
  quotaState,
  toLatinDigits,
} from "./traffic";
import type { TrafficSummary } from "../types/download";

function summary(used: number, quota: number | null): TrafficSummary {
  return {
    periodStart: "2026-09-01",
    explicitPeriod: true,
    periodDomesticBytes: 0,
    periodInternationalBytes: used,
    todayDomesticBytes: 0,
    todayInternationalBytes: 0,
    monthDomesticBytes: 0,
    monthInternationalBytes: 0,
    internationalQuota: quota,
  };
}

describe("quota input", () => {
  it("accepts Persian digits and the Persian decimal separator", () => {
    expect(toLatinDigits("۲۰")).toBe("20");
    expect(gigabytesToBytes("۱٫۵")).toBe(Math.round(1.5 * GIB));
    expect(gigabytesToBytes("20")).toBe(20 * GIB);
  });

  it("refuses empty, zero and negative quotas", () => {
    expect(gigabytesToBytes("")).toBeNull();
    expect(gigabytesToBytes("0")).toBeNull();
    expect(gigabytesToBytes("-3")).toBeNull();
    expect(gigabytesToBytes("abc")).toBeNull();
  });

  it("shows stored quotas in gigabytes", () => {
    expect(bytesToGigabytes(20 * GIB)).toBe("20");
    expect(bytesToGigabytes(null)).toBe("");
  });
});

describe("quota state", () => {
  it("is absent without a quota", () => {
    expect(quotaState(summary(10, null))).toBeNull();
  });

  it("warns from 80% and reports a used-up quota", () => {
    expect(quotaState(summary(50, 100))?.level).toBe("ok");
    expect(quotaState(summary(85, 100))?.level).toBe("warning");
    const reached = quotaState(summary(130, 100));
    expect(reached?.level).toBe("reached");
    expect(reached?.ratio).toBe(1);
    expect(reached?.left).toBe(0);
  });
});

describe("helpers", () => {
  it("formats the local day", () => {
    expect(localDay(new Date(2026, 8, 4))).toBe("2026-09-04");
  });

  it("splits traffic safely when nothing was downloaded", () => {
    expect(domesticShare(0, 0)).toBe(0);
    expect(domesticShare(1, 3)).toBe(0.25);
  });
});
