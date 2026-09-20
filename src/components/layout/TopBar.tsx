import { Plus, Search } from "lucide-react";

import "./TopBar.css";

type TopBarProps = {
  title: string;
  subtitle?: string;
  searchValue: string;
  onSearchChange: (value: string) => void;
  onAddDownload?: () => void;
};

export function TopBar({
  title,
  subtitle,
  searchValue,
  onSearchChange,
  onAddDownload,
}: TopBarProps) {
  return (
    <header className="topbar">
      <div className="topbar__heading">
        <h1>{title}</h1>
        {subtitle ? <span>{subtitle}</span> : null}
      </div>

      <div className="topbar__actions">
        <label className="topbar__search">
          <Search size={16} strokeWidth={1.9} />

          <input
            type="search"
            value={searchValue}
            onChange={(event) =>
              onSearchChange(event.target.value)
            }
            placeholder="Search downloads..."
            aria-label="Search downloads"
          />
        </label>

        <button
          className="topbar__add"
          type="button"
          onClick={onAddDownload}
        >
          <Plus size={17} strokeWidth={2.2} />
          <span>Add Download</span>
        </button>
      </div>
    </header>
  );
}
