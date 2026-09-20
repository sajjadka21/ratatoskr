import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import { Channel, invoke } from "@tauri-apps/api/core";

import { AddDownloadModal } from "./components/downloads/AddDownloadModal";
import { DownloadContextMenu } from "./components/downloads/DownloadContextMenu";
import { DownloadDetailsPanel } from "./components/downloads/DownloadDetailsPanel";
import { DownloadRow } from "./components/downloads/DownloadRow";
import { AppShell } from "./components/layout/AppShell";

import type {
  DownloadSection,
  SidebarCounts,
} from "./components/layout/Sidebar";

import type { DownloadListItem } from "./types/download";

import "./App.css";

type HealthState = "ready" | "error";

type ComponentHealth = {
  status: HealthState;
  message: string | null;
};

type HealthCheckResponse = {
  core: ComponentHealth;
  storage: ComponentHealth;
  database: ComponentHealth;
};

type AppInfoResponse = {
  name: string;
  version: string;
};

type DownloadProgressEvent = {
  downloadId: string;
  downloadedBytes: number;
  totalBytes: number | null;
};

type StartDownloadResponse = {
  id: string;
  filename: string | null;
  destinationPath: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  status: string;
};

const sectionTitles: Record<
  DownloadSection,
  string
> = {
  all: "All Downloads",
  active: "Active",
  queued: "Queued",
  completed: "Completed",
  failed: "Failed",
};

function isActiveStatus(status: string): boolean {
  return [
    "created",
    "probing",
    "downloading",
    "paused",
    "retrying",
    "finalizing",
  ].includes(status);
}

