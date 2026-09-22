import { describe, expect, it } from "vitest";

import {
  formatBytes,
  formatDuration,
  formatHost,
  formatRate,
} from "./format";

describe("formatBytes", () => {
  it("reports whole bytes without a fraction", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
  });

  it("scales to the largest unit that fits", () => {
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(1024 ** 2)).toBe("1.00 MB");
    expect(formatBytes(1024 ** 3 * 2.5)).toBe("2.50 GB");
  });

  it("never renders a negative or unusable size", () => {
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });
});

describe("formatRate", () => {
  it("omits the readout when nothing has been measured", () => {
    expect(formatRate(null)).toBeNull();
  });

  it("renders a measured rate per second", () => {
    expect(formatRate(1024 * 1024)).toBe("1.00 MB/s");
  });

  it("renders a stalled transfer as zero rather than nothing", () => {
    expect(formatRate(0)).toBe("0 B/s");
  });
});

describe("formatDuration", () => {
  it("omits an estimate that does not exist", () => {
    expect(formatDuration(null)).toBeNull();
    expect(formatDuration(-5)).toBeNull();
  });

  it("uses seconds, minutes and hours as the value grows", () => {
    expect(formatDuration(45)).toBe("45s");
    expect(formatDuration(90)).toBe("1m 30s");
    expect(formatDuration(3_700)).toBe("1h 1m");
  });

  it("refuses to pretend to be precise beyond a day", () => {
    expect(formatDuration(90_000)).toBe("> 1 day");
  });
});

describe("formatHost", () => {
  it("strips the www prefix", () => {
    expect(formatHost("https://www.example.com/a/b.bin")).toBe("example.com");
  });

  it("falls back to the raw value when it is not a URL", () => {
    expect(formatHost("not a url")).toBe("not a url");
  });
});
