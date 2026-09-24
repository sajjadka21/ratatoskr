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

/** The brand mark: two currents flowing down into a download arrow. */
export function RudMark({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 34 34" aria-hidden="true" className="rud-mark">
      <rect width="34" height="34" rx="10" fill="var(--accent)" />
      <path
        d="M8 12.5c3-3 6 3 9 0s6-3 9 0M8 18c3-3 6 3 9 0s6-3 9 0"
        stroke="var(--accent-ink)"
        strokeWidth="2.2"
        fill="none"
        strokeLinecap="round"
      />
      <path
        d="M17 22v5.5m-3-3 3 3 3-3"
        stroke="var(--accent-ink)"
        strokeWidth="2.2"
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
        <RudMark />
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
