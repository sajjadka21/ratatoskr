import { ArrowRight, Command, Download, Search, Zap } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import { searchCommands, type PaletteCommand } from "../../utils/commandSearch";

import "./CommandPalette.css";

const GROUP_ICONS = { action: Zap, go: ArrowRight, download: Download } as const;

/** Ctrl+K: every action and every download, one search away. */
export function CommandPalette({
  open,
  commands,
  onClose,
}: {
  open: boolean;
  commands: PaletteCommand[];
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const results = useMemo(() => searchCommands(commands, query), [commands, query]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  useEffect(() => setActive(0), [query]);

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active]);

  if (!open) return null;

  function run(command: PaletteCommand | undefined) {
    if (!command) return;
    onClose();
    command.run();
  }

  return (
    <div
      className="command-palette__backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="command-palette" role="dialog" aria-modal="true" aria-label={t("palette.label")}>
        <div className="command-palette__search">
          <Search size={16} aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            placeholder={t("palette.placeholder")}
            aria-label={t("palette.placeholder")}
            aria-controls="command-palette-list"
            aria-activedescendant={results[active] ? `palette-${results[active].id}` : undefined}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setActive((index) => Math.min(results.length - 1, index + 1));
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setActive((index) => Math.max(0, index - 1));
              } else if (event.key === "Enter") {
                event.preventDefault();
                run(results[active]);
              } else if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                onClose();
              }
            }}
          />
          <kbd className="ltr">
            <Command size={11} aria-hidden="true" />K
          </kbd>
        </div>
        <ul id="command-palette-list" ref={listRef} className="command-palette__list" role="listbox">
          {results.length === 0 ? (
            <li className="command-palette__empty">{t("palette.empty")}</li>
          ) : (
            results.map((command, index) => {
              const Icon = GROUP_ICONS[command.group];
              return (
                <li
                  key={command.id}
                  id={`palette-${command.id}`}
                  data-index={index}
                  role="option"
                  aria-selected={index === active}
                  className={index === active ? "command-palette__item command-palette__item--active" : "command-palette__item"}
                  onMouseMove={() => setActive(index)}
                  onClick={() => run(command)}
                >
                  <Icon size={15} aria-hidden="true" />
                  <span className="command-palette__label">{command.label}</span>
                  {command.hint ? <span className="command-palette__hint">{command.hint}</span> : null}
                </li>
              );
            })
          )}
        </ul>
      </div>
    </div>
  );
}
