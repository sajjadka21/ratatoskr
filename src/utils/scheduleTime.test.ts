import { describe, expect, it } from "vitest";

import { checkOnce, fromLocalInput, toLocalInput } from "./scheduleTime";

describe("schedule time", () => {
  it("round-trips a local date and time", () => {
    const seconds = fromLocalInput("2026-12-05T23:30")!;
    expect(toLocalInput(seconds)).toBe("2026-12-05T23:30");
  });

  it("treats empty and broken values as no date", () => {
    expect(fromLocalInput("")).toBeNull();
    expect(fromLocalInput("tomorrow")).toBeNull();
    expect(toLocalInput(0)).toBe("");
    expect(toLocalInput(null)).toBe("");
  });

  it("needs a start, and an end after it", () => {
    expect(checkOnce("", "")).toBe("needsStart");
    expect(checkOnce("2026-12-05T10:00", "")).toBe("ok");
    expect(checkOnce("2026-12-05T10:00", "2026-12-05T09:00")).toBe("endBeforeStart");
    expect(checkOnce("2026-12-05T10:00", "2026-12-05T10:00")).toBe("endBeforeStart");
    expect(checkOnce("2026-12-05T10:00", "2026-12-06T02:00")).toBe("ok");
  });
});
