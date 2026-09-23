const KIB = 1024;

/** `02:00` for 120. Values wrap into one day. */
export function formatMinuteOfDay(minute: number): string {
  const normalized = ((Math.round(minute) % 1440) + 1440) % 1440;
  const hours = Math.floor(normalized / 60);
  const minutes = normalized % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

/** Minutes after midnight for `HH:MM`, or null when it is not a time. */
export function parseMinuteOfDay(value: string): number | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!match) return null;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (hours > 23 || minutes > 59) return null;
  return hours * 60 + minutes;
}

/** Whole KB/s shown in Settings for a limit stored in bytes per second. */
export function bytesToKibPerSecond(bytes: number | null): number | null {
  return bytes === null || bytes <= 0 ? null : Math.max(1, Math.round(bytes / KIB));
}

export function kibPerSecondToBytes(kib: number): number {
  return Math.max(1, Math.round(kib * KIB));
}
