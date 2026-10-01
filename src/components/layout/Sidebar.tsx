import { Wordmark } from "../../brand/Wordmark";
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
 * The brand mark: the runic tree from the Ratatoskr wordmark, the world tree
 * the squirrel runs up and down carrying messages.
 */
export function RatatoskMark({ size = 34 }: { size?: number }) {
  return <span className="brand-squirrel brand-mark" style={{ width: size, height: size }} role="img" aria-label="Ratatoskr" />;
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
        <RatatoskMark size={44} />
        <div className="sidebar__brand-copy">
          <Wordmark height={24} className="brand-wordmark" />
          <span>{t("app.fullName")}</span>
        </div>
      </div>

      <button type="button" className="sidebar__add" aria-label={t("nav.addLink")} title={t("nav.addLink")} onClick={onAddDownload}>
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
              aria-label={t(label)}
              title={t(label)}
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
            aria-label={t(label)}
            title={t(label)}
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
          aria-label={t("nav.settings")}
          title={t("nav.settings")}
          onClick={() => onPage("settings")}
        >
          <Settings size={17} strokeWidth={1.9} />
          <span className="sidebar__label">{t("nav.settings")}</span>
        </button>
      </div>
    </aside>
  );
}
