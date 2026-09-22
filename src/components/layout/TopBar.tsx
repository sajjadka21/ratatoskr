import { Gauge, Plus, Search } from "lucide-react";

import { formatRate } from "../../utils/format";

import "./TopBar.css";

type TopBarProps = {
  title: string;
  subtitle?: string;
  searchValue: string;
  activeCount?: number;
  aggregateBytesPerSecond?: number | null;
  onSearchChange: (value: string) => void;
  onAddDownload?: () => void;
};

export function TopBar({
  title,
  subtitle,
  searchValue,
  activeCount = 0,
  aggregateBytesPerSecond = null,
  onSearchChange,
  onAddDownload,
}: TopBarProps) {
  const aggregateRate = formatRate(aggregateBytesPerSecond);
  return (
    <header className="topbar">
      <div className="topbar__heading">
        <h1>{title}</h1>
        {subtitle ? <span>{subtitle}</span> : null}
      </div>

      <div className="topbar__actions">
        {aggregateRate ? (
          <div
            className="topbar__throughput"
            title={`${activeCount} active ${
              activeCount === 1 ? "transfer" : "transfers"
            }`}
          >
            <Gauge size={15} strokeWidth={1.9} />
            <span>{aggregateRate}</span>
          </div>
        ) : null}

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
