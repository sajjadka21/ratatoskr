import type { Language, MessageKey } from "../i18n/messages";
import { messages } from "../i18n/messages";
import type { Translate } from "../i18n/I18n";

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
  if (code === "quota" || code === "interrupted") return t(`notice.${code}`);
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
