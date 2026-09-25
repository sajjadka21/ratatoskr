import { describe, expect, it } from "vitest";

import { createTranslator } from "../i18n/I18n";
import { categoryName, explainRule } from "./categories";

describe("categories", () => {
  it("names built-in categories in the interface language", () => {
    const fa = createTranslator("fa");
    expect(categoryName("other", "Other", fa)).toBe("سایر");
    expect(categoryName("custom-1", "Courses", fa)).toBe("Courses");
  });

  it("explains where a download went without English in Persian", () => {
    const fa = createTranslator("fa");
    expect(explainRule({ ruleName: null, categoryId: "other", categoryName: "Other" }, fa)).toBe(
      "در دسته‌ی «سایر» قرار می‌گیرد.",
    );
    expect(explainRule({ ruleName: "Courses", categoryId: "video", categoryName: "Video" }, fa)).toContain("Courses");
    expect(explainRule({ ruleName: null, categoryId: null, categoryName: null }, fa)).toContain("پیش‌فرض");
  });
});
