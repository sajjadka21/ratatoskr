// @vitest-environment happy-dom

import { computeAccessibleName } from "dom-accessibility-api";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { createTranslator, I18nProvider } from "../../i18n/I18n";
import type { Language, MessageKey } from "../../i18n/messages";
import { Sidebar } from "./Sidebar";

const buttonLabels: MessageKey[] = [
  "nav.addLink", "nav.all", "nav.active", "nav.queued", "nav.completed", "nav.failed",
  "nav.linkGrabber", "nav.queues", "nav.categories", "nav.stats", "nav.settings",
];

afterEach(() => document.body.replaceChildren());

describe.each<Language>(["en", "fa"])("compact sidebar accessible names (%s)", (language) => {
  it("retains a meaningful translated name for every icon-only button", () => {
    document.body.innerHTML = renderToStaticMarkup(
      <I18nProvider language={language}>
        <Sidebar
          page="downloads"
          section="all"
          counts={{ all: 0, active: 0, queued: 0, completed: 0, failed: 0 }}
          bytesToday={0}
          engineState="ready"
          onSection={() => {}}
          onPage={() => {}}
          onAddDownload={() => {}}
        />
      </I18nProvider>,
    );

    // happy-dom has no viewport-driven CSS layout. Apply the compact mode's
    // visibility to rendered label nodes, then use the accessibility-name
    // algorithm, not snapshots of component source or JSX attributes.
    for (const label of document.querySelectorAll<HTMLElement>(
      ".sidebar__label, .sidebar__add span, .sidebar__add kbd, .sidebar__count",
    )) {
      label.style.display = "none";
    }

    const buttons = [...document.querySelectorAll<HTMLButtonElement>("aside button")];
    const t = createTranslator(language);
    expect(buttons).toHaveLength(buttonLabels.length);
    buttons.forEach((button, index) => {
      expect(computeAccessibleName(button), `compact button ${buttonLabels[index]}`).toBe(t(buttonLabels[index]!));
    });
  });
});
