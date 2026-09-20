import type { ReactNode } from "react";

import {
  Sidebar,
  type DownloadSection,
  type SidebarCounts,
} from "./Sidebar";

import { TopBar } from "./TopBar";

import "./AppShell.css";

type AppShellProps = {
  children: ReactNode;
  title?: string;
  subtitle?: string;
  activeItem?: DownloadSection;
  counts?: SidebarCounts;
  searchValue: string;
  onSearchChange: (value: string) => void;
  onSectionChange?: (
    section: DownloadSection,
  ) => void;
  onAddDownload?: () => void;
};

export function AppShell({
  children,
  title = "All Downloads",
  subtitle,
  activeItem = "all",
  counts,
  searchValue,
  onSearchChange,
  onSectionChange,
  onAddDownload,
}: AppShellProps) {
  return (
    <div className="app-shell">
      <Sidebar
        activeItem={activeItem}
        counts={counts}
        onSelect={onSectionChange}
      />

      <div className="app-shell__workspace">
        <TopBar
          title={title}
          subtitle={subtitle}
          searchValue={searchValue}
          onSearchChange={onSearchChange}
          onAddDownload={onAddDownload}
        />

        <main className="app-shell__content">
          <div className="app-shell__content-inner">
            {children}
          </div>
        </main>
      </div>
    </div>
  );
}
