import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  ExternalLink,
  FileDown,
  FolderOpen,
  Pause,
  Play,
  RotateCcw,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { VideoQualityPicker } from "../components/downloads/VideoQualityPicker";
import { FileBadge } from "../components/common/FileBadge";
import { useI18n } from "../i18n/I18n";
import { messages } from "../i18n/messages";
import type {
  ConnectionCheck,
  DownloadCategory,
  DownloadListItem,
  DownloadQueue,
  DownloadSettings,
} from "../types/download";
import { extractHttpUrls } from "../utils/downloadLinks";
import { displayName } from "../utils/fileKind";
import { categoryName } from "../utils/categories";
import { formatHost } from "../utils/format";
import { friendlyError, noticeText } from "../utils/notices";
import {
  isVideoPage,
  withVideoQualityForAll,
  type VideoQuality,
} from "../utils/videoPages";

import "../components/downloads/AddDownloadModal.css";
import "./MiniWindow.css";

const DOWNLOAD_TASK_EVENT = "download-task-event";
const QUEUE_RUNNER_EVENT = "queue-runner-event";
const MINI_LINKS_EVENT = "mini-links";
/** More links than this, started together, go through a queue. */
const BATCH_START_LIMIT = 3;

type Live = { bytesPerSecond: number | null; etaSeconds: number | null };

type TaskEvent = {
  kind: string;
  downloadId: string | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
  download: DownloadListItem | null;
};

/**
 * The small download window. `mode=add` takes links (from the clipboard or a
 * video page in the browser) and starts them; `mode=task` follows one
 * download, first asking where to save it when `confirm=1` (a download the
 * browser handed over).
 */
export function MiniWindow() {
  const params = useMemo(() => new URLSearchParams(window.location.search), []);
  const [taskId, setTaskId] = useState<string | null>(
    params.get("mode") === "task" ? params.get("id") : null,
  );
  const [confirming, setConfirming] = useState(params.get("confirm") === "1");

  const follow = useCallback((id: string) => {
    setTaskId(id);
    setConfirming(false);
  }, []);

  const root = useRef<HTMLDivElement>(null);
  useFitWindow(root);

  return (
    <div className="mini" ref={root}>
      {taskId ? (
        <TaskView id={taskId} confirming={confirming} onStarted={follow} />
      ) : (
        <AddView onStarted={follow} />
      )}
    </div>
  );
}

function close() {
  void getCurrentWindow().close();
}

const AFTER_ONE = ["none", "open", "sleep", "hibernate", "shutdown", "exit_app"] as const;

/** What happens when this download finishes, like other managers offer. */
function AfterThis({ id }: { id: string }) {
  const { t } = useI18n();
  const [value, setValue] = useState("none");

  useEffect(() => {
    invoke<string>("get_download_after", { id })
      .then(setValue)
      .catch(() => {});
  }, [id]);

  return (
    <label className="mini__field mini__field--wide">
      <span>{t("afterOne.label")}</span>
      <select
        value={value}
        onChange={(event) =>
          void invoke<string>("set_download_after", { id, action: event.target.value })
            .then(setValue)
            .catch(() => {})
        }
      >
        {AFTER_ONE.map((action) => (
          <option key={action} value={action}>
            {t(`afterOne.${action}`)}
          </option>
        ))}
      </select>
    </label>
  );
}

/** Heights the window may take, in logical pixels. */
const MIN_HEIGHT = 180;
const MAX_HEIGHT = 640;

/**
 * Keeps the window as tall as what it shows, so there is neither empty space
 * under a short form nor a scroll bar in a long one: the body's content plus
 * the footer, measured whenever anything inside changes.
 */
