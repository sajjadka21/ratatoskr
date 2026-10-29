import { describe, expect, it } from "vitest";

import { createFormatter } from "./format";
import { createTranslator } from "./I18n";
import { messages } from "./messages";

describe("translations", () => {
  it("has a Persian string for every English key", () => {
    const missing = Object.keys(messages.en).filter(
      (key) => !(key in messages.fa) || !messages.fa[key as keyof typeof messages.fa],
    );
    expect(missing).toEqual([]);
  });

  it("keeps the same placeholders in both languages", () => {
    const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    const mismatched = Object.entries(messages.en).filter(
      ([key, text]) =>
        placeholders(text).join() !==
        placeholders(messages.fa[key as keyof typeof messages.fa]).join(),
    );
    expect(mismatched.map(([key]) => key)).toEqual([]);
  });

  it("fills placeholders", () => {
    const t = createTranslator("en");
    expect(t("table.items", { count: 3 })).toBe("3 items");
  });
});

describe("counts", () => {
  it("says one thing in English without a plural, and leaves Persian as it is", () => {
    const en = createTranslator("en");
    const fa = createTranslator("fa");
    expect(en("table.items", { count: "1" })).toBe("1 item");
    expect(en("table.items", { count: "8" })).toBe("8 items");
    expect(en("add.duplicateCompleted", { count: "1" })).toBe("Already downloaded: 1 completed download is in your history.");
    expect(fa("table.items", { count: "۱" })).toBe("۱ مورد");
  });
});

describe("Persian formatting", () => {
  const fa = createFormatter("fa");

  it("uses Persian digits and unit names", () => {
    expect(fa.bytes(1_048_576)).toBe("۱ مگابایت");
    expect(fa.bytes(2.5 * 1024 ** 3)).toBe("۲٫۵ گیگابایت");
    expect(fa.rate(11.4 * 1_048_576)).toBe("۱۱٫۴ مگابایت/ث");
    expect(fa.percent(43.5)).toBe("٪۴۳٫۵");
  });

  it("writes durations in words", () => {
    expect(fa.duration(35)).toBe("۳۵ ثانیه");
    expect(fa.duration(307)).toBe("۵ دقیقه و ۷ ثانیه");
    expect(fa.duration(7200)).toBe("۲ ساعت");
    expect(fa.duration(null)).toBeNull();
  });

  it("uses the Solar Hijri calendar", () => {
    // 2026-09-23 is 1 Mehr 1405.
    expect(fa.date(1_790_121_600 + 12 * 3600)).toContain("۱۴۰۵");
  });
});

describe("English formatting", () => {
  const en = createFormatter("en");
  it("keeps the existing compact style", () => {
    expect(en.bytes(2.51 * 1024 ** 3)).toBe("2.51 GB");
    expect(en.duration(307)).toBe("5m 7s");
    expect(en.rate(null)).toBeNull();
  });
});

describe("calendar days", () => {
  it("shows a stored day in the Solar Hijri calendar in Persian", () => {
    const fa = createFormatter("fa");
    const en = createFormatter("en");
    expect(fa.day("2026-09-24")).toContain("مهر");
    expect(fa.day("2026-09-24")).toContain("۱۴۰۵");
    expect(en.day("2026-09-24")).toContain("2026");
    expect(en.day("not a day")).toBe("not a day");
  });
});
