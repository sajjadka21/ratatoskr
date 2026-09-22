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
  settingsActive?: boolean;
  queuesActive?: boolean;
  linkGrabberActive?: boolean;
  categoriesActive?: boolean;

  counts?: SidebarCounts;

  searchValue: string;

  activeCount?: number;
  aggregateBytesPerSecond?: number | null;

  onSearchChange: (
    value: string,
  ) => void;

  onSectionChange?: (
    section: DownloadSection,
  ) => void;

  onOpenSettings?: () => void;
  onOpenQueues?: () => void;
  onOpenLinkGrabber?: () => void;
  onOpenCategories?: () => void;
  onAddDownload?: () => void;
};

export function AppShell({
  children,
  title = "All Downloads",
  subtitle,
  activeItem = "all",
  settingsActive = false,
  queuesActive = false,
  linkGrabberActive = false,
  categoriesActive = false,
  counts,
  searchValue,
  activeCount,
  aggregateBytesPerSecond,
  onSearchChange,
  onSectionChange,
  onOpenSettings,
  onOpenQueues,
  onOpenLinkGrabber,
  onOpenCategories,
  onAddDownload,
}: AppShellProps) {
  return (
    <div className="app-shell">
      <Sidebar
        activeItem={activeItem}
        settingsActive={settingsActive}
        queuesActive={queuesActive}
        linkGrabberActive={linkGrabberActive}
        categoriesActive={categoriesActive}
        counts={counts}
        onSelect={onSectionChange}
        onOpenSettings={
          onOpenSettings
        }
        onOpenQueues={onOpenQueues}
        onOpenLinkGrabber={onOpenLinkGrabber}
        onOpenCategories={onOpenCategories}
      />

      <div className="app-shell__workspace">
        <TopBar
          title={title}
          subtitle={subtitle}
          searchValue={searchValue}
          activeCount={activeCount}
          aggregateBytesPerSecond={
            aggregateBytesPerSecond
          }
          onSearchChange={
            onSearchChange
          }
          onAddDownload={
            onAddDownload
          }
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