function useFitWindow(root: React.RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    let frame = 0;
    let lastHeight = 0;
    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const body = element.querySelector<HTMLElement>(".mini__body");
        const footer = element.querySelector<HTMLElement>(".mini__footer");
        if (!body) return;
        const style = getComputedStyle(body);
        const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
        const gap = parseFloat(style.rowGap) || 0;
        const children = Array.from(body.children) as HTMLElement[];
        const content =
          children.reduce((sum, child) => sum + child.getBoundingClientRect().height, 0) +
          gap * Math.max(0, children.length - 1);
        const wanted = Math.round(
          Math.min(
            MAX_HEIGHT,
            Math.max(MIN_HEIGHT, content + padding + (footer?.getBoundingClientRect().height ?? 0)),
          ),
        );
        if (Math.abs(wanted - lastHeight) < 2) return;
        lastHeight = wanted;
        void getCurrentWindow()
          .setSize(new LogicalSize(window.innerWidth, wanted))
          .catch(() => {});
      });
    };
    const resize = new ResizeObserver(measure);
    const watchChildren = () => {
      resize.disconnect();
      element.querySelectorAll(".mini__body > *, .mini__footer").forEach((child) => resize.observe(child));
      measure();
    };
    const mutations = new MutationObserver(watchChildren);
    mutations.observe(element, { childList: true, subtree: true });
    watchChildren();
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      mutations.disconnect();
    };
  }, [root]);
}

// ---------------------------------------------------------------------------
// Adding links
// ---------------------------------------------------------------------------

