import type { TrafficSummary } from "../types/download";

export const GIB = 1024 * 1024 * 1024;

/** A quota typed in gigabytes, as bytes; null when it is not a positive number. */
export function gigabytesToBytes(text: string): number | null {
  const normalized = toLatinDigits(text).replace(/[٫,]/g, ".").trim();
  if (!normalized) return null;
  const value = Number(normalized);
  if (!Number.isFinite(value) || value <= 0) return null;
  return Math.round(value * GIB);
}

export function bytesToGigabytes(bytes: number | null): string {
  if (!bytes) return "";
  return String(Math.round((bytes / GIB) * 100) / 100);
}

/** Persian and Arabic-Indic digits typed into a field, as Latin digits. */
export function toLatinDigits(text: string): string {
  return text
    .replace(/[۰-۹]/g, (digit) => String(digit.charCodeAt(0) - 0x06f0))
    .replace(/[٠-٩]/g, (digit) => String(digit.charCodeAt(0) - 0x0660));
}

export type QuotaState = {
  used: number;
  quota: number;
  left: number;
  /** 0 to 1. */
  ratio: number;
  level: "ok" | "warning" | "reached";
};

/** Where the international quota stands, or null when there is none. */
export function quotaState(summary: TrafficSummary | null): QuotaState | null {
  if (!summary?.internationalQuota) return null;
  const quota = summary.internationalQuota;
  const used = summary.periodInternationalBytes;
  const ratio = Math.min(1, used / quota);
  return {
    used,
    quota,
    left: Math.max(0, quota - used),
    ratio,
    level: used >= quota ? "reached" : ratio >= 0.8 ? "warning" : "ok",
  };
}

/** Today's date as `YYYY-MM-DD` in local time. */
export function localDay(date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Share of domestic traffic, 0 to 1, for the split bar. */
export function domesticShare(domestic: number, international: number): number {
  const total = domestic + international;
  return total > 0 ? domestic / total : 0;
}
