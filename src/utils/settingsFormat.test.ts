import { describe, expect, it } from "vitest";

import {
  bytesToKibPerSecond,
  formatMinuteOfDay,
  kibPerSecondToBytes,
  parseMinuteOfDay,
} from "./settingsFormat";

describe("schedule times", () => {
  it("round-trips wall-clock times", () => {
    for (const value of ["00:00", "02:00", "07:30", "23:59"]) {
      expect(formatMinuteOfDay(parseMinuteOfDay(value)!)).toBe(value);
    }
  });

  it("rejects values that are not a time of day", () => {
    expect(parseMinuteOfDay("24:00")).toBeNull();
    expect(parseMinuteOfDay("7:5")).toBeNull();
    expect(parseMinuteOfDay("")).toBeNull();
  });

  it("wraps minutes into a single day", () => {
    expect(formatMinuteOfDay(1440 + 90)).toBe("01:30");
  });
});

describe("speed limits", () => {
  it("converts between KB/s and bytes per second", () => {
    expect(kibPerSecondToBytes(1024)).toBe(1_048_576);
    expect(bytesToKibPerSecond(1_048_576)).toBe(1024);
    expect(bytesToKibPerSecond(null)).toBeNull();
    expect(bytesToKibPerSecond(0)).toBeNull();
  });
});
