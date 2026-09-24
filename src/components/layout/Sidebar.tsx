import {
  BarChart3,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Download,
  FolderOpen,
  Layers3,
  List,
  Plus,
  ScanLine,
  Settings,
} from "lucide-react";
import type { ComponentType } from "react";

import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";

import "./Sidebar.css";

export type DownloadSection = "all" | "active" | "queued" | "completed" | "failed";
export type WorkspacePage = "downloads" | "linkgrabber" | "categories" | "queues" | "stats" | "settings";
export type SidebarCounts = Record<DownloadSection, number>;

type SidebarProps = {
  page: WorkspacePage;
  section: DownloadSection;
  counts: SidebarCounts;
  bytesToday: number;
  engineState: "ready" | "checking" | "down";
  onSection: (section: DownloadSection) => void;
  onPage: (page: WorkspacePage) => void;
  onAddDownload: () => void;
};

const SECTIONS: Array<{
  id: DownloadSection;
  label: MessageKey;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  tone: string;
}> = [
  { id: "all", label: "nav.all", icon: List, tone: "all" },
  { id: "active", label: "nav.active", icon: Download, tone: "active" },
  { id: "queued", label: "nav.queued", icon: Clock3, tone: "queued" },
  { id: "completed", label: "nav.completed", icon: CheckCircle2, tone: "completed" },
  { id: "failed", label: "nav.failed", icon: CircleAlert, tone: "failed" },
];

const PAGES: Array<{
  id: Exclude<WorkspacePage, "downloads" | "settings">;
  label: MessageKey;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
}> = [
  { id: "linkgrabber", label: "nav.linkGrabber", icon: ScanLine },
  { id: "queues", label: "nav.queues", icon: Layers3 },
  { id: "categories", label: "nav.categories", icon: FolderOpen },
  { id: "stats", label: "nav.stats", icon: BarChart3 },
];

/**
 * The brand mark: Ratatosk, the squirrel of Norse myth who runs up and
 * down the world tree carrying messages, bringing a file down.
 */
export function RatatoskMark({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 34 34" aria-hidden="true" className="brand-mark">
      <rect width="34" height="34" rx="10" fill="var(--accent)" />
      <g fill="var(--accent-ink)">
        <path d="M19.5 27.5c6.5-.2 9.6-5.4 8.6-11.2-.8-4.6-4.9-7.6-8.6-6.3-2.7.9-3.4 4.1-1.5 5.8 1.4 1.3 3.6.9 4.2-.8.3 2.5-.6 5.4-2.7 7.4z" />
        <path d="M9.4 27.5c-.6-3.6.4-7.4 3.3-9.3 2.4-1.6 5.6-1 6.9 1.4 1.3 2.4.8 5.6-.6 7.9z" />
        <ellipse cx="12" cy="14.8" rx="4.3" ry="3.8" />
        <path d="M12.2 11.8c.1-1.6.8-3.1 1.9-3.9.5 1.4.5 3-.1 4.4z" />
      </g>
      <circle cx="10.4" cy="14.2" r=".95" fill="var(--accent)" />
      <path
        d="M7.6 20.2v5.6m-2-2 2 2 2-2"
        stroke="var(--accent-ink)"
        strokeWidth="1.9"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function Sidebar({
  page,
  section,
  counts,
  bytesToday,
  engineState,
  onSection,
  onPage,
  onAddDownload,
}: SidebarProps) {
  const { t, fmt } = useI18n();
  const engineLabel: MessageKey =
    engineState === "ready" ? "nav.engineReady" : engineState === "down" ? "nav.engineDown" : "nav.engineChecking";

  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <RatatoskMark />
        <div className="sidebar__brand-copy">
          <strong>{t("app.name")}</strong>
          <span>{t("app.fullName")}</span>
        </div>
      </div>

      <button type="button" className="sidebar__add" onClick={onAddDownload}>
        <Plus size={17} strokeWidth={2.4} />
        <span>{t("nav.addLink")}</span>
        <kbd className="ltr">Ctrl+N</kbd>
      </button>

      <nav className="sidebar__nav" aria-label={t("nav.sections")}>
        {SECTIONS.map(({ id, label, icon: Icon, tone }) => {
          const active = page === "downloads" && section === id;
          const count = counts[id];
          return (
            <button
              key={id}
              type="button"
              className={`sidebar__item sidebar__item--tone-${tone} ${active ? "sidebar__item--active" : ""}`}
              aria-current={active ? "page" : undefined}
              onClick={() => onSection(id)}
            >
              <Icon size={17} strokeWidth={1.9} />
              <span className="sidebar__label">{t(label)}</span>
              {count > 0 ? <span className="sidebar__count num">{fmt.number(count)}</span> : null}
            </button>
          );
        })}
      </nav>

      <div className="sidebar__group">
        <span className="sidebar__group-label">{t("nav.workspace")}</span>
        {PAGES.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            className={`sidebar__item ${page === id ? "sidebar__item--active" : ""}`}
            aria-current={page === id ? "page" : undefined}
            onClick={() => onPage(id)}
          >
            <Icon size={17} strokeWidth={1.9} />
            <span className="sidebar__label">{t(label)}</span>
          </button>
        ))}
      </div>

      <div className="sidebar__footer">
        <div className="sidebar__stat">
          <span>{t("nav.today")}</span>
          <strong className="num">{fmt.bytes(bytesToday)}</strong>
        </div>
        <div className={`sidebar__engine sidebar__engine--${engineState}`}>
          <span className="sidebar__engine-dot" />
          {t(engineLabel)}
        </div>
        <button
          type="button"
          className={`sidebar__item ${page === "settings" ? "sidebar__item--active" : ""}`}
          aria-current={page === "settings" ? "page" : undefined}
          onClick={() => onPage("settings")}
        >
          <Settings size={17} strokeWidth={1.9} />
          <span className="sidebar__label">{t("nav.settings")}</span>
        </button>
      </div>
    </aside>
  );
}
