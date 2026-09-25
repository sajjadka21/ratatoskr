import type { Translate } from "../i18n/I18n";
import type { MessageKey } from "../i18n/messages";

const BUILT_IN = new Set(["applications", "archives", "documents", "video", "audio", "images", "other"]);

/** Built-in categories are named in the interface language; custom ones as typed. */
export function categoryName(id: string | null, name: string | null, t: Translate): string {
  if (id && BUILT_IN.has(id)) return t(`category.${id}` as MessageKey);
  return name ?? id ?? "";
}

/** Why a download is filed where it is, as the engine decided it. */
export type RuleExplanation = {
  ruleName: string | null;
  categoryId: string | null;
  categoryName: string | null;
};

export function explainRule(explanation: RuleExplanation, t: Translate): string {
  if (explanation.ruleName) return t("details.ruleMatched", { name: explanation.ruleName });
  if (explanation.categoryId) {
    return t("details.categoryMatched", {
      name: categoryName(explanation.categoryId, explanation.categoryName, t),
    });
  }
  return t("details.noCategory");
}
