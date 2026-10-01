import { BRAND_THEMES, type BrandTheme, type UiPreferences } from "../types/download";

export type AppearanceMode = "dark" | "light" | "system";
export function resolveAppearance(preferences: UiPreferences, systemDark: boolean) {
  const legacyMode = ["light", "dark", "system"].includes(preferences.theme)
    ? preferences.theme as AppearanceMode : "dark";
  const mode = preferences.appearanceMode ?? legacyMode;
  const brand = BRAND_THEMES.includes(preferences.theme as BrandTheme)
    ? preferences.theme as BrandTheme : "ember-forge";
  const dark = mode === "dark" || (mode === "system" && systemDark);
  return { mode, brand, theme: dark ? brand : "light" as const, dark };
}
export function changeAppearance(preferences: UiPreferences, mode: AppearanceMode): UiPreferences {
  return { ...preferences, theme: resolveAppearance(preferences, true).brand, appearanceMode: mode };
}
export function changeBrand(preferences: UiPreferences, brand: BrandTheme): UiPreferences {
  return { ...preferences, theme: brand, appearanceMode: resolveAppearance(preferences, true).mode };
}
