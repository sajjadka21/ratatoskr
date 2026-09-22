import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readText } from "@tauri-apps/plugin-clipboard-manager";

import {
  AddDownloadModal,
  type AddDownloadAction,
} from "./components/downloads/AddDownloadModal";
import { DownloadContextMenu } from "./components/downloads/DownloadContextMenu";
import { RemoveHistoryDialog } from "./components/downloads/RemoveHistoryDialog";
import { DownloadDetailsPanel } from "./components/downloads/DownloadDetailsPanel";
import { DownloadRow } from "./components/downloads/DownloadRow";
import { AppShell } from "./components/layout/AppShell";
import { QueuePage } from "./components/queues/QueuePage";
import { useQueues } from "./hooks/useQueues";

import type {
  DownloadSection,
  SidebarCounts,
} from "./components/layout/Sidebar";

import type {
  DownloadListItem,
  TransferMetricsMap,
} from "./types/download";

import {
  SettingsPage,
  type AddDownloadInputMode,
} from "./components/settings/SettingsPage";
import { extractHttpUrls } from "./utils/downloadLinks";

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

type DownloadTaskEvent = {
  kind: "progress" | "updated";
  downloadId: string;
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
  status: string;
  download: DownloadListItem | null;
};

const DOWNLOAD_TASK_EVENT = "download-task-event";

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
  const [settingsOpen, setSettingsOpen] =
    useState(false);
  const [queuesOpen, setQueuesOpen] = useState(false);

  const [
    addDownloadInputMode,
    setAddDownloadInputMode,
  ] = useState<AddDownloadInputMode>(
    "clipboard",
  );

  const [
    settingsSaving,
    setSettingsSaving,
  ] = useState(false);

  const [
    settingsError,
    setSettingsError,
  ] = useState<string | null>(null);

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

  const [removeCandidate, setRemoveCandidate] =
    useState<DownloadListItem | null>(null);

  const [removingHistory, setRemovingHistory] =
    useState(false);

  const [removeHistoryError, setRemoveHistoryError] =
    useState<string | null>(null);
  const [modalOpen, setModalOpen] =
    useState(false);

  const [url, setUrl] =
    useState("");

  const [error, setError] =
    useState<string | null>(null);

  const [creatingTasks, setCreatingTasks] =
    useState(false);

  const [liveMetrics, setLiveMetrics] =
    useState<TransferMetricsMap>({});

  const startingTaskIds = useRef(new Set<string>());

  const refreshHealth = useCallback(async () => {
    const [healthResult, appInfoResult] =
      await Promise.all([
        invoke<HealthCheckResponse>("health_check"),
        invoke<AppInfoResponse>("get_app_info"),
      ]);

    setHealth(healthResult);
    setAppInfo(appInfoResult);
  }, []);

  const refreshInputMode =
    useCallback(async () => {
      const mode =
        await invoke<AddDownloadInputMode>(
          "get_add_download_input_mode",
        );

      setAddDownloadInputMode(
        mode,
      );
    }, []);
  const refreshDownloads = useCallback(async () => {
    const items =
      await invoke<DownloadListItem[]>(
        "list_downloads",
      );

    setDownloads(items);
  }, []);

  const upsertDownloads = useCallback((records: DownloadListItem[]) => {
    setDownloads((current) => {
      const next = new Map(current.map((item) => [item.id, item]));
      for (const record of records) next.set(record.id, record);
      return [...next.values()].sort(
        (left, right) =>
          right.createdAt - left.createdAt || left.id.localeCompare(right.id),
      );
    });
  }, []);

  const updateDownloadProgress = useCallback(
    (
      downloadId: string,
      downloadedBytes: number,
      totalBytes: number | null,
      bytesPerSecond: number | null = null,
      etaSeconds: number | null = null,
    ) => {
      setDownloads((current) =>
        current.map((item) =>
          item.id === downloadId
            ? {
                ...item,
                downloadedBytes,
                totalBytes: totalBytes ?? item.totalBytes,
                status: "downloading",
              }
            : item,
        ),
      );

      setLiveMetrics((current) => ({
        ...current,
        [downloadId]: { bytesPerSecond, etaSeconds },
      }));
    },
    [],
  );

  /// A task that reached a terminal state is no longer moving, so its live
  /// rate must disappear rather than freeze at the last measured value.
  const clearLiveMetrics = useCallback((downloadId: string) => {
    setLiveMetrics((current) => {
      if (!(downloadId in current)) {
        return current;
      }

      const next = { ...current };
      delete next[downloadId];
      return next;
    });
  }, []);

  const {
    queues,
    refreshQueues,
    createQueue,
    startQueue,
    stopQueue,
    setQueueEnabled,
    reorderQueue,
    moveQueuedDownload,
    removeFromQueue,
    changePriority,
  } = useQueues({
    upsertDownloads,
    updateDownloadProgress,
    clearLiveMetrics,
    refreshDownloads,
  });

  useEffect(() => {
    void Promise.all([
      refreshHealth(),
      refreshDownloads(),
      refreshInputMode(),
      refreshQueues(),
    ]).catch((reason) => {
      setError(String(reason));
    });
  }, [refreshHealth, refreshDownloads, refreshInputMode, refreshQueues]);

  // Engine events are published to the window rather than to the invoke that
  // started the work, so a transfer that this session did not start - a queue
  // resumed when the app launched - still updates the list.
  useEffect(() => {
    const subscription = listen<DownloadTaskEvent>(
      DOWNLOAD_TASK_EVENT,
      ({ payload }) => {
        if (payload.kind === "updated" && payload.download) {
          clearLiveMetrics(payload.downloadId);
          upsertDownloads([payload.download]);
          return;
        }

        updateDownloadProgress(
          payload.downloadId,
          payload.downloadedBytes,
          payload.totalBytes,
          payload.bytesPerSecond,
          payload.etaSeconds,
        );
      },
    );

    return () => {
      void subscription.then((unlisten) => unlisten());
    };
  }, [upsertDownloads, updateDownloadProgress, clearLiveMetrics]);

  useEffect(() => {
    function handleEscape(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      if (modalOpen || contextMenu || removeCandidate) {
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
    removeCandidate,
    selectedDownloadId,
  ]);
  const parsedDownloadUrls = useMemo(
    () => extractHttpUrls(url),
    [url],
  );
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

  const queueNames = useMemo(
    () => new Map(queues.map((queue) => [queue.id, queue.name])),
    [queues],
  );

  /// Aggregate throughput is the sum of what the engine is currently
  /// measuring per transfer - never an average or an extrapolation.
  const throughput = useMemo(() => {
    let bytesPerSecond = 0;
    let transfers = 0;

    for (const item of downloads) {
      if (item.status.toLowerCase() !== "downloading") {
        continue;
      }

      transfers += 1;
      bytesPerSecond += liveMetrics[item.id]?.bytesPerSecond ?? 0;
    }

    return {
      transfers,
      bytesPerSecond: transfers > 0 ? bytesPerSecond : null,
    };
  }, [downloads, liveMetrics]);

  const selectedDownload =
    filteredDownloads.find(
      (item) =>
        item.id === selectedDownloadId,
    ) ?? null;

  async function startPersistedTask(id: string) {
    if (startingTaskIds.current.has(id)) {
      return;
    }

    startingTaskIds.current.add(id);

    try {
      const claimed = await invoke<DownloadListItem>(
        "start_download",
        {
          id,
        },
      );

      setDownloads((current) => {
        const existing = current.find(
          (item) => item.id === claimed.id,
        );

        if (!existing) {
          return [claimed, ...current];
        }

        if (existing.status.toLowerCase() !== "created") {
          return current;
        }

        return current.map((item) =>
          item.id === claimed.id ? claimed : item,
        );
      });
    } catch (reason) {
      console.error(
        `Could not start task ${id}:`,
        reason,
      );

      await refreshDownloads();
    } finally {
      startingTaskIds.current.delete(id);
    }
  }

  async function createDownloadTasks(
    action: AddDownloadAction,
  ) {
    const links =
      extractHttpUrls(url);

    if (links.length === 0) {
      setError(
        "Enter at least one valid HTTP or HTTPS download link.",
      );
      return;
    }

    if (!allReady) {
      setError(
        "The download engine is not ready yet.",
      );
      return;
    }

    setCreatingTasks(true);
    setError(null);

    const failedLinks: string[] = [];
    const createdTasks: DownloadListItem[] = [];

    try {
      let targetQueueId: string | null =
        action.kind === "queue" ? action.queueId : null;

      if (action.kind === "create-queue") {
        const queue = await createQueue({
          name: action.queueName,
          maxConcurrent: 3,
          maxConcurrentPerHost: 2,
          defaultPriority: "normal",
        });
        targetQueueId = queue.id;
      }

      for (const downloadUrl of links) {
        let task: DownloadListItem;
        try {
          task = await invoke<DownloadListItem>("create_download_task", {
            url: downloadUrl,
          });
          upsertDownloads([task]);
        } catch (reason) {
          console.error("Task creation failed:", reason);
          failedLinks.push(downloadUrl);
          continue;
        }

        if (targetQueueId) {
          try {
            task = await invoke<DownloadListItem>("enqueue_download_task", {
              id: task.id,
              queueId: targetQueueId,
              priority: null,
            });
          } catch (reason) {
            console.error("Queue assignment failed:", reason);
            setError(
              "A task was created but could not be assigned to the selected queue.",
            );
          }
        }

        createdTasks.push(task);
        upsertDownloads([task]);
      }

      if (createdTasks.length > 0) {
        setUrl("");
        setModalOpen(false);
      }

      if (failedLinks.length > 0) {
        setUrl(
          failedLinks.join("\n"),
        );

        setError(
          `${failedLinks.length} of ${links.length} tasks could not be created.`,
        );
      }

      if (action.kind === "start-now") {
        for (const task of createdTasks) {
          void startPersistedTask(task.id);
        }
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setCreatingTasks(false);
    }
  }

  async function openAddDownload() {
    setError(null);

    if (
      addDownloadInputMode ===
      "clipboard"
    ) {
      try {
        const clipboardText =
          await readText();

        const clipboardLinks =
          extractHttpUrls(
            clipboardText ?? "",
          );

        setUrl(
          clipboardLinks.join("\n"),
        );
      } catch (reason) {
        console.warn(
          "Could not read clipboard:",
          reason,
        );

        setUrl("");
      }
    } else {
      setUrl("");
    }

    setModalOpen(true);
  }

  function closeAddDownload() {
    if (creatingTasks) {
      return;
    }

    setError(null);
    setModalOpen(false);
  }

  /// Queue changes are applied by Rust; the UI only reports what failed.
  async function runQueueAction(
    action: () => Promise<unknown>,
  ) {
    try {
      await action();
    } catch (reason) {
      console.error("Queue action failed:", reason);
      setError(String(reason));
      await refreshDownloads();
    }
  }

  async function assignToQueue(
    item: DownloadListItem,
    queueId: string,
  ) {
    await runQueueAction(() =>
      item.queueId
        ? moveQueuedDownload(item.id, queueId)
        : invoke<DownloadListItem>(
            "enqueue_download_task",
            {
              id: item.id,
              queueId,
              priority: null,
            },
          ).then((record) =>
            upsertDownloads([record]),
          ),
    );
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
  function requestRemoveFromHistory(
    item: DownloadListItem,
  ) {
    setContextMenu(null);
    setRemoveHistoryError(null);
    setRemoveCandidate(item);
  }

  function closeRemoveHistory() {
    if (removingHistory) {
      return;
    }

    setRemoveHistoryError(null);
    setRemoveCandidate(null);
  }

  async function confirmRemoveFromHistory(deleteFile: boolean) {
    if (!removeCandidate) {
      return;
    }

    setRemovingHistory(true);
    setRemoveHistoryError(null);

    try {
      await invoke<void>(
        "remove_download",
        {
          id: removeCandidate.id,
          deleteFile,
        },
      );

      if (
        selectedDownloadId ===
        removeCandidate.id
      ) {
        setSelectedDownloadId(null);
      }

      await refreshDownloads();

      setRemoveCandidate(null);
    } catch (reason) {
      setRemoveHistoryError(
        String(reason),
      );
    } finally {
      setRemovingHistory(false);
    }
  }
  async function changeAddDownloadInputMode(
    mode: AddDownloadInputMode,
  ) {
    if (
      mode === addDownloadInputMode ||
      settingsSaving
    ) {
      return;
    }

    setSettingsSaving(true);
    setSettingsError(null);

    try {
      await invoke<void>(
        "set_add_download_input_mode",
        {
          mode,
        },
      );

      setAddDownloadInputMode(
        mode,
      );
    } catch (reason) {
      setSettingsError(
        String(reason),
      );
    } finally {
      setSettingsSaving(false);
    }
  }

  function openSettings() {
    setSettingsError(null);
    setSettingsOpen(true);
    setQueuesOpen(false);
    setSelectedDownloadId(null);
    setContextMenu(null);
  }
  function changeSection(
    section: DownloadSection,
  ) {
    setSettingsOpen(false);
    setQueuesOpen(false);
    setActiveSection(section);
    setSelectedDownloadId(null);
  }

  function openQueues() {
    setQueuesOpen(true);
    setSettingsOpen(false);
    setSelectedDownloadId(null);
    setContextMenu(null);
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
        title={
          settingsOpen
            ? "Settings"
            : queuesOpen
              ? "Queues"
              : sectionTitles[activeSection]
        }
        subtitle={`Version ${appInfo?.version ?? "..."} · ${backendLabel}`}
        activeItem={activeSection}
        settingsActive={settingsOpen}
        queuesActive={queuesOpen}
        counts={counts}
        onOpenSettings={openSettings}
        onOpenQueues={openQueues}
        searchValue={searchQuery}
        activeCount={throughput.transfers}
        aggregateBytesPerSecond={
          throughput.bytesPerSecond
        }
        onSearchChange={setSearchQuery}
        onSectionChange={changeSection}
        onAddDownload={openAddDownload}
      >
        {settingsOpen ? (
          <SettingsPage
            inputMode={
              addDownloadInputMode
            }
            saving={
              settingsSaving
            }
            error={
              settingsError
            }
            onInputModeChange={(mode) =>
              void changeAddDownloadInputMode(
                mode,
              )
            }
          />
        ) : queuesOpen ? (
          <QueuePage
            queues={queues}
            downloads={downloads}
            onCreateQueue={createQueue}
            onStartQueue={startQueue}
            onStopQueue={stopQueue}
            onSetQueueEnabled={setQueueEnabled}
            onReorder={reorderQueue}
            onMove={moveQueuedDownload}
            onRemove={removeFromQueue}
            onPriority={changePriority}
          />
        ) : (
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
                    metrics={liveMetrics[item.id]}
                    queueName={
                      item.queueId
                        ? queueNames.get(item.queueId)
                        : undefined
                    }
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
            metrics={
              selectedDownload
                ? liveMetrics[selectedDownload.id]
                : undefined
            }
            queueName={
              selectedDownload?.queueId
                ? queueNames.get(
                    selectedDownload.queueId,
                  )
                : undefined
            }
            onStart={(id) =>
              void startPersistedTask(id)
            }
            onClose={() =>
              setSelectedDownloadId(null)
            }
          />
          </div>
        )}
      </AppShell>

      <DownloadContextMenu
        item={contextMenu?.item ?? null}
        queues={queues}
        x={contextMenu?.x ?? 0}
        y={contextMenu?.y ?? 0}
        onClose={() =>
          setContextMenu(null)
        }
        onShowDetails={(id) =>
          setSelectedDownloadId(id)
        }
        onStart={(id) =>
          void startPersistedTask(id)
        }
        onAssignQueue={(item, queueId) =>
          void assignToQueue(item, queueId)
        }
        onRemoveFromQueue={(item) =>
          void runQueueAction(() =>
            removeFromQueue(item.id),
          )
        }
        onChangePriority={(item, priority) =>
          void runQueueAction(() =>
            changePriority(item.id, priority),
          )
        }
        onRemoveFromHistory={
          requestRemoveFromHistory
        }
      />

      <RemoveHistoryDialog
        item={removeCandidate}
        removing={removingHistory}
        error={removeHistoryError}
        onCancel={closeRemoveHistory}
        onConfirm={(deleteFile) =>
          void confirmRemoveFromHistory(deleteFile)
        }
      />

      <AddDownloadModal
        open={modalOpen}
        url={url}
        submitting={creatingTasks}
        engineReady={allReady}
        error={error}
        linkCount={
          parsedDownloadUrls.length
        }
        queues={queues}
        onUrlChange={setUrl}
        onClose={closeAddDownload}
        onSubmit={(action) =>
          void createDownloadTasks(action)
        }
      />
    </>
  );
}

export default App;









































