import { describe, expect, it } from "vitest";

import { createTranslator } from "../i18n/I18n";
import { isAlarm, noticeText } from "./notices";

describe("notices", () => {
  it("keeps warnings about a finished file visible", () => {
    expect(isAlarm("integrity_failed")).toBe(true);
    expect(isAlarm("threat_found")).toBe(true);
    expect(isAlarm("link_refreshed")).toBe(false);
    expect(isAlarm(null)).toBe(false);
  });

  it("translates the after-download notices by code", () => {
    const fa = createTranslator("fa");
    expect(noticeText("threat_found", "Windows Defender found a threat", fa, "fa")).toContain("تهدید");
    expect(noticeText("integrity_failed", "does not match", fa, "fa")).toContain("چک‌سام");
  });

  it("shows unknown notices as the engine wrote them", () => {
    const en = createTranslator("en");
    expect(noticeText("something_new", "Plain text", en, "en")).toBe("Plain text");
    expect(noticeText("threat_found", null, en, "en")).toBeNull();
  });
});
