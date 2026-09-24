import { createContext, useContext, useMemo, type ReactNode } from "react";

import { createFormatter, type Formatter } from "./format";
import { messages, type Language, type MessageKey } from "./messages";

export type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

type I18nValue = {
  language: Language;
  dir: "rtl" | "ltr";
  t: Translate;
  fmt: Formatter;
};

export function createTranslator(language: Language): Translate {
  const table = messages[language];
  return (key, values) => {
    const template: string = table[key] ?? messages.en[key] ?? key;
    if (!values) return template;
    return template.replace(/\{(\w+)\}/g, (match, name: string) =>
      name in values ? String(values[name]) : match,
    );
  };
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ language, children }: { language: Language; children: ReactNode }) {
  const value = useMemo<I18nValue>(
    () => ({
      language,
      dir: language === "fa" ? "rtl" : "ltr",
      t: createTranslator(language),
      fmt: createFormatter(language),
    }),
    [language],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error("useI18n must be used inside I18nProvider");
  }
  return value;
}