function AddView({ onStarted }: { onStarted: (id: string) => void }) {
  const { t, fmt } = useI18n();
  const [links, setLinks] = useState<string[]>([]);
  const [quality, setQuality] = useState<VideoQuality | null>(null);
  const [playlist, setPlaylist] = useState<string[] | null>(null);
  const [lookingUp, setLookingUp] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const collect = () =>
      invoke<string[]>("take_mini_links")
        .then((incoming) => {
          if (incoming.length === 0) return;
          setLinks((current) => [...new Set([...current, ...incoming])]);
        })
        .catch(() => {});
    void collect();
    const subscription = listen(MINI_LINKS_EVENT, () => void collect());
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  const text = links.join("\n");
  const single = links.length === 1 ? links[0] : null;
  const video = single !== null && isVideoPage(single);
  const waitingForList = lookingUp && video && Boolean(single?.match(/[?&]list=|\/playlist|\/@|\/channel\//));

  async function add(start: boolean) {
    setBusy(true);
    setError(null);
    const chosen = playlist && links.length === 1 ? playlist : links;
    const withQuality =
      quality === null ? chosen : withVideoQualityForAll(chosen.join("\n"), quality).split("\n");
    try {
      const { folder, name } = currentChoice.current();
      const created: DownloadListItem[] = [];
      let failed: string | null = null;
      for (const link of withQuality) {
        try {
          created.push(
            await invoke<DownloadListItem>("create_download_task", {
              url: link,
              directory: folder,
              filename: withQuality.length === 1 ? name : null,
            }),
          );
        } catch (reason) {
          failed ??= String(reason);
        }
      }
      if (created.length === 0) {
        setError(friendlyError(failed ?? t("toast.noLinks"), t));
        return;
      }
      if (!start) {
        close();
        return;
      }
      if (created.length > BATCH_START_LIMIT) {
        await startThroughQueue(created, t);
        close();
        return;
      }
      for (const task of created) {
        await invoke("start_download", { id: task.id }).catch(() => {});
      }
      if (created.length === 1) {
        onStarted(created[0].id);
      } else {
        for (const task of created) {
          await invoke("open_download_window", { id: task.id }).catch(() => {});
        }
        close();
      }
    } catch (reason) {
      setError(friendlyError(String(reason), t));
    } finally {
      setBusy(false);
    }
  }

  const currentChoice = useRef<() => SaveChoice>(() => ({ folder: null, name: null }));
  const [serverName, setServerName] = useState<string | null>(null);

  if (links.length === 0) {
    return (
      <div className="mini__body mini__body--center">
        <p className="mini__muted">{t("mini.waiting")}</p>
      </div>
    );
  }

  return (
    <>
      <div className="mini__body">
        <header className="mini__header">
          <div className="mini__icon">
            <FileDown size={18} />
          </div>
          <div className="mini__heading">
            <h1>
              {links.length > 1
                ? t("mini.titleMany", { count: fmt.number(links.length) })
                : video
                  ? t("mini.titleVideo")
                  : t("mini.titleFile")}
            </h1>
            <span className="ltr">{formatHost(links[0])}</span>
          </div>
        </header>

        {single && !video ? <FileFacts url={single} onName={setServerName} /> : null}
        {links.length > 1 ? (
          <ul className="mini__links" dir="ltr">
            {links.slice(0, 50).map((link) => (
              <li key={link} title={link}>
                {link}
              </li>
            ))}
          </ul>
        ) : null}

        <VideoQualityPicker
          text={text}
          quality={quality}
          onQualityChange={setQuality}
          onReplaceLinks={(next) => setLinks(extractHttpUrls(next))}
          onPlaylist={setPlaylist}
          onBusy={setLookingUp}
        />

        <SaveAs
          suggestedName={single && !video ? serverName : null}
          register={(read) => (currentChoice.current = read)}
          disabled={busy}
        />

        {error ? <div className="mini__error">{error}</div> : null}
      </div>

      <footer className="mini__footer">
        <button
          type="button"
          className="mini__primary"
          disabled={busy || waitingForList}
          onClick={() => void add(true)}
        >
          <Download size={15} />
          {busy ? t("add.creating") : t("mini.start")}
        </button>
        <button type="button" className="mini__secondary" disabled={busy} onClick={() => void add(false)}>
          {t("mini.later")}
        </button>
        <span className="mini__spacer" />
        <button type="button" className="mini__ghost" disabled={busy} onClick={close}>
          {t("add.cancel")}
        </button>
      </footer>
    </>
  );
}

async function startThroughQueue(created: DownloadListItem[], t: (key: "add.batchQueueName") => string) {
  const names = new Set([t("add.batchQueueName"), messages.en["add.batchQueueName"], messages.fa["add.batchQueueName"]]);
  const queues = await invoke<DownloadQueue[]>("list_queues");
  const queue =
    queues.find((candidate) => names.has(candidate.name)) ??
    (await invoke<DownloadQueue>("create_queue", {
      name: t("add.batchQueueName"),
      maxConcurrent: BATCH_START_LIMIT,
      maxConcurrentPerHost: 2,
      defaultPriority: "normal",
    }));
  for (const task of created) {
    await invoke("enqueue_download_task", { id: task.id, queueId: queue.id, priority: null }).catch(
      () => {},
    );
  }
  await invoke("start_queue", { queueId: queue.id }).catch(() => {});
}

/** Name and size of a file link, asked from the server like a download would. */
function FileFacts({ url, onName }: { url: string; onName?: (name: string) => void }) {
  const { t, fmt } = useI18n();
  const [check, setCheck] = useState<ConnectionCheck | null>(null);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setChecking(true);
    invoke<ConnectionCheck>("check_connection", { url })
      .then((result) => {
        if (!cancelled) setCheck(result);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setChecking(false);
      });
    return () => {
      cancelled = true;
    };
  }, [url]);

  const name = check?.filename ?? fallbackName(url);
  useEffect(() => {
    onName?.(name);
  }, [name, onName]);
  return (
    <div className="mini__file">
      <FileBadge name={name} />
      <div className="mini__file-text">
        <strong className="ltr" title={name}>
          {name}
        </strong>
        <span>
          {checking
            ? t("mini.checking")
            : check?.totalBytes
              ? fmt.bytes(check.totalBytes)
              : check && !check.reachable
                ? t("mini.unreachable")
                : t("mini.sizeUnknown")}
          {check && check.reachable && !check.rangeSupported ? ` · ${t("mini.noResume")}` : ""}
        </span>
      </div>
    </div>
  );
}

function fallbackName(url: string): string {
  try {
    const last = new URL(url).pathname.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : new URL(url).hostname;
  } catch {
    return url;
  }
}

/** Where and under what name to save: `null` keeps what the app would choose. */
type SaveChoice = { folder: string | null; name: string | null };

/**
 * The name to save under (for one file), the category and the folder, like
 * the "Save as" part of other download managers' start dialog. Picking a
 * category uses its folder; "Change…" picks any folder.
 */
