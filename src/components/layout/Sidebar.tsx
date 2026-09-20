import {
  CheckCircle2,
  CircleX,
  Clock3,
  Download,
  List,
  Settings,
  Zap,
} from "lucide-react";

import "./Sidebar.css";

export type DownloadSection =
  | "all"
  | "active"
  | "queued"
  | "completed"
  | "failed";

export type SidebarCounts =
  Record<DownloadSection, number>;

type SidebarProps = {
  activeItem?: DownloadSection;
  counts?: SidebarCounts;
  onSelect?: (section: DownloadSection) => void;
};

const items: Array<{
  id: DownloadSection;
  label: string;
  icon: typeof List;
}> = [
  { id: "all", label: "All Downloads", icon: List },
  { id: "active", label: "Active", icon: Zap },
  { id: "queued", label: "Queued", icon: Clock3 },
  { id: "completed", label: "Completed", icon: CheckCircle2 },
  { id: "failed", label: "Failed", icon: CircleX },
];

export function Sidebar({
  activeItem = "all",
  counts,
  onSelect,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <div className="sidebar__brand-icon">
          <Download size={18} strokeWidth={2.4} />
        </div>

        <div className="sidebar__brand-copy">
          <div className="sidebar__brand-title">
            Download
          </div>

          <div className="sidebar__brand-subtitle">
            Manager
          </div>
        </div>
      </div>

      <nav
        className="sidebar__nav"
        aria-label="Download sections"
      >
        {items.map(({ id, label, icon: Icon }) => {
          const count = counts?.[id] ?? 0;

          return (
            <button
              key={id}
              type="button"
              className={`sidebar__item ${
                activeItem === id
                  ? "sidebar__item--active"
                  : ""
              }`}
              aria-current={
                activeItem === id
                  ? "page"
                  : undefined
              }
              onClick={() => onSelect?.(id)}
            >
              <Icon size={17} strokeWidth={1.9} />

              <span className="sidebar__item-label">
                {label}
              </span>

              {count > 0 ? (
                <span className="sidebar__count">
                  {count}
                </span>
              ) : null}
            </button>
          );
        })}
      </nav>

      <div className="sidebar__footer">
        <button
          className="sidebar__item"
          type="button"
        >
          <Settings size={17} strokeWidth={1.9} />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  );
}
