import type { Formatter } from "../i18n/format";
import type { StreamVariant } from "../types/download";

/** An HLS playlist link, judged by its path. */
export function isStreamLink(url: string): boolean {
  try {
    const parsed = new URL(url.trim());
    return /^https?:$/.test(parsed.protocol) && /\.m3u8$/i.test(parsed.pathname);
  } catch {
    return false;
  }
}

/** `1080p`, with Persian digits in Persian and no thousands separator. */
export function heightLabel(height: number, language: string): string {
  const digits = String(height);
  return `${language === "fa" ? digits.replace(/[0-9]/g, (digit) => "۰۱۲۳۴۵۶۷۸۹"[Number(digit)]) : digits}p`;
}

/** `720p · 2.4 Mb/s`, or whatever of that the playlist says. */
export function variantLabel(variant: StreamVariant, fmt: Formatter): string {
  const parts: string[] = [];
  if (variant.height) parts.push(heightLabel(variant.height, fmt.language));
  if (variant.bandwidth) {
    const megabits = variant.bandwidth / 1_000_000;
    parts.push(`${fmt.number(megabits, megabits >= 10 ? 0 : 1)} Mb/s`);
  }
  return parts.join(" · ") || "—";
}