function SaveAs({
  suggestedName,
  register,
  disabled,
}: {
  /** The server's name for one file; without it no name field is shown. */
  suggestedName: string | null;
  register: (read: () => SaveChoice) => void;
  disabled: boolean;
}) {
  const { t } = useI18n();
  const [usual, setUsual] = useState<string | null>(null);
  const [categories, setCategories] = useState<DownloadCategory[]>([]);
  const [categoryId, setCategoryId] = useState<string>("");
  const [chosen, setChosen] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);

  useEffect(() => {
    invoke<DownloadSettings>("get_download_settings")
      .then((settings) => setUsual(settings.defaultDirectory ?? settings.systemDirectory))
      .catch(() => {});
    invoke<DownloadCategory[]>("list_categories")
      .then(setCategories)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!nameEdited) setName(suggestedName ?? "");
  }, [suggestedName, nameEdited]);

  // The category its extension belongs to, as the rules would file it.
  const extension = /\.([a-z0-9]{1,8})$/i.exec(name || suggestedName || "")?.[1]?.toLowerCase();
  const matched = extension
    ? categories.find((category) => category.extensions.includes(extension))
    : undefined;
  const category = categories.find((candidate) => candidate.id === categoryId);
  const categoryFolder = category?.defaultDirectory ?? null;
  const folder = chosen ?? categoryFolder;
  const shown = folder ?? matched?.defaultDirectory ?? usual;
  const trimmed = name.trim();
  const renamed = nameEdited && trimmed && trimmed !== suggestedName ? trimmed : null;

  useEffect(() => {
    register(() => ({ folder, name: renamed }));
  }, [folder, renamed, register]);

  async function choose() {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: t("add.chooseFolderTitle"),
        defaultPath: shown ?? undefined,
      });
      if (typeof picked === "string" && picked) setChosen(picked);
    } catch {
      // Closing the picker is not an error.
    }
  }

  return (
    <div className="mini__save">
      {suggestedName !== null ? (
        <label className="mini__field">
          <span>{t("mini.saveAs")}</span>
          <input
            dir="ltr"
            value={name}
            disabled={disabled}
            spellCheck={false}
            onChange={(event) => {
              setName(event.target.value);
              setNameEdited(true);
            }}
          />
        </label>
      ) : null}
      {categories.length > 0 ? (
        <label className="mini__field">
          <span>{t("mini.category")}</span>
          <select
            value={categoryId}
            disabled={disabled}
            onChange={(event) => {
              setCategoryId(event.target.value);
              setChosen(null);
            }}
          >
            <option value="">
              {matched
                ? t("mini.categoryAuto", { name: categoryName(matched.id, matched.name, t) })
                : t("mini.categoryNone")}
            </option>
            {categories.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {categoryName(candidate.id, candidate.name, t)}
              </option>
            ))}
          </select>
        </label>
      ) : null}
      <div className="mini__folder">
        <FolderOpen size={14} aria-hidden="true" />
        <span className="mini__folder-path" title={shown ?? undefined} dir={shown ? "ltr" : undefined}>
          {shown ?? t("add.systemDownloads")}
        </span>
        {chosen ? (
          <button type="button" disabled={disabled} onClick={() => setChosen(null)}>
            {t("add.resetFolder")}
          </button>
        ) : null}
        <button type="button" disabled={disabled} onClick={() => void choose()}>
          {t("add.changeFolder")}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// One download
// ---------------------------------------------------------------------------

