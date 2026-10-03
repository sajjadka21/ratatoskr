import type { ReactNode } from "react";
import { Power, Search, Settings } from "lucide-react";

import { useI18n } from "../../i18n/I18n";

import "./TopBar.css";

type TopBarProps = {
  title: string;
  subtitle?: string;
  searchValue: string;
  onSearchChange: (value: string) => void;
  showSearch?: boolean;
  actions?: ReactNode;
  onSettingsClick: () => void;
  onQuitClick: () => void;
};

export function TopBar({ title, subtitle, searchValue, onSearchChange, showSearch = true, actions, onSettingsClick, onQuitClick }: TopBarProps) {
  const { t } = useI18n();

  return (
    <header className="topbar">
      <div className="topbar__heading">
        <h1>{title}</h1>
        {subtitle ? <span className="num">{subtitle}</span> : null}
      </div>

      <div className="topbar__actions">
        {actions}
        <button type="button" className="topbar__shortcut" aria-label={t("nav.settings")} title={t("nav.settings")} onClick={onSettingsClick}>
          <Settings size={16} /><span>{t("nav.settings")}</span>
        </button>
        <button type="button" className="topbar__shortcut topbar__shortcut--quit" aria-label={t("close.quit")} title={t("close.quit")} onClick={onQuitClick}>
          <Power size={16} /><span>{t("close.quit")}</span>
        </button>
      </div>
      {showSearch ? (
        <label className="topbar__search">
          <Search size={16} strokeWidth={1.9} />
          <input
            id="download-search"
            type="search"
            value={searchValue}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder={t("top.search")}
            aria-label={t("top.searchLabel")}
          />
          <kbd className="ltr">Ctrl F</kbd>
        </label>
      ) : null}
    </header>
  );
}