function App() {
  const [health, setHealth] =
    useState<HealthCheckResponse | null>(null);

  const [appInfo, setAppInfo] =
    useState<AppInfoResponse | null>(null);

  const [downloads, setDownloads] =
    useState<DownloadListItem[]>([]);

  const [activeSection, setActiveSection] =
    useState<DownloadSection>("all");

  const [searchQuery, setSearchQuery] =
    useState("");

  const [selectedDownloadId, setSelectedDownloadId] =
    useState<string | null>(null);

  const [contextMenu, setContextMenu] =
    useState<{
      item: DownloadListItem;
      x: number;
      y: number;
    } | null>(null);

  const [modalOpen, setModalOpen] =
    useState(false);

  const [url, setUrl] =
    useState("");

  const [progress, setProgress] =
    useState<DownloadProgressEvent | null>(null);

  const [error, setError] =
    useState<string | null>(null);

  const [downloading, setDownloading] =
    useState(false);

  const refreshHealth = useCallback(async () => {
    const [healthResult, appInfoResult] =
      await Promise.all([
        invoke<HealthCheckResponse>("health_check"),
        invoke<AppInfoResponse>("get_app_info"),
      ]);

    setHealth(healthResult);
    setAppInfo(appInfoResult);
  }, []);

  const refreshDownloads = useCallback(async () => {
    const items =
      await invoke<DownloadListItem[]>(
        "list_downloads",
      );

    setDownloads(items);
  }, []);

  useEffect(() => {
    void Promise.all([
      refreshHealth(),
      refreshDownloads(),
    ]).catch((reason) => {
      setError(String(reason));
    });
  }, [refreshHealth, refreshDownloads]);

  useEffect(() => {
    function handleEscape(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      if (modalOpen || contextMenu) {
        return;
      }

      if (selectedDownloadId) {
        setSelectedDownloadId(null);
      }
    }

    window.addEventListener(
      "keydown",
      handleEscape,
    );

    return () => {
      window.removeEventListener(
        "keydown",
        handleEscape,
      );
    };
  }, [
    modalOpen,
    contextMenu,
    selectedDownloadId,
  ]);
  const allReady =
    health?.core.status === "ready" &&
    health?.storage.status === "ready" &&
    health?.database.status === "ready";

  const counts = useMemo<SidebarCounts>(() => {
    const result: SidebarCounts = {
      all: downloads.length,
      active: 0,
      queued: 0,
      completed: 0,
      failed: 0,
    };

    for (const item of downloads) {
      const status = item.status.toLowerCase();

      if (isActiveStatus(status)) {
        result.active += 1;
      }

      if (status === "queued") {
        result.queued += 1;
      }

      if (status === "completed") {
        result.completed += 1;
      }

      if (status === "failed") {
        result.failed += 1;
      }
    }

    return result;
  }, [downloads]);

  const filteredDownloads = useMemo(() => {
    const query = searchQuery
      .trim()
      .toLowerCase();

    return downloads.filter((item) => {
      const status = item.status.toLowerCase();

      const matchesSection =
        activeSection === "all" ||
        (activeSection === "active" &&
          isActiveStatus(status)) ||
        (activeSection === "queued" &&
          status === "queued") ||
        (activeSection === "completed" &&
          status === "completed") ||
        (activeSection === "failed" &&
          status === "failed");

      if (!matchesSection) {
        return false;
      }

      if (!query) {
        return true;
      }

      const searchable = [
        item.filename,
        item.sourceUrl,
        item.resolvedUrl,
        item.destinationPath,
        item.mimeType,
        item.status,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();

      return searchable.includes(query);
    });
  }, [
    downloads,
    activeSection,
    searchQuery,
  ]);

  const selectedDownload =
    filteredDownloads.find(
      (item) =>
        item.id === selectedDownloadId,
    ) ?? null;

  async function startDownload() {
    const trimmedUrl = url.trim();

    if (!trimmedUrl) {
      setError("Enter a download URL.");
      return;
    }

    if (!allReady) {
      setError(
        "The download engine is not ready yet.",
      );
      return;
    }

    setDownloading(true);
    setProgress(null);
    setError(null);

    const onProgress =
      new Channel<DownloadProgressEvent>();

    onProgress.onmessage = (message) => {
      setProgress(message);
    };

    try {
      await invoke<StartDownloadResponse>(
        "start_download",
        {
          url: trimmedUrl,
          onProgress,
        },
      );

      await refreshDownloads();

      setUrl("");
      setProgress(null);
      setModalOpen(false);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setDownloading(false);
    }
  }

  function openAddDownload() {
    setError(null);
    setProgress(null);
    setModalOpen(true);
  }

  function closeAddDownload() {
    if (downloading) {
      return;
    }

    setError(null);
    setProgress(null);
    setModalOpen(false);
  }

  function openContextMenu(
    item: DownloadListItem,
    x: number,
    y: number,
  ) {
    setContextMenu({
      item,
      x,
      y,
    });
  }
  function changeSection(
    section: DownloadSection,
  ) {
    setActiveSection(section);
    setSelectedDownloadId(null);
  }

  const backendLabel = allReady
    ? "Engine ready"
    : health
      ? "Engine unavailable"
      : "Checking engine...";

  const emptyTitle =
    searchQuery.trim()
      ? "No matching downloads"
      : activeSection === "all"
        ? "No downloads yet"
        : `No ${sectionTitles[
            activeSection
          ].toLowerCase()} downloads`;

  return (
    <>
      <AppShell
        title={sectionTitles[activeSection]}
        subtitle={`Version ${appInfo?.version ?? "..."} · ${backendLabel}`}
        activeItem={activeSection}
        counts={counts}
        searchValue={searchQuery}
        onSearchChange={setSearchQuery}
        onSectionChange={changeSection}
        onAddDownload={openAddDownload}
      >
        <div className="downloads-workspace">
          <section className="download-library">
            <div className="download-library__header">
              <div>
                <h2>Downloads</h2>

                <span>
                  {filteredDownloads.length === 1
                    ? "1 item"
                    : `${filteredDownloads.length} items`}
                </span>
              </div>
            </div>

            {filteredDownloads.length > 0 ? (
              <div className="download-library__list">
                {filteredDownloads.map((item) => (
                  <DownloadRow
                    key={item.id}
                    item={item}
                    selected={
                      selectedDownloadId ===
                      item.id
                    }
                    onSelect={() =>
                      setSelectedDownloadId(
                        item.id,
                      )
                    }
                    onContextMenu={
                      openContextMenu
                    }
                  />
                ))}
              </div>
            ) : (
              <div className="download-library__empty">
                <div>
                  <strong>{emptyTitle}</strong>

                  <span>
                    {searchQuery.trim()
                      ? "Try a different filename, URL or domain."
                      : "Nothing to show in this section."}
                  </span>

                  {downloads.length === 0 ? (
                    <button
                      type="button"
                      onClick={openAddDownload}
                    >
                      Add Download
                    </button>
                  ) : null}
                </div>
              </div>
            )}
          </section>

          <DownloadDetailsPanel
            item={selectedDownload}
            onClose={() =>
              setSelectedDownloadId(null)
            }
          />
        </div>
      </AppShell>

      <DownloadContextMenu
        item={contextMenu?.item ?? null}
        x={contextMenu?.x ?? 0}
        y={contextMenu?.y ?? 0}
        onClose={() =>
          setContextMenu(null)
        }
        onShowDetails={(id) =>
          setSelectedDownloadId(id)
        }
      />

      <AddDownloadModal
        open={modalOpen}
        url={url}
        downloading={downloading}
        engineReady={allReady}
        error={error}
        downloadedBytes={
          progress?.downloadedBytes ?? 0
        }
        totalBytes={
          progress?.totalBytes ?? null
        }
        onUrlChange={setUrl}
        onClose={closeAddDownload}
        onDownload={() => void startDownload()}
      />
    </>
  );
}

export default App;







