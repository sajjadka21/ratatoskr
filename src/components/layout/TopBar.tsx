import { Search } from "lucide-react";

import { useI18n } from "../../i18n/I18n";

import "./TopBar.css";

type TopBarProps = {
  title: string;
  subtitle?: string;
  searchValue: string;
  onSearchChange: (value: string) => void;
  showSearch?: boolean;
};

export function TopBar({ title, subtitle, searchValue, onSearchChange, showSearch = true }: TopBarProps) {
  const { t } = useI18n();

  return (
    <header className="topbar">
      <div className="topbar__heading">
        <h1>{title}</h1>
        {subtitle ? <span className="num">{subtitle}</span> : null}
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
