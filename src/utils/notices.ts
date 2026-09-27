import type { Language, MessageKey } from "../i18n/messages";
import { messages } from "../i18n/messages";
import type { Translate } from "../i18n/I18n";

const TRANSLATED_NOTICES = new Set([
  "quota",
  "interrupted",
  "link_expired",
  "link_refreshed",
  "protected_stream",
  "live_stream",
  "needs_muxing",
  "unsupported_stream",
  "ffmpeg_failed",
  "integrity_failed",
  "threat_found",
  "stream_error",
  "rate_limited",
  "server_busy",
  "server_error",
  "not_found",
  "access_denied",
  "http_refused",
  "network_error",
  "filesystem_error",
  "incomplete_transfer",
  "invalid_range_response",
  "segment_overflow",
  "storage_error",
  "invalid_source",
  "download_error",
  "needs_ytdlp",
  "ytdlp_failed",
]);

/**
 * The engine's own English wording, worth showing beside the translated
 * notice only when it adds something (a file-system reason, say).
 */
export function noticeDetail(code: string | null, message: string | null): string | null {
  if (!message || !code || !TRANSLATED_NOTICES.has(code)) return null;
  return ["filesystem_error", "http_refused", "server_error", "download_error", "ytdlp_failed"].includes(code) ? message : null;
}

/**
 * Notices about a finished file that must stay visible: the file is
 * damaged or dangerous even though the download itself completed.
 */
export function isAlarm(code: string | null): boolean {
  return code === "integrity_failed" || code === "threat_found";
}

/**
 * The text shown for a task's notice. The engine writes its notices in
 * English; the ones the interface knows by code are shown translated.
 */
export function noticeText(
  code: string | null,
  message: string | null,
  t: Translate,
  language: Language,
): string | null {
  if (!message) return null;
  if (code && TRANSLATED_NOTICES.has(code)) return t(`notice.${code}` as MessageKey);
  // The restart message names the exact cause; Persian gets the general
  // explanation rather than an English sentence.
  if (code === "restarted" && language === "fa") return t("notice.restarted");
  return message;
}

/** The adaptive engine's reason, in the interface language when known. */
export function engineReasonText(reason: string | null, t: Translate): string | null {
  if (!reason) return null;
  const key = `engine.reason.${reason}`;
  return key in messages.en ? t(key as MessageKey) : reason;
}

/** Backend refusals the user can meet, by the start of their English text. */
const COMMAND_ERRORS: [RegExp, MessageKey][] = [
  [/cannot be removed while its status is/i, "error.removeRunning"],
  [/cannot remove download while status is/i, "error.removeRunning"],
  [/invalid download state transition/i, "error.stateChanged"],
  [/^download not found/i, "error.downloadGone"],
  [/^queue not found/i, "error.queueGone"],
  [/failed to delete downloaded file/i, "error.deleteFailed"],
  [/database mutex is poisoned|sqlite error/i, "error.database"],
  [/download engine is (not ready|unavailable)|download execution is unavailable/i, "error.engineBusy"],
];

/**
 * A readable message for an error a command returned. Messages that are
 * already the user's language, or that are not recognized, are kept.
 */
export function friendlyError(message: string, t: Translate): string {
  const text = message.replace(/^Error:\s*/, "");
  const known = COMMAND_ERRORS.find(([pattern]) => pattern.test(text));
  return known ? t(known[1]) : message;
}