function TaskView({
  id,
  confirming,
  onStarted,
}: {
  id: string;
  confirming: boolean;
  onStarted: (id: string) => void;
}) {
  const { t, fmt, language } = useI18n();
  const [item, setItem] = useState<DownloadListItem | null>(null);
  const [live, setLive] = useState<Live>({ bytesPerSecond: null, etaSeconds: null });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [gone, setGone] = useState(false);
  const currentChoice = useRef<() => SaveChoice>(() => ({ folder: null, name: null }));
  const [serverName, setServerName] = useState<string | null>(null);

  const load = useCallback(() => {
    invoke<DownloadListItem | null>("get_download", { id })
      .then((found) => {
        if (found) setItem(found);
        else setGone(true);
      })
      .catch(() => {});
  }, [id]);

  useEffect(() => {
    setGone(false);
    load();
  }, [load]);

  useEffect(() => {
    const apply = (event: TaskEvent) => {
      if (event.kind === "removed" && event.downloadId === id) {
        setGone(true);
        return;
      }
      if (event.download && event.download.id === id) {
        setItem(event.download);
        setLive({ bytesPerSecond: null, etaSeconds: null });
        return;
      }
      const progressId = event.downloadId;
      if (progressId === id && event.downloadedBytes !== null) {
        setItem((current) =>
          current
            ? {
                ...current,
                status:
                  ["created", "probing", "queued", "retrying"].includes(current.status.toLowerCase())
                    ? "downloading"
                    : current.status,
                downloadedBytes: event.downloadedBytes ?? current.downloadedBytes,
                totalBytes: event.totalBytes ?? current.totalBytes,
              }
            : current,
        );
        setLive({ bytesPerSecond: event.bytesPerSecond, etaSeconds: event.etaSeconds });
      }
    };
    const own = listen<TaskEvent>(DOWNLOAD_TASK_EVENT, ({ payload }) => apply(payload));
    const queued = listen<TaskEvent>(QUEUE_RUNNER_EVENT, ({ payload }) => apply(payload));
    return () => {
      void own.then((unlisten) => unlisten());
      void queued.then((unlisten) => unlisten());
    };
  }, [id]);

  /** Runs a command on this download; false when it was refused. */
  async function run(command: string, args: Record<string, unknown> = { id }): Promise<boolean> {
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<DownloadListItem | null>(command, args);
      if (result && typeof result === "object" && "id" in result && result.id === id) setItem(result);
      return true;
    } catch (reason) {
      setError(friendlyError(String(reason), t));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function confirmStart() {
    setBusy(true);
    setError(null);
    try {
      const { folder, name } = currentChoice.current();
      if (folder) await invoke("set_download_folder", { id, directory: folder });
      if (name) await invoke("set_download_name", { id, filename: name });
      const target = await invoke<string>("start_handoff", { id });
      onStarted(target);
    } catch (reason) {
      setError(friendlyError(String(reason), t));
    } finally {
      setBusy(false);
    }
  }

  async function discard() {
    setBusy(true);
    try {
      await invoke("remove_download", { id, deleteFile: false });
    } catch {
      // Already gone or running: nothing to discard.
    }
    close();
  }

  if (gone) {
    return (
      <div className="mini__body mini__body--center">
        <p className="mini__muted">{t("mini.gone")}</p>
        <button type="button" className="mini__secondary" onClick={close}>
          {t("mini.close")}
        </button>
      </div>
    );
  }
  if (!item) {
    return (
      <div className="mini__body mini__body--center">
        <p className="mini__muted">{t("mini.checking")}</p>
      </div>
    );
  }

  const name = displayName(item);
  const status = item.status.toLowerCase();
  const total = item.totalBytes;
  const percent = total ? Math.min(100, (item.downloadedBytes / total) * 100) : null;

  if (confirming && status === "created") {
    return (
      <>
        <div className="mini__body">
          <header className="mini__header">
            <div className="mini__icon">
              <FileDown size={18} />
            </div>
            <div className="mini__heading">
              <h1>{t("mini.titleFile")}</h1>
              <span className="ltr">{formatHost(item.sourceUrl)}</span>
            </div>
          </header>
          <FileFacts url={item.sourceUrl} onName={setServerName} />
          <SaveAs
            suggestedName={serverName}
            register={(read) => (currentChoice.current = read)}
            disabled={busy}
          />
          {error ? <div className="mini__error">{error}</div> : null}
        </div>
        <footer className="mini__footer">
          <button type="button" className="mini__primary" disabled={busy} onClick={() => void confirmStart()}>
            <Download size={15} />
            {t("mini.start")}
          </button>
          <button type="button" className="mini__secondary" disabled={busy} onClick={close}>
            {t("mini.later")}
          </button>
          <span className="mini__spacer" />
          <button type="button" className="mini__ghost" disabled={busy} onClick={() => void discard()}>
            {t("add.cancel")}
          </button>
        </footer>
      </>
    );
  }

  const running = ["downloading", "probing", "finalizing", "queued", "created"].includes(status);
  const stoppable = status === "downloading" || status === "probing";
  const resumable = status === "paused" || status === "retrying";

  if (status === "completed") {
    return (
      <>
        <div className="mini__body">
          <div className="mini__done">
            <CheckCircle2 size={26} />
            <div>
              <h1>{t("mini.done")}</h1>
              <strong className="ltr" title={name}>
                {name}
              </strong>
              <span>{total ? fmt.bytes(total) : null}</span>
            </div>
          </div>
          {error ? <div className="mini__error">{error}</div> : null}
        </div>
        <footer className="mini__footer">
          <button type="button" className="mini__primary" onClick={() => void run("open_download_file").then((ok) => ok && close())}>
            <ExternalLink size={15} />
            {t("mini.openFile")}
          </button>
          <button type="button" className="mini__secondary" onClick={() => void run("reveal_download_file").then((ok) => ok && close())}>
            <FolderOpen size={15} />
            {t("mini.openFolder")}
          </button>
          <span className="mini__spacer" />
          <button type="button" className="mini__ghost" onClick={close}>
            {t("mini.close")}
          </button>
        </footer>
      </>
    );
  }

  const failed = status === "failed" || status === "cancelled";
  const reason =
    status === "failed"
      ? (noticeText(item.errorCode, item.errorMessage, t, language) ?? item.errorMessage)
      : null;

  return (
    <>
      <div className="mini__body">
        <div className="mini__task">
          <FileBadge item={item} />
          <div className="mini__file-text">
            <strong className="ltr" title={name}>
              {name}
            </strong>
            <span>
              {t(`status.${status}` as "status.downloading")}
              {percent !== null && !failed ? ` · ${fmt.percent(percent)}` : ""}
            </span>
          </div>
        </div>

        {!failed ? (
          <div
            className={`mini__bar ${percent === null ? "mini__bar--unknown" : ""}`}
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent ?? undefined}
          >
            <span style={{ inlineSize: `${percent ?? 30}%` }} />
          </div>
        ) : null}

        {failed ? (
          <div className="mini__failure">
            <AlertTriangle size={15} />
            <span>{reason ?? t("status.cancelled")}</span>
          </div>
        ) : (
          <dl className="mini__facts">
            <div>
              <dt>{t("mini.received")}</dt>
              <dd>
                {fmt.bytes(item.downloadedBytes)}
                {total ? ` ${t("mini.of")} ${fmt.bytes(total)}` : ""}
              </dd>
            </div>
            <div>
              <dt>{t("mini.speed")}</dt>
              <dd>{stoppable ? (fmt.rate(live.bytesPerSecond) ?? "—") : "—"}</dd>
            </div>
            <div>
              <dt>{t("mini.remaining")}</dt>
              <dd>{stoppable ? (fmt.duration(live.etaSeconds) ?? "—") : "—"}</dd>
            </div>
          </dl>
        )}
        {!failed ? <AfterThis id={id} /> : null}
        {error ? <div className="mini__error">{error}</div> : null}
      </div>

      <footer className="mini__footer">
        {stoppable ? (
          <button type="button" className="mini__secondary" disabled={busy} onClick={() => void run("pause_download")}>
            <Pause size={15} />
            {t("mini.pause")}
          </button>
        ) : null}
        {resumable ? (
          <button type="button" className="mini__primary" disabled={busy} onClick={() => void run("resume_download")}>
            <Play size={15} />
            {t("mini.resume")}
          </button>
        ) : null}
        {status === "failed" ? (
          <button type="button" className="mini__primary" disabled={busy} onClick={() => void run("start_download")}>
            <RotateCcw size={15} />
            {t("mini.retry")}
          </button>
        ) : null}
        {running || resumable ? (
          <button type="button" className="mini__ghost" disabled={busy} onClick={() => void run("cancel_download")}>
            <X size={15} />
            {t("mini.cancelDownload")}
          </button>
        ) : null}
        <span className="mini__spacer" />
        <button
          type="button"
          className="mini__ghost"
          onClick={() => void invoke("show_main_window", { focus: id }).catch(() => {})}
        >
          {t("mini.showInApp")}
        </button>
        {failed ? (
          <button type="button" className="mini__ghost" onClick={close}>
            {t("mini.close")}
          </button>
        ) : null}
      </footer>
    </>
  );
}
