import type { ReactNode } from "react";

import { Sidebar, type DownloadSection, type SidebarCounts, type WorkspacePage } from "./Sidebar";
import { TopBar } from "./TopBar";

import "./AppShell.css";

type AppShellProps = {
  children: ReactNode;
  title: string;
  subtitle?: string;
  page: WorkspacePage;
  section: DownloadSection;
  counts: SidebarCounts;
  bytesToday: number;
  engineState: "ready" | "checking" | "down";
  searchValue: string;
  onSearchChange: (value: string) => void;
  onSection: (section: DownloadSection) => void;
  onPage: (page: WorkspacePage) => void;
  onAddDownload: () => void;
  /// Pages with their own scroll and layout (the download table) fill the
  /// workspace instead of scrolling inside a padded column.
  fill?: boolean;
};

export function AppShell({
  children,
  title,
  subtitle,
  page,
  section,
  counts,
  bytesToday,
  engineState,
  searchValue,
  onSearchChange,
  onSection,
  onPage,
  onAddDownload,
  fill = false,
}: AppShellProps) {
  return (
    <div className="app-shell">
      <Sidebar
        page={page}
        section={section}
        counts={counts}
        bytesToday={bytesToday}
        engineState={engineState}
        onSection={onSection}
        onPage={onPage}
        onAddDownload={onAddDownload}
      />

      <div className="app-shell__workspace">
        <TopBar
          title={title}
          subtitle={subtitle}
          searchValue={searchValue}
          onSearchChange={onSearchChange}
          showSearch={page === "downloads"}
        />

        <main className={fill ? "app-shell__content app-shell__content--fill" : "app-shell__content"}>
          {fill ? children : <div className="app-shell__content-inner">{children}</div>}
        </main>
      </div>
    </div>
  );
}
