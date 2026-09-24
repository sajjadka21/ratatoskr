import type { Language } from "./messages";

/** Locale tags: Persian uses Persian digits and the Solar Hijri calendar. */
const NUMBER_LOCALE: Record<Language, string> = { en: "en-US", fa: "fa-IR" };
const DATE_LOCALE: Record<Language, string> = {
  en: "en-GB",
  fa: "fa-IR-u-ca-persian",
};

const BYTE_UNITS: Record<Language, string[]> = {
  en: ["B", "KB", "MB", "GB", "TB", "PB"],
  fa: ["بایت", "کیلوبایت", "مگابایت", "گیگابایت", "ترابایت", "پتابایت"],
};

const RATE_SUFFIX: Record<Language, string> = { en: "/s", fa: "/ث" };

export type Formatter = ReturnType<typeof createFormatter>;

export function createFormatter(language: Language) {
  const numberLocale = NUMBER_LOCALE[language];
  const integer = new Intl.NumberFormat(numberLocale, { maximumFractionDigits: 0 });
  const decimals = [0, 1, 2].map(
    (digits) =>
      new Intl.NumberFormat(numberLocale, {
        minimumFractionDigits: 0,
        maximumFractionDigits: digits,
      }),
  );
  const dateTime = new Intl.DateTimeFormat(DATE_LOCALE[language], {
    dateStyle: "medium",
    timeStyle: "short",
  });
  const dayFormat = new Intl.DateTimeFormat(DATE_LOCALE[language], { dateStyle: "long" });
  const shortDayFormat = new Intl.DateTimeFormat(DATE_LOCALE[language], {
    day: "numeric",
    month: language === "fa" ? "long" : "short",
  });
  const time = new Intl.DateTimeFormat(DATE_LOCALE[language], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  function number(value: number, fractionDigits?: number): string {
    if (!Number.isFinite(value)) return "—";
    return fractionDigits === undefined
      ? integer.format(value)
      : decimals[Math.min(2, Math.max(0, fractionDigits))].format(value);
  }

  /** `2.51 GB` / `۲٫۵۱ گیگابایت`. Values under 1 KB have no decimals. */
  function bytes(value: number | null | undefined): string {
    const units = BYTE_UNITS[language];
    if (value === null || value === undefined || !Number.isFinite(value) || value <= 0) {
      return `${number(0)} ${units[0]}`;
    }
    const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
    const scaled = value / 1024 ** index;
    const digits = index === 0 ? 0 : scaled >= 100 ? 0 : scaled >= 10 ? 1 : 2;
    return `${number(scaled, digits)} ${units[index]}`;
  }

  /** A measured rate, or null when nothing was measured yet. */
  function rate(bytesPerSecond: number | null | undefined): string | null {
    if (bytesPerSecond === null || bytesPerSecond === undefined || !Number.isFinite(bytesPerSecond)) {
      return null;
    }
    return `${bytes(bytesPerSecond)}${RATE_SUFFIX[language]}`;
  }

  /** Compact remaining time; anything past a day is not worth precision. */
  function duration(seconds: number | null | undefined): string | null {
    if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
      return null;
    }
    const whole = Math.round(seconds);
    const h = Math.floor(whole / 3600);
    const m = Math.floor((whole % 3600) / 60);
    const s = whole % 60;
    if (language === "fa") {
      if (whole < 60) return `${number(s)} ثانیه`;
      if (whole < 3600) return s ? `${number(m)} دقیقه و ${number(s)} ثانیه` : `${number(m)} دقیقه`;
      if (whole < 86_400) return m ? `${number(h)} ساعت و ${number(m)} دقیقه` : `${number(h)} ساعت`;
      return "بیش از یک روز";
    }
    if (whole < 60) return `${s}s`;
    if (whole < 3600) return `${m}m ${s}s`;
    if (whole < 86_400) return `${h}h ${m}m`;
    return "> 1 day";
  }

  function percent(value: number): string {
    const digits = value >= 100 || value === 0 ? 0 : 1;
    return language === "fa" ? `٪${number(value, digits)}` : `${number(value, digits)}%`;
  }

  function date(unixSeconds: number | null | undefined): string {
    return unixSeconds ? dateTime.format(new Date(unixSeconds * 1000)) : "—";
  }

  /** A calendar day given as `YYYY-MM-DD`: `24 September 2026` / `۲ مهر ۱۴۰۵`. */
  function day(isoDay: string): string {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDay);
    if (!match) return isoDay;
    const [, year, month, date] = match;
    return dayFormat.format(new Date(Number(year), Number(month) - 1, Number(date)));
  }

  /** Day and month only: `24 Sept` / `۲ مهر`. */
  function shortDay(isoDay: string): string {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDay);
    if (!match) return isoDay;
    const [, year, month, date] = match;
    return shortDayFormat.format(new Date(Number(year), Number(month) - 1, Number(date)));
  }

  function clock(unixMillis: number): string {
    return time.format(new Date(unixMillis));
  }

  return { language, number, bytes, rate, duration, percent, date, day, shortDay, clock };
}
