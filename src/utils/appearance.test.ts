import { describe, expect, it } from "vitest";

import type { BrandTheme, UiPreferences } from "../types/download";
import { changeAppearance, changeBrand, resolveAppearance } from "./appearance";

function preferences(theme: UiPreferences["theme"], appearanceMode?: UiPreferences["appearanceMode"]): UiPreferences {
  return { language: "fa", theme, closeToTray: true, ...(appearanceMode === undefined ? {} : { appearanceMode }) };
}

const brands: BrandTheme[] = ["ember-forge", "midnight-arcane", "forest-rune", "frost-byte"];

describe("backward-compatible appearance", () => {
  it.each(brands)("keeps the old %s brand in dark mode", (brand) => {
    expect(resolveAppearance(preferences(brand), false)).toEqual({
      mode: "dark", brand, theme: brand, dark: true,
    });
  });

  it("keeps legacy light users in light mode with a usable brand asset", () => {
    expect(resolveAppearance(preferences("light"), true)).toEqual({
      mode: "light", brand: "ember-forge", theme: "light", dark: false,
    });
  });

  it("legacy dark does not unexpectedly follow a light system setting", () => {
    expect(resolveAppearance(preferences("dark"), false)).toEqual({
      mode: "dark", brand: "ember-forge", theme: "ember-forge", dark: true,
    });
  });

  it("legacy system mode follows the system without forgetting the selected mode", () => {
    const old = preferences("system");
    expect(resolveAppearance(old, false)).toEqual({
      mode: "system", brand: "ember-forge", theme: "light", dark: false,
    });
    expect(resolveAppearance(old, true)).toEqual({
      mode: "system", brand: "ember-forge", theme: "ember-forge", dark: true,
    });
  });

  it("a null migration field behaves like a missing field", () => {
    for (const legacy of ["light", "dark", "system", "midnight-arcane"] as const) {
      expect(resolveAppearance(preferences(legacy, null), false))
        .toEqual(resolveAppearance(preferences(legacy), false));
    }
  });
});

describe("independent brand and display mode", () => {
  it.each(brands)("retains %s while rendering a light surface", (brand) => {
    expect(resolveAppearance(preferences(brand, "light"), true)).toEqual({
      mode: "light", brand, theme: "light", dark: false,
    });
  });

  it("an explicit dark mode overrides a legacy light theme", () => {
    expect(resolveAppearance(preferences("light", "dark"), false)).toEqual({
      mode: "dark", brand: "ember-forge", theme: "ember-forge", dark: true,
    });
  });

  it("system changes update the surface while retaining the Frost brand", () => {
    const saved = preferences("frost-byte", "system");
    expect(resolveAppearance(saved, false)).toEqual({
      mode: "system", brand: "frost-byte", theme: "light", dark: false,
    });
    expect(resolveAppearance(saved, true)).toEqual({
      mode: "system", brand: "frost-byte", theme: "frost-byte", dark: true,
    });
  });

  it("switching to light keeps a brand-bearing saved theme", () => {
    const previous = preferences("forest-rune");
    const changed = changeAppearance(previous, "light");
    expect(changed).toEqual({ ...previous, theme: "forest-rune", appearanceMode: "light" });
    expect(resolveAppearance(changed, true).brand).toBe("forest-rune");
    expect(resolveAppearance(changed, true).dark).toBe(false);
  });

  it("choosing a brand does not switch a legacy light user to dark", () => {
    const changed = changeBrand(preferences("light"), "midnight-arcane");
    expect(changed).toEqual({ language: "fa", closeToTray: true,
      theme: "midnight-arcane", appearanceMode: "light" });
    expect(resolveAppearance(changed, true)).toEqual({
      mode: "light", brand: "midnight-arcane", theme: "light", dark: false,
    });
  });

  it("choosing a brand preserves legacy system mode", () => {
    const changed = changeBrand(preferences("system"), "forest-rune");
    expect(changed.appearanceMode).toBe("system");
    expect(resolveAppearance(changed, false).theme).toBe("light");
    expect(resolveAppearance(changed, true).theme).toBe("forest-rune");
  });

  it("toggling light, another brand, and system does not lose either choice", () => {
    const original = preferences("midnight-arcane");
    const light = changeAppearance(original, "light");
    const frost = changeBrand(light, "frost-byte");
    const automatic = changeAppearance(frost, "system");
    expect(resolveAppearance(frost, true)).toEqual({
      mode: "light", brand: "frost-byte", theme: "light", dark: false,
    });
    expect(resolveAppearance(automatic, false)).toEqual({
      mode: "system", brand: "frost-byte", theme: "light", dark: false,
    });
    expect(resolveAppearance(automatic, true)).toEqual({
      mode: "system", brand: "frost-byte", theme: "frost-byte", dark: true,
    });
    expect(original).toEqual(preferences("midnight-arcane"));
    expect(light.theme).toBe("midnight-arcane");
  });

  it("appearance changes preserve language and window behaviour", () => {
    const original: UiPreferences = { language: "en", theme: "light", closeToTray: false };
    expect(changeAppearance(original, "dark")).toEqual({
      language: "en", theme: "ember-forge", closeToTray: false, appearanceMode: "dark",
    });
    expect(changeBrand(original, "frost-byte")).toEqual({
      language: "en", theme: "frost-byte", closeToTray: false, appearanceMode: "light",
    });
  });
});
