import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent, SyntheticEvent } from "react";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readText } from "@tauri-apps/plugin-clipboard-manager";

import { AddDownloadModal, type AddDownloadAction } from "./components/downloads/AddDownloadModal";
import { BulkActionBar, type BulkAction } from "./components/downloads/BulkActionBar";
import { DownloadContextMenu } from "./components/downloads/DownloadContextMenu";
import { DownloadDetailsPanel, type ActivityEntry } from "./components/downloads/DownloadDetailsPanel";
import { DownloadTable, type SortKey, type SortState } from "./components/downloads/DownloadTable";
import { RefreshLinkDialog } from "./components/downloads/RefreshLinkDialog";
import { RemoveHistoryDialog } from "./components/downloads/RemoveHistoryDialog";
import { ThroughputBand } from "./components/downloads/ThroughputBand";
import { CategoriesPage } from "./components/categories/CategoriesPage";
import { StatsPage } from "./components/stats/StatsPage";
import { CompletionBanner } from "./components/feedback/CompletionBanner";
import { Toasts, type Toast, type ToastKind } from "./components/feedback/Toasts";
import { AppShell } from "./components/layout/AppShell";
import type { DownloadSection, SidebarCounts, WorkspacePage } from "./components/layout/Sidebar";
import { LinkGrabberPage } from "./components/linkgrabber/LinkGrabberPage";
import { QueuePage } from "./components/queues/QueuePage";
import { SettingsPage, type AddDownloadInputMode } from "./components/settings/SettingsPage";
import { useQueues } from "./hooks/useQueues";
import { useThroughputHistory } from "./hooks/useThroughputHistory";
import { useI18n } from "./i18n/I18n";
import { engineReasonText, noticeText } from "./utils/notices";
import { CommandPalette } from "./components/common/CommandPalette";
import type { PaletteCommand } from "./utils/commandSearch";
import type { MessageKey } from "./i18n/messages";
import type {
  CompletionActionEvent,
  DownloadListItem,
  DownloadPriority,
  DownloadSettings,
  RestoreOutcome,
  TrafficSummary,
  TaskAction,
  TransferMetricsMap,
  UiPreferences,
} from "./types/download";
import { extractHttpUrls } from "./utils/downloadLinks";
import { displayName } from "./utils/fileKind";

import "./App.css";

type ComponentHealth = { status: "ready" | "error"; message: string | null };
type HealthCheckResponse = { core: ComponentHealth; storage: ComponentHealth; database: ComponentHealth };

type DownloadTaskEvent = {
  kind: "progress" | "updated" | "removed";
  downloadId: string;
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
  activeConnections: number | null;
  maxConnections: number | null;
  adaptiveReason: string | null;
  status: string;
  download: DownloadListItem | null;
};

const DOWNLOAD_TASK_EVENT = "download-task-event";
const LINK_INTAKE_EVENT = "link-intake";
const COMPLETION_ACTION_EVENT = "completion-action";

/// Statuses a late progress event may move to "downloading". A progress event
/// that arrives after a pause, cancel or completion must not revive the row.
const PROGRESS_ACCEPTING_STATUSES = new Set(["created", "probing", "queued", "retrying", "downloading"]);
const ACTIVE_STATUSES = new Set(["created", "probing", "downloading", "paused", "retrying", "finalizing"]);
const RESUMABLE_STATUSES = new Set(["paused", "retrying"]);
const ACTIVITY_LIMIT = 30;

const TOAST_DURATION_MS: Record<ToastKind, number> = { success: 4_000, info: 5_000, error: 9_000 };

const SECTION_TITLES: Record<DownloadSection, MessageKey> = {
  all: "nav.all",
  active: "nav.active",
  queued: "nav.queued",
  completed: "nav.completed",
  failed: "nav.failed",
};

const PAGE_TITLES: Record<Exclude<WorkspacePage, "downloads">, MessageKey> = {
  linkgrabber: "nav.linkGrabber",
  categories: "nav.categories",
  queues: "nav.queues",
  stats: "nav.stats",
  settings: "nav.settings",
};

function matchesSection(section: DownloadSection, status: string): boolean {
  switch (section) {
    case "all":
      return true;
    case "active":
      return ACTIVE_STATUSES.has(status);
    case "queued":
      return status === "queued";
    case "completed":
      return status === "completed";
    case "failed":
      return status === "failed" || status === "cancelled";
  }
}

function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return Boolean(
    element && (["INPUT", "TEXTAREA", "SELECT"].includes(element.tagName) || element.isContentEditable),
  );
}

type AppProps = {
  preferences: UiPreferences;
  onPreferencesChange: (preferences: UiPreferences) => void;
};

function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

