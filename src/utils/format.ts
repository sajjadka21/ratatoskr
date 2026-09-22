const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/**
 * Formats a byte count for display. Sub-kilobyte values keep no decimals
 * because a fractional byte is meaningless.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }

  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    BYTE_UNITS.length - 1,
  );

  const value = bytes / 1024 ** index;

  return `${value.toFixed(index === 0 ? 0 : 2)} ${BYTE_UNITS[index]}`;
}

/**
 * Formats a measured transfer rate. Returns null when there is nothing
 * measured yet, so callers can omit the readout instead of showing a
 * placeholder rate that was never observed.
 */
export function formatRate(bytesPerSecond: number | null): string | null {
  if (bytesPerSecond === null || !Number.isFinite(bytesPerSecond)) {
    return null;
  }

  return `${formatBytes(bytesPerSecond)}/s`;
}

/**
 * Formats a remaining-time estimate as a compact duration. Anything at or
 * beyond a day is reported as "> 1 day" rather than a precise number nobody
 * would trust.
 */
export function formatDuration(seconds: number | null): string | null {
  if (
    seconds === null ||
    !Number.isFinite(seconds) ||
    seconds < 0
  ) {
    return null;
  }

  const whole = Math.round(seconds);

  if (whole < 60) {
    return `${whole}s`;
  }

  if (whole < 3600) {
    return `${Math.floor(whole / 60)}m ${whole % 60}s`;
  }

  if (whole < 86_400) {
    const hours = Math.floor(whole / 3600);
    return `${hours}h ${Math.floor((whole % 3600) / 60)}m`;
  }

  return "> 1 day";
}

/**
 * Host label for a download row. Falls back to the raw value so an
 * unparseable URL still shows something meaningful.
 */
export function formatHost(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}