function App({ preferences, onPreferencesChange }: AppProps) {
  const { t, fmt, language } = useI18n();

  const [health, setHealth] = useState<HealthCheckResponse | null>(null);
  const [downloads, setDownloads] = useState<DownloadListItem[]>([]);
  const [page, setPage] = useState<WorkspacePage>("downloads");
  const [section, setSection] = useState<DownloadSection>("all");
  const [sort, setSort] = useState<SortState>({ key: "added", descending: true });
  const [searchQuery, setSearchQuery] = useState("");

  const [inputMode, setInputMode] = useState<AddDownloadInputMode>("clipboard");
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [downloadSettings, setDownloadSettings] = useState<DownloadSettings | null>(null);

  const [focusedId, setFocusedId] = useState<string | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const lastSelectedIndex = useRef<number | null>(null);

  const [contextMenu, setContextMenu] = useState<{ item: DownloadListItem; x: number; y: number } | null>(null);
  const [removeCandidate, setRemoveCandidate] = useState<DownloadListItem | null>(null);
  const [removingHistory, setRemovingHistory] = useState(false);
  const [removeHistoryError, setRemoveHistoryError] = useState<string | null>(null);
  const [refreshCandidate, setRefreshCandidate] = useState<DownloadListItem | null>(null);

  const [modalOpen, setModalOpen] = useState(false);
  const [url, setUrl] = useState("");
  const [addError, setAddError] = useState<string | null>(null);
  const [creatingTasks, setCreatingTasks] = useState(false);

  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextToastId = useRef(1);
  const [completionAction, setCompletionAction] = useState<CompletionActionEvent | null>(null);
  const [linkIntake, setLinkIntake] = useState<{ id: number; urls: string[] } | null>(null);

  const [liveMetrics, setLiveMetrics] = useState<TransferMetricsMap>({});
  const [nowSeconds, setNowSeconds] = useState(() => Math.floor(Date.now() / 1000));
  const startingTaskIds = useRef(new Set<string>());

  // Per-download activity seen while the window is open: status changes and
  // the engine's own explanations of its connection decisions.
  const [activity, setActivity] = useState<Record<string, ActivityEntry[]>>({});
  const knownStatus = useRef(new Map<string, string>());
  const knownReason = useRef(new Map<string, string>());
  const downloadsRef = useRef<DownloadListItem[]>([]);

  const recordActivity = useCallback((id: string, entry: ActivityEntry) => {
    setActivity((current) => {
      const list = current[id] ?? [];
      return { ...current, [id]: [...list, entry].slice(-ACTIVITY_LIMIT) };
    });
  }, []);

  const noteStatus = useCallback(
    (id: string, status: string) => {
      const previous = knownStatus.current.get(id);
      knownStatus.current.set(id, status);
      if (previous !== undefined && previous !== status) {
        recordActivity(id, { at: Date.now(), kind: "status", status });
      }
    },
    [recordActivity],
  );

  // ---- feedback -----------------------------------------------------------

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  /// Feedback that does not belong to an open dialog. Identical messages
  /// collapse so a failing bulk action cannot flood the screen.
  const notify = useCallback(
    (kind: ToastKind, message: string) => {
      const id = nextToastId.current++;
      setToasts((current) => [...current.filter((toast) => toast.message !== message).slice(-3), { id, kind, message }]);
      window.setTimeout(() => dismissToast(id), TOAST_DURATION_MS[kind]);
    },
    [dismissToast],
  );
  const reportError = useCallback((message: string) => notify("error", message), [notify]);
  const reportSaved = useCallback((message: string) => notify("success", message), [notify]);

  // A restore chosen before the last restart reports how it went, once.
  useEffect(() => {
    invoke<RestoreOutcome | null>("take_restore_outcome")
      .then((outcome) => {
        if (!outcome) return;
        if (outcome.restored) notify("success", t("backup.restored"));
        else notify("error", t("backup.refused", { reason: outcome.reason ?? "" }));
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---- engine state -------------------------------------------------------

  const refreshDownloads = useCallback(async () => {
    const items = await invoke<DownloadListItem[]>("list_downloads");
    for (const item of items) knownStatus.current.set(item.id, item.status.toLowerCase());
    setDownloads(items);
  }, []);

  const upsertDownloads = useCallback(
    (records: DownloadListItem[]) => {
      for (const record of records) noteStatus(record.id, record.status.toLowerCase());
      setDownloads((current) => {
        const next = new Map(current.map((item) => [item.id, item]));
        for (const record of records) next.set(record.id, record);
        return [...next.values()].sort(
          (left, right) => right.createdAt - left.createdAt || left.id.localeCompare(right.id),
        );
      });
    },
    [noteStatus],
  );

  const updateDownloadProgress = useCallback(
    (
      downloadId: string,
      downloadedBytes: number,
      totalBytes: number | null,
      bytesPerSecond: number | null = null,
      etaSeconds: number | null = null,
      activeConnections: number | null = null,
      maxConnections: number | null = null,
      adaptiveReason: string | null = null,
    ) => {
      const known = downloadsRef.current.find((item) => item.id === downloadId);
      if (known && PROGRESS_ACCEPTING_STATUSES.has(known.status.toLowerCase())) {
        noteStatus(downloadId, "downloading");
        if (adaptiveReason && knownReason.current.get(downloadId) !== adaptiveReason) {
          knownReason.current.set(downloadId, adaptiveReason);
          recordActivity(downloadId, {
            at: Date.now(),
            kind: "engine",
            text: engineReasonText(adaptiveReason, latestT.current) ?? adaptiveReason,
          });
        }
      }

      setDownloads((current) => {
        const index = current.findIndex((item) => item.id === downloadId);
        if (index < 0) return current;
        const item = current[index];
        if (!PROGRESS_ACCEPTING_STATUSES.has(item.status.toLowerCase())) return current;
        // Replace only the row that moved so every other row keeps its
        // identity and memoised rows skip rendering.
        const next = current.slice();
        next[index] = { ...item, downloadedBytes, totalBytes: totalBytes ?? item.totalBytes, status: "downloading" };
        return next;
      });

      setLiveMetrics((current) => ({
        ...current,
        [downloadId]: { bytesPerSecond, etaSeconds, activeConnections, maxConnections, adaptiveReason },
      }));
    },
    [noteStatus, recordActivity],
  );

  /// A task that stopped moving loses its live rate instead of freezing it.
  const clearLiveMetrics = useCallback((downloadId: string) => {
    setLiveMetrics((current) => {
      if (!(downloadId in current)) return current;
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
  } = useQueues({ upsertDownloads, updateDownloadProgress, clearLiveMetrics, refreshDownloads });

  // The usage chip follows traffic without being a live counter: a refresh
  // every 20 seconds, and whenever the downloads page comes back into view.
  const [traffic, setTraffic] = useState<TrafficSummary | null>(null);
  useEffect(() => {
    if (page !== "downloads") return;
    let cancelled = false;
    const load = () =>
      invoke<TrafficSummary>("get_traffic_summary")
        .then((summary) => {
          if (!cancelled) setTraffic(summary);
        })
        .catch(() => {});
    void load();
    const timer = window.setInterval(() => void load(), 20_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [page]);

  useEffect(() => {
    void Promise.all([
      invoke<HealthCheckResponse>("health_check").then(setHealth),
      refreshDownloads(),
      invoke<AddDownloadInputMode>("get_add_download_input_mode").then(setInputMode),
      invoke<DownloadSettings>("get_download_settings").then(setDownloadSettings),
      refreshQueues(),
    ]).catch((reason) => notify("error", t("toast.couldNotLoad", { reason: String(reason) })));
    // Loading happens once; the language only affects the error text.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Engine events go to the whole window, so transfers this session did not
  // start (a queue resumed at launch, a browser handoff) still update the list.
  const latestT = useRef(t);
  latestT.current = t;
  const latestLanguage = useRef(language);
  latestLanguage.current = language;
  useEffect(() => {
    const subscription = listen<DownloadTaskEvent>(DOWNLOAD_TASK_EVENT, ({ payload }) => {
      if (payload.kind === "removed") {
        clearLiveMetrics(payload.downloadId);
        setDownloads((current) => current.filter((item) => item.id !== payload.downloadId));
        return;
      }
      if (payload.kind === "updated" && payload.download) {
        const status = payload.download.status.toLowerCase();
        const name = displayName(payload.download);
        if (status === "completed") {
          notify("success", latestT.current("toast.completed", { name }));
        } else if (status === "failed") {
          notify(
            "error",
            payload.download.errorMessage
              ? latestT.current("toast.failedWithReason", {
                  name,
                  reason:
                    noticeText(
                      payload.download.errorCode,
                      payload.download.errorMessage,
                      latestT.current,
                      latestLanguage.current,
                    ) ?? payload.download.errorMessage,
                })
              : latestT.current("toast.failed", { name }),
          );
        }
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
        payload.activeConnections,
        payload.maxConnections,
        payload.adaptiveReason,
      );
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, [upsertDownloads, updateDownloadProgress, clearLiveMetrics, notify]);

  // A finished queue can schedule sleep, shutdown or closing the app.
  useEffect(() => {
    const subscription = listen<CompletionActionEvent>(COMPLETION_ACTION_EVENT, ({ payload }) => {
      if (payload.state === "pending") {
        setCompletionAction(payload);
        return;
      }
      setCompletionAction((current) => (current && current.id === payload.id ? null : current));
      if (payload.state === "skipped" && payload.message) notify("info", payload.message);
      else if (payload.state === "cancelled") notify("info", latestT.current("toast.cancelledAction"));
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, [notify]);

  // Links from the browser's "Send selected links" open in LinkGrabber.
  useEffect(() => {
    async function collectIntake() {
      try {
        const urls = await invoke<string[]>("take_pending_link_intake");
        if (urls.length === 0) return;
        setLinkIntake((current) => ({ id: (current?.id ?? 0) + 1, urls }));
        setPage("linkgrabber");
      } catch (reason) {
        console.warn("Could not collect browser links:", reason);
      }
    }
    void collectIntake();
    const subscription = listen(LINK_INTAKE_EVENT, () => void collectIntake());
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  const hasPendingRetry = downloads.some((item) => item.status.toLowerCase() === "retrying" && item.retryAt !== null);
  useEffect(() => {
    if (!hasPendingRetry) return;
    setNowSeconds(Math.floor(Date.now() / 1000));
    const timer = window.setInterval(() => setNowSeconds(Math.floor(Date.now() / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [hasPendingRetry]);

  // ---- derived views ------------------------------------------------------

  downloadsRef.current = downloads;

  const allReady =
    health?.core.status === "ready" && health?.storage.status === "ready" && health?.database.status === "ready";
  const engineState = allReady ? "ready" : health ? "down" : "checking";

  const counts = useMemo<SidebarCounts>(() => {
    const result: SidebarCounts = { all: downloads.length, active: 0, queued: 0, completed: 0, failed: 0 };
    for (const item of downloads) {
      const status = item.status.toLowerCase();
      for (const key of ["active", "queued", "completed", "failed"] as const) {
        if (matchesSection(key, status)) result[key] += 1;
      }
    }
    return result;
  }, [downloads]);

  const bytesToday = useMemo(() => {
    const midnight = new Date();
    midnight.setHours(0, 0, 0, 0);
    const since = midnight.getTime() / 1000;
    return downloads.reduce(
      (sum, item) =>
        sum +
        (item.status.toLowerCase() === "completed" && (item.completedAt ?? 0) >= since
          ? item.totalBytes ?? item.downloadedBytes
          : 0),
      0,
    );
  }, [downloads]);

  const visibleDownloads = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    const filtered = downloads.filter((item) => {
      if (!matchesSection(section, item.status.toLowerCase())) return false;
      if (!query) return true;
      return [item.filename, item.sourceUrl, item.resolvedUrl, item.destinationPath, item.mimeType]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(query);
    });

    const progress = (item: DownloadListItem) =>
      item.totalBytes ? item.downloadedBytes / item.totalBytes : item.status.toLowerCase() === "completed" ? 1 : 0;
    const compare: Record<SortKey, (a: DownloadListItem, b: DownloadListItem) => number> = {
      added: (a, b) => a.createdAt - b.createdAt,
      name: (a, b) => displayName(a).localeCompare(displayName(b), language),
      progress: (a, b) => progress(a) - progress(b),
      speed: (a, b) => (liveMetrics[a.id]?.bytesPerSecond ?? -1) - (liveMetrics[b.id]?.bytesPerSecond ?? -1),
      size: (a, b) => (a.totalBytes ?? 0) - (b.totalBytes ?? 0),
    };
    const direction = sort.descending ? -1 : 1;
    return filtered.sort((a, b) => direction * compare[sort.key](a, b) || a.id.localeCompare(b.id));
    // Speed sorting re-orders live; everything else only when data changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [downloads, section, searchQuery, sort, language, sort.key === "speed" ? liveMetrics : null]);

  const queueNames = useMemo(() => new Map(queues.map((queue) => [queue.id, queue.name])), [queues]);
  const history = useThroughputHistory(downloads, liveMetrics);

  const throughput = useMemo(() => {
    let bytesPerSecond = 0;
    let active = 0;
    let remainingBytes = 0;
    let unknownRemaining = false;
    for (const item of downloads) {
      if (item.status.toLowerCase() !== "downloading") continue;
      active += 1;
      bytesPerSecond += liveMetrics[item.id]?.bytesPerSecond ?? 0;
      if (item.totalBytes) remainingBytes += Math.max(0, item.totalBytes - item.downloadedBytes);
      else unknownRemaining = true;
    }
    const eta = active && bytesPerSecond > 0 && !unknownRemaining ? remainingBytes / bytesPerSecond : null;
    return { bytesPerSecond, active, eta };
  }, [downloads, liveMetrics]);

  const resumableCount = downloads.filter((item) => RESUMABLE_STATUSES.has(item.status.toLowerCase())).length;
  const focusedDownload = detailsOpen ? downloads.find((item) => item.id === focusedId) ?? null : null;
  const selectedDownloads = useMemo(
    () => downloads.filter((item) => selectedIds.has(item.id)),
    [downloads, selectedIds],
  );

  // ---- tray ---------------------------------------------------------------

  const trayTooltip =
    throughput.active > 0
      ? t("tray.active", {
          speed: fmt.rate(throughput.bytesPerSecond) ?? "",
          count: fmt.number(throughput.active),
        })
      : t("tray.idle");
  const lastTrayLanguage = useRef<string | null>(null);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      const labels =
        lastTrayLanguage.current === language
          ? null
          : { show: t("tray.show"), pauseAll: t("tray.pauseAll"), quit: t("tray.quit") };
      lastTrayLanguage.current = language;
      void invoke("set_tray_status", { tooltip: trayTooltip, labels }).catch(() => {});
    }, 1500);
    return () => window.clearTimeout(timer);
  }, [trayTooltip, language, t]);

  // ---- task actions -------------------------------------------------------

  async function startPersistedTask(id: string) {
    if (startingTaskIds.current.has(id)) return;
    startingTaskIds.current.add(id);
    try {
      const claimed = await invoke<DownloadListItem>("start_download", { id });
      upsertDownloads([claimed]);
    } catch (reason) {
      notify("error", t("toast.startFailed", { reason: String(reason) }));
      await refreshDownloads();
    } finally {
      startingTaskIds.current.delete(id);
    }
  }

  /// Every control goes straight to Rust, which owns the lifecycle and
  /// refuses anything its state machine does not allow.
  async function runTaskAction(item: DownloadListItem, action: TaskAction) {
    setContextMenu(null);
    try {
      switch (action) {
        case "start":
        case "retry":
          await startPersistedTask(item.id);
          break;
        case "resume":
          upsertDownloads([await invoke<DownloadListItem>("resume_download", { id: item.id })]);
          break;
        case "restart":
          upsertDownloads([await invoke<DownloadListItem>("restart_download", { id: item.id })]);
          break;
        case "pause":
          await invoke<DownloadListItem>("pause_download", { id: item.id });
          break;
        case "cancel":
          upsertDownloads([await invoke<DownloadListItem>("cancel_download", { id: item.id })]);
          break;
      }
    } catch (reason) {
      notify(
        "error",
        t("toast.couldNot", {
          action: t(`action.${action}` as MessageKey),
          name: displayName(item),
          reason: String(reason),
        }),
      );
      await refreshDownloads();
    }
  }

  async function runBulkAction(action: BulkAction) {
    const failures: string[] = [];
    for (const item of selectedDownloads) {
      try {
        switch (action) {
          case "start":
          case "retry":
            await startPersistedTask(item.id);
            break;
          case "pause":
            await invoke("pause_download", { id: item.id });
            break;
          case "resume":
            await invoke("resume_download", { id: item.id });
            break;
          case "cancel":
            await invoke("cancel_download", { id: item.id });
            break;
          case "remove":
            await invoke("remove_download", { id: item.id, deleteFile: false });
            break;
        }
      } catch (reason) {
        failures.push(String(reason));
      }
    }
    if (failures.length) notify("error", t("toast.bulkFailed", { reason: failures[0] }));
    await refreshDownloads();
    clearSelection();
  }

  async function runBulkQueue(queueId: string) {
    for (const item of selectedDownloads) {
      try {
        if (item.queueId) await moveQueuedDownload(item.id, queueId);
        else await invoke("enqueue_download_task", { id: item.id, queueId, priority: null });
      } catch (reason) {
        notify("error", t("toast.queueFailed", { reason: String(reason) }));
      }
    }
    await refreshDownloads();
    clearSelection();
  }

  async function runBulkPriority(priority: DownloadPriority) {
    for (const item of selectedDownloads) {
      try {
        await changePriority(item.id, priority);
      } catch (reason) {
        notify("error", t("toast.queueFailed", { reason: String(reason) }));
      }
    }
    await refreshDownloads();
    clearSelection();
  }

  async function pauseAll() {
    try {
      const count = await invoke<number>("pause_all_downloads");
      if (count > 0) notify("info", t("toast.pausedAll", { count: fmt.number(count) }));
    } catch (reason) {
      notify("error", String(reason));
    }
  }

  async function resumeAll() {
    for (const item of downloads) {
      if (!RESUMABLE_STATUSES.has(item.status.toLowerCase())) continue;
      try {
        upsertDownloads([await invoke<DownloadListItem>("resume_download", { id: item.id })]);
      } catch (reason) {
        notify("error", t("toast.bulkFailed", { reason: String(reason) }));
      }
    }
  }

  async function setSpeedLimit(limit: number | null) {
    try {
      setDownloadSettings(await invoke<DownloadSettings>("set_global_speed_limit", { bytesPerSecond: limit }));
    } catch (reason) {
      notify("error", String(reason));
    }
  }

  async function submitRefreshedLink(item: DownloadListItem, sourceUrl: string) {
    try {
      upsertDownloads([await invoke<DownloadListItem>("refresh_download_source", { id: item.id, sourceUrl })]);
      setRefreshCandidate(null);
    } catch (reason) {
      await refreshDownloads();
      throw t("toast.refreshFailed", { reason: String(reason) });
    }
  }

  async function runQueueAction(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (reason) {
      notify("error", t("toast.queueFailed", { reason: String(reason) }));
      await refreshDownloads();
    }
  }

  async function assignToQueue(item: DownloadListItem, queueId: string) {
    await runQueueAction(() =>
      item.queueId
        ? moveQueuedDownload(item.id, queueId)
        : invoke<DownloadListItem>("enqueue_download_task", { id: item.id, queueId, priority: null }).then(
            (record) => upsertDownloads([record]),
          ),
    );
  }

  // ---- add download -------------------------------------------------------

  async function createDownloadTasks(action: AddDownloadAction, inputValue = url) {
    // LinkGrabber submits without the dialog, so its errors need a toast.
    const report = modalOpen ? setAddError : (message: string) => notify("error", message);
    const links = extractHttpUrls(inputValue);
    if (links.length === 0) return report(t("toast.noLinks"));
    if (!allReady) return report(t("toast.engineNotReady"));

    setCreatingTasks(true);
    setAddError(null);
    const failedLinks: string[] = [];
    const created: DownloadListItem[] = [];

    try {
      let targetQueueId: string | null = action.kind === "queue" ? action.queueId : null;
      if (action.kind === "create-queue") {
        const queue = await createQueue({
          name: action.queueName,
          maxConcurrent: 3,
          maxConcurrentPerHost: 2,
          defaultPriority: "normal",
        });
        targetQueueId = queue.id;
      }

      for (const link of links) {
        let task: DownloadListItem;
        try {
          task = await invoke<DownloadListItem>("create_download_task", { url: link });
        } catch (reason) {
          console.error("Task creation failed:", reason);
          failedLinks.push(link);
          continue;
        }
        if (targetQueueId) {
          try {
            task = await invoke<DownloadListItem>("enqueue_download_task", {
              id: task.id,
              queueId: targetQueueId,
              priority: null,
            });
          } catch {
            report(t("toast.queueAssignFailed"));
          }
        }
        created.push(task);
        upsertDownloads([task]);
      }

      if (created.length > 0) {
        setUrl("");
        setModalOpen(false);
        setPage("downloads");
      }
      if (failedLinks.length > 0) {
        setUrl(failedLinks.join("\n"));
        report(
          t("toast.someNotCreated", {
            failed: fmt.number(failedLinks.length),
            total: fmt.number(links.length),
          }),
        );
      }
      if (action.kind === "start-now") {
        for (const task of created) void startPersistedTask(task.id);
      }
    } catch (reason) {
      report(String(reason));
    } finally {
      setCreatingTasks(false);
    }
  }

  async function openAddDownload() {
    setAddError(null);
    if (inputMode === "clipboard") {
      try {
        setUrl(extractHttpUrls((await readText()) ?? "").join("\n"));
      } catch {
        setUrl("");
      }
    } else {
      setUrl("");
    }
    setModalOpen(true);
  }

  // ---- selection ----------------------------------------------------------

  function clearSelection() {
    setSelectedIds(new Set());
    lastSelectedIndex.current = null;
  }

  function selectDownload(item: DownloadListItem, index: number, event: SyntheticEvent) {
    const pointer = event as ReactMouseEvent;
    const keyboard = event as ReactKeyboardEvent;
    const additive = pointer.ctrlKey || pointer.metaKey || keyboard.ctrlKey || keyboard.metaKey;
    const range = pointer.shiftKey || keyboard.shiftKey;
    const previous = lastSelectedIndex.current;

    setFocusedId(item.id);
    setDetailsOpen(true);
    lastSelectedIndex.current = index;
    setSelectedIds((current) => {
      if (range && previous !== null) {
        const [from, to] = [Math.min(previous, index), Math.max(previous, index)];
        return new Set(visibleDownloads.slice(from, to + 1).map((download) => download.id));
      }
      if (additive) {
        const next = new Set(current);
        if (next.has(item.id)) next.delete(item.id);
        else next.add(item.id);
        return next;
      }
      return new Set([item.id]);
    });
  }

  function moveFocus(step: number, extend: boolean) {
    if (visibleDownloads.length === 0) return;
    const current = visibleDownloads.findIndex((item) => item.id === focusedId);
    const index = Math.max(0, Math.min(visibleDownloads.length - 1, current < 0 ? 0 : current + step));
    const item = visibleDownloads[index];
    setFocusedId(item.id);
    if (extend && lastSelectedIndex.current !== null) {
      const [from, to] = [Math.min(lastSelectedIndex.current, index), Math.max(lastSelectedIndex.current, index)];
      setSelectedIds(new Set(visibleDownloads.slice(from, to + 1).map((download) => download.id)));
    } else {
      lastSelectedIndex.current = index;
      setSelectedIds(new Set([item.id]));
    }
  }

  // Stable handlers for the memoised rows; they read this render's logic.
  const rowHandlers = useRef({
    select: selectDownload,
    action: (item: DownloadListItem, action: TaskAction) => void runTaskAction(item, action),
    contextMenu: (item: DownloadListItem, x: number, y: number) => setContextMenu({ item, x, y }),
  });
  rowHandlers.current = {
    select: selectDownload,
    action: (item, action) => void runTaskAction(item, action),
    contextMenu: (item, x, y) => setContextMenu({ item, x, y }),
  };
  const handleRowSelect = useCallback(
    (item: DownloadListItem, index: number, event: SyntheticEvent) => rowHandlers.current.select(item, index, event),
    [],
  );
  const handleRowAction = useCallback(
    (item: DownloadListItem, action: TaskAction) => rowHandlers.current.action(item, action),
    [],
  );
  const handleRowContextMenu = useCallback(
    (item: DownloadListItem, x: number, y: number) => rowHandlers.current.contextMenu(item, x, y),
    [],
  );

  // ---- command palette ----------------------------------------------------

  const paletteCommands: PaletteCommand[] = (() => {
    const commands: PaletteCommand[] = [
      { id: "add", group: "action", label: t("nav.addLink"), hint: "Ctrl+N", keywords: "add new link url download افزودن", run: () => void openAddDownload() },
      { id: "pause-all", group: "action", label: t("band.pauseAll"), keywords: "pause all stop توقف", run: () => void pauseAll() },
      { id: "resume-all", group: "action", label: t("band.resumeAll"), keywords: "resume all continue start ادامه", run: () => void resumeAll() },
      { id: "limit-none", group: "action", label: t("palette.limitNone"), keywords: "speed limit unlimited سرعت", run: () => void setSpeedLimit(null) },
      { id: "limit-1m", group: "action", label: t("palette.limitTo", { rate: fmt.rate(1024 * 1024) ?? "" }), keywords: "speed limit 1 mb سرعت", run: () => void setSpeedLimit(1024 * 1024) },
      { id: "limit-5m", group: "action", label: t("palette.limitTo", { rate: fmt.rate(5 * 1024 * 1024) ?? "" }), keywords: "speed limit 5 mb سرعت", run: () => void setSpeedLimit(5 * 1024 * 1024) },
      {
        id: "language",
        group: "action",
        label: preferences.language === "fa" ? "Switch to English" : "تغییر زبان به فارسی",
        keywords: "language english persian farsi زبان",
        run: () => onPreferencesChange({ ...preferences, language: preferences.language === "fa" ? "en" : "fa" }),
      },
      {
        id: "theme",
        group: "action",
        label: t("palette.theme"),
        keywords: "theme dark light mode تم تیره روشن",
        run: () =>
          onPreferencesChange({
            ...preferences,
            theme: preferences.theme === "dark" ? "light" : preferences.theme === "light" ? "system" : "dark",
          }),
      },
      { id: "go-downloads", group: "go", label: t("nav.all"), keywords: "downloads list دانلودها", run: () => goToSection("all") },
      { id: "go-active", group: "go", label: t("nav.active"), keywords: "active downloading", run: () => goToSection("active") },
      { id: "go-failed", group: "go", label: t("nav.failed"), keywords: "failed errors attention", run: () => goToSection("failed") },
      { id: "go-grabber", group: "go", label: t("nav.linkGrabber"), keywords: "link grabber collect", run: () => goToPage("linkgrabber") },
      { id: "go-queues", group: "go", label: t("nav.queues"), keywords: "queues schedule", run: () => goToPage("queues") },
      { id: "go-categories", group: "go", label: t("nav.categories"), keywords: "categories folders", run: () => goToPage("categories") },
      { id: "go-stats", group: "go", label: t("nav.stats"), keywords: "statistics stats chart traffic آمار", run: () => goToPage("stats") },
      { id: "go-settings", group: "go", label: t("nav.settings"), hint: "Ctrl+,", keywords: "settings preferences options proxy quota", run: () => goToPage("settings") },
    ];
    for (const item of downloads.slice(0, 500)) {
      commands.push({
        id: `download-${item.id}`,
        group: "download",
        label: displayName(item),
        hint: t(`status.${item.status.toLowerCase()}` as MessageKey),
        keywords: hostOf(item.sourceUrl),
        run: () => {
          goToSection("all");
          setFocusedId(item.id);
          setSelectedIds(new Set([item.id]));
          setDetailsOpen(true);
        },
      });
    }
    return commands;
  })();

  // ---- keyboard -----------------------------------------------------------

  const overlayOpen =
    modalOpen || paletteOpen || Boolean(contextMenu) || Boolean(removeCandidate) || Boolean(refreshCandidate);
  const keyboard = useRef<(event: KeyboardEvent) => void>(() => {});
  keyboard.current = (event: KeyboardEvent) => {
    const control = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();

    if (control && (key === "k" || (event.shiftKey && key === "p"))) {
      event.preventDefault();
      setPaletteOpen((open) => !open);
      return;
    }
    if (control && key === ",") {
      event.preventDefault();
      goToPage("settings");
      return;
    }
    if (control && key === "n") {
      event.preventDefault();
      void openAddDownload();
      return;
    }
    if (control && key === "f") {
      event.preventDefault();
      setPage("downloads");
      window.setTimeout(() => document.getElementById("download-search")?.focus(), 0);
      return;
    }
    if (overlayOpen || isTypingTarget(event.target) || page !== "downloads") return;

    if (control && key === "a") {
      event.preventDefault();
      setSelectedIds(new Set(visibleDownloads.map((item) => item.id)));
    } else if (event.key === "Escape") {
      if (selectedIds.size > 1) clearSelection();
      else setDetailsOpen(false);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      moveFocus(event.key === "ArrowDown" ? 1 : -1, event.shiftKey);
    } else if (
      event.key === " " &&
      selectedDownloads.length > 0 &&
      (event.target as HTMLElement | null)?.tagName !== "BUTTON"
    ) {
      event.preventDefault();
      for (const item of selectedDownloads) {
        const status = item.status.toLowerCase();
        if (status === "downloading" || status === "probing") void runTaskAction(item, "pause");
        else if (status === "paused" || status === "retrying") void runTaskAction(item, "resume");
        else if (status === "created") void runTaskAction(item, "start");
      }
    } else if (event.key === "Delete" && selectedDownloads.length > 0) {
      event.preventDefault();
      if (selectedDownloads.length === 1) setRemoveCandidate(selectedDownloads[0]);
      else void runBulkAction("remove");
    }
  };
  useEffect(() => {
    const handler = (event: KeyboardEvent) => keyboard.current(event);
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // ---- settings -----------------------------------------------------------

  async function changeInputMode(mode: AddDownloadInputMode) {
    if (mode === inputMode || settingsSaving) return;
    setSettingsSaving(true);
    setSettingsError(null);
    try {
      await invoke<void>("set_add_download_input_mode", { mode });
      setInputMode(mode);
    } catch (reason) {
      setSettingsError(String(reason));
    } finally {
      setSettingsSaving(false);
    }
  }

  async function confirmRemove(deleteFile: boolean) {
    if (!removeCandidate) return;
    setRemovingHistory(true);
    setRemoveHistoryError(null);
    try {
      await invoke<void>("remove_download", { id: removeCandidate.id, deleteFile });
      if (focusedId === removeCandidate.id) setDetailsOpen(false);
      setSelectedIds((current) => {
        const next = new Set(current);
        next.delete(removeCandidate.id);
        return next;
      });
      await refreshDownloads();
      setRemoveCandidate(null);
    } catch (reason) {
      setRemoveHistoryError(String(reason));
    } finally {
      setRemovingHistory(false);
    }
  }

  function goToSection(next: DownloadSection) {
    setPage("downloads");
    setSection(next);
    clearSelection();
  }

  function goToPage(next: WorkspacePage) {
    setPage(next);
    setContextMenu(null);
  }

  function toggleSort(key: SortKey) {
    setSort((current) =>
      current.key === key ? { key, descending: !current.descending } : { key, descending: key !== "name" },
    );
  }

  // ---- render -------------------------------------------------------------

  const title = page === "downloads" ? t(SECTION_TITLES[section]) : t(PAGE_TITLES[page]);
  const subtitle =
    page === "downloads"
      ? visibleDownloads.length === 1
        ? t("table.oneItem")
        : t("table.items", { count: fmt.number(visibleDownloads.length) })
      : undefined;

  const query = searchQuery.trim();
  const emptyState = (
    <div className="download-table__empty">
      <div>
        <strong>
          {query ? t("table.empty.search") : downloads.length === 0 ? t("table.empty.none") : t("table.empty.section")}
        </strong>
        <span>
          {query
            ? t("table.empty.searchHint")
            : downloads.length === 0
              ? t("table.empty.noneHint")
              : t("table.empty.sectionHint")}
        </span>
        {downloads.length === 0 ? (
          <button type="button" onClick={() => void openAddDownload()}>
            {t("nav.addLink")}
          </button>
        ) : null}
      </div>
    </div>
  );

  return (
    <>
      <AppShell
        title={title}
        subtitle={subtitle}
        page={page}
        section={section}
        counts={counts}
        bytesToday={bytesToday}
        engineState={engineState}
        searchValue={searchQuery}
        onSearchChange={setSearchQuery}
        onSection={goToSection}
        onPage={goToPage}
        onAddDownload={() => void openAddDownload()}
        fill={page === "downloads"}
      >
        {page === "settings" ? (
          <SettingsPage
            inputMode={inputMode}
            saving={settingsSaving}
            error={settingsError}
            onInputModeChange={(mode) => void changeInputMode(mode)}
            queues={queues}
            downloadSettings={downloadSettings}
            onDownloadSettingsChange={setDownloadSettings}
            onError={reportError}
            onSaved={reportSaved}
            uiPreferences={preferences}
            onUiPreferencesChange={onPreferencesChange}
          />
        ) : page === "queues" ? (
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
        ) : page === "linkgrabber" ? (
          <LinkGrabberPage
            queues={queues}
            engineReady={allReady}
            submitting={creatingTasks}
            onSubmit={(urls, action) => void createDownloadTasks(action, urls.join("\n"))}
            intake={linkIntake}
          />
        ) : page === "categories" ? (
          <CategoriesPage onError={reportError} />
        ) : page === "stats" ? (
          <StatsPage onError={reportError} />
        ) : (
          <div className="downloads-workspace">
            <section className="downloads-workspace__main">
              <ThroughputBand
                bytesPerSecond={throughput.bytesPerSecond}
                history={history.total}
                activeCount={throughput.active}
                queuedCount={counts.queued}
                etaSeconds={throughput.eta}
                resumableCount={resumableCount}
                speedLimit={downloadSettings?.globalSpeedLimit ?? null}
                onSetSpeedLimit={(limit) => void setSpeedLimit(limit)}
                onPauseAll={() => void pauseAll()}
                onResumeAll={() => void resumeAll()}
                traffic={traffic}
                onOpenTraffic={() => setPage("settings")}
              />

              {selectedIds.size > 1 ? (
                <div className="downloads-workspace__bulk">
                  <BulkActionBar
                    count={selectedIds.size}
                    queues={queues}
                    onAction={(action) => void runBulkAction(action)}
                    onQueue={(queueId) => void runBulkQueue(queueId)}
                    onPriority={(priority) => void runBulkPriority(priority)}
                    onClear={clearSelection}
                  />
                </div>
              ) : null}

              <div className="downloads-workspace__table">
                <DownloadTable
                  items={visibleDownloads}
                  metrics={liveMetrics}
                  history={history.byTask}
                  selectedIds={selectedIds}
                  focusedId={focusedId}
                  queueNames={queueNames}
                  nowSeconds={nowSeconds}
                  sort={sort}
                  empty={emptyState}
                  onSort={toggleSort}
                  onSelect={handleRowSelect}
                  onAction={handleRowAction}
                  onContextMenu={handleRowContextMenu}
                />
              </div>

              <footer className="downloads-workspace__status">
                <span className="num">
                  {selectedIds.size > 0
                    ? t("table.selected", { count: fmt.number(selectedIds.size) })
                    : subtitle}
                </span>
                <span className="downloads-workspace__hint">{t("shortcuts.hint")}</span>
              </footer>
            </section>

            <DownloadDetailsPanel
              item={focusedDownload}
              metrics={focusedDownload ? liveMetrics[focusedDownload.id] : undefined}
              history={focusedDownload ? history.byTask[focusedDownload.id] : undefined}
              activity={focusedDownload ? activity[focusedDownload.id] ?? [] : []}
              queueName={focusedDownload?.queueId ? queueNames.get(focusedDownload.queueId) : undefined}
              onAction={(target, action) => void runTaskAction(target, action)}
              onRefreshSource={setRefreshCandidate}
              onClose={() => setDetailsOpen(false)}
              onError={reportError}
            />
          </div>
        )}
      </AppShell>

      <DownloadContextMenu
        item={contextMenu?.item ?? null}
        queues={queues}
        x={contextMenu?.x ?? 0}
        y={contextMenu?.y ?? 0}
        onClose={() => setContextMenu(null)}
        onShowDetails={(id) => {
          setFocusedId(id);
          setDetailsOpen(true);
        }}
        onAction={(target, action) => void runTaskAction(target, action)}
        onAssignQueue={(item, queueId) => void assignToQueue(item, queueId)}
        onRemoveFromQueue={(item) => void runQueueAction(() => removeFromQueue(item.id))}
        onChangePriority={(item, priority) => void runQueueAction(() => changePriority(item.id, priority))}
        onRemoveFromHistory={(item) => {
          setContextMenu(null);
          setRemoveHistoryError(null);
          setRemoveCandidate(item);
        }}
        onRefreshSource={(item) => {
          setContextMenu(null);
          setRefreshCandidate(item);
        }}
        onError={reportError}
      />

      <RemoveHistoryDialog
        item={removeCandidate}
        removing={removingHistory}
        error={removeHistoryError}
        onCancel={() => {
          if (removingHistory) return;
          setRemoveHistoryError(null);
          setRemoveCandidate(null);
        }}
        onConfirm={(deleteFile) => void confirmRemove(deleteFile)}
      />

      <RefreshLinkDialog
        item={refreshCandidate}
        onCancel={() => setRefreshCandidate(null)}
        onSubmit={submitRefreshedLink}
      />

      <Toasts toasts={toasts} onDismiss={dismissToast} />

      {completionAction ? (
        <CompletionBanner
          event={completionAction}
          onCancel={() => {
            void invoke<boolean>("cancel_completion_action").catch((reason) => notify("error", String(reason)));
            setCompletionAction(null);
          }}
        />
      ) : null}

      <CommandPalette open={paletteOpen} commands={paletteCommands} onClose={() => setPaletteOpen(false)} />

      <AddDownloadModal
        open={modalOpen}
        url={url}
        submitting={creatingTasks}
        engineReady={allReady}
        error={addError}
        linkCount={extractHttpUrls(url).length}
        queues={queues}
        defaultDirectory={downloadSettings?.defaultDirectory ?? null}
        onUrlChange={setUrl}
        onClose={() => {
          if (creatingTasks) return;
          setAddError(null);
          setModalOpen(false);
        }}
        onSubmit={(action) => void createDownloadTasks(action)}
      />
    </>
  );
}

export default App;
