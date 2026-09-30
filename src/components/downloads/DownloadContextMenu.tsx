import { invoke } from "@tauri-apps/api/core";
import {
  AppWindow,
  ChevronRight,
  Copy,
  ExternalLink,
  FileText,
  FolderOpen,
  Layers3,
  Link2,
  ListX,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  SignalHigh,
  Trash2,
  XCircle,
} from "lucide-react";


import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import type {
  DownloadListItem,
  DownloadPriority,
  DownloadQueue,
  TaskAction,
} from "../../types/download";

import { availableActions } from "../../utils/taskActions";
import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";

import "./DownloadContextMenu.css";

type DownloadContextMenuProps = {
  item: DownloadListItem | null;
  queues: DownloadQueue[];
  x: number;
  y: number;
  onClose: () => void;
  onShowDetails: (id: string) => void;
  onAction: (
    item: DownloadListItem,
    action: TaskAction,
  ) => void;
  onAssignQueue: (
    item: DownloadListItem,
    queueId: string,
  ) => void;
  onRemoveFromQueue: (
    item: DownloadListItem,
  ) => void;
  onChangePriority: (
    item: DownloadListItem,
    priority: DownloadPriority,
  ) => void;
  onRemoveFromHistory: (
    item: DownloadListItem,
  ) => void;
  onRefreshSource: (item: DownloadListItem) => void;
  /** Makes a new queue with this name and puts the download in it. */
  onCreateQueue?: (item: DownloadListItem, name: string) => void;
  onError?: (message: string) => void;
};

const ACTION_ICONS = {
  start: Play,
  resume: Play,
  pause: Pause,
  retry: RotateCcw,
  cancel: XCircle,
  restart: RefreshCw,
} as const;

/// Which expandable group is open. Only one can be open at a time so the menu
/// never grows past the window on a small screen.
type OpenGroup = "queue" | "priority" | null;

const PRIORITIES: DownloadPriority[] = ["very_high", "high", "normal", "low"];

export function DownloadContextMenu({
  item,
  queues,
  x,
  y,
  onClose,
  onShowDetails,
  onAction,
  onAssignQueue,
  onRemoveFromQueue,
  onChangePriority,
  onRemoveFromHistory,
  onRefreshSource,
  onCreateQueue,
  onError,
}: DownloadContextMenuProps) {
  const [newQueue, setNewQueue] = useState("");
  const { t } = useI18n();
  const menuRef = useRef<HTMLDivElement>(null);

  const [position, setPosition] = useState({
    left: x,
    top: y,
  });

  const [copied, setCopied] = useState<
    "url" | "path" | null
  >(null);

  const [openGroup, setOpenGroup] =
    useState<OpenGroup>(null);

  useLayoutEffect(() => {
    const menu = menuRef.current;

    if (!menu || !item) return;

    const rect = menu.getBoundingClientRect();
    const padding = 10;

    setPosition({
      left: Math.max(
        padding,
        Math.min(
          x,
          window.innerWidth - rect.width - padding,
        ),
      ),

      top: Math.max(
        padding,
        Math.min(
          y,
          window.innerHeight - rect.height - padding,
        ),
      ),
    });
  }, [x, y, item, openGroup]);

  useEffect(() => {
    setOpenGroup(null);
  }, [item?.id]);

  useEffect(() => {
    if (!item) return;

    function handleMouseDown(event: MouseEvent) {
      const menu = menuRef.current;

      if (
        menu &&
        !menu.contains(event.target as Node)
      ) {
        onClose();
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener(
      "mousedown",
      handleMouseDown,
    );

    window.addEventListener(
      "keydown",
      handleKeyDown,
    );

    return () => {
      window.removeEventListener(
        "mousedown",
        handleMouseDown,
      );

      window.removeEventListener(
        "keydown",
        handleKeyDown,
      );
    };
  }, [item, onClose]);

  if (!item) return null;

  const status = item.status.toLowerCase();
  const destinationPath = item.destinationPath;

  const hasFile =
    Boolean(destinationPath) &&
    status === "completed";

  // Mirrors the canonical rules the backend enforces: a task an executor may
  // still be writing to is neither removable nor free to move between queues.
  const isRemovable = [
    "created",
    "queued",
    "completed",
    "failed",
    "cancelled",
  ].includes(status);

  const canQueue = [
    "created",
    "queued",
    "paused",
    "failed",
    "cancelled",
  ].includes(status);

  const actions = availableActions(item);

  async function handleOpenFile() {
    if (!destinationPath) return;

    try {
      await invoke("open_download_file", { id: item!.id });
      onClose();
    } catch (reason) {
      onClose();
      onError?.(t("context.openFailed", { reason: String(reason) }));
    }
  }

  async function handleReveal() {
    if (!destinationPath) return;

    try {
      await invoke("reveal_download_file", { id: item!.id });
      onClose();
    } catch (reason) {
      onClose();
      onError?.(t("context.revealFailed", { reason: String(reason) }));
    }
  }

  async function copyText(
    value: string,
    field: "url" | "path",
  ) {
    try {
      await navigator.clipboard.writeText(value);

      setCopied(field);

      window.setTimeout(() => {
        onClose();
      }, 450);
    } catch (reason) {
      console.error(
        "Could not copy:",
        reason,
      );
    }
  }

  return (
    <div
      ref={menuRef}
      className="download-context-menu"
      style={{
        left: position.left,
        top: position.top,
      }}
      role="menu"
    >
      {actions.length > 0 ? (
        <>
          {actions.map((action) => {
            const ActionIcon = ACTION_ICONS[action];

            return (
              <button
                key={action}
                type="button"
                role="menuitem"
                onClick={() => {
                  onAction(item, action);
                  onClose();
                }}
              >
                <ActionIcon size={15} />
                <span>{t(`action.${action}` as MessageKey)}</span>
              </button>
            );
          })}

          <div className="download-context-menu__divider" />
        </>
      ) : null}

      <button
        type="button"
        role="menuitem"
        disabled={!hasFile}
        onClick={() => void handleOpenFile()}
      >
        <ExternalLink size={15} />
        <span>{t("action.open")}</span>
      </button>

      <button
        type="button"
        role="menuitem"
        disabled={!hasFile}
        onClick={() => {
          void invoke("open_download_with", { id: item.id })
            .then(onClose)
            .catch((reason) => {
              onClose();
              onError?.(t("context.openFailed", { reason: String(reason) }));
            });
        }}
      >
        <AppWindow size={15} />
        <span>{t("action.openWith")}</span>
      </button>

      <button
        type="button"
        role="menuitem"
        disabled={!hasFile}
        onClick={() => void handleReveal()}
      >
        <FolderOpen size={15} />
        <span>{t("action.reveal")}</span>
      </button>

      <button
        type="button"
        role="menuitem"
        disabled={!canQueue}
        onClick={() => {
          onRefreshSource(item);
          onClose();
        }}
      >
        <RefreshCw size={15} />
        <span>{t("action.refreshSource")}</span>
      </button>

      <div className="download-context-menu__divider" />

      <button
        type="button"
        role="menuitem"
        onClick={() =>
          void copyText(item.sourceUrl.replace(/#rud-quality=[^&]*$/, ""), "url")
        }
      >
        <Link2 size={15} />

        <span>
          {copied === "url" ? t("action.copied") : t("action.copyUrl")}
        </span>
      </button>

      <button
        type="button"
        role="menuitem"
        disabled={!destinationPath}
        onClick={() => {
          if (!destinationPath) return;

          void copyText(
            destinationPath,
            "path",
          );
        }}
      >
        <Copy size={15} />

        <span>
          {copied === "path" ? t("action.copied") : t("action.copyPath")}
        </span>
      </button>

      <div className="download-context-menu__divider" />

      {canQueue ? (
        <>
          <button
            type="button"
            role="menuitem"
            aria-expanded={openGroup === "queue"}
            onClick={() =>
              setOpenGroup(
                openGroup === "queue" ? null : "queue",
              )
            }
          >
            <Layers3 size={15} />

            <span>
              {item.queueId ? t("action.changeQueue") : t("action.addToQueue")}
            </span>

            <ChevronRight
              size={14}
              className={`download-context-menu__chevron ${
                openGroup === "queue"
                  ? "download-context-menu__chevron--open"
                  : ""
              }`}
            />
          </button>

          {openGroup === "queue" ? (
            <div className="download-context-menu__group">
              {queues.length > 0 ? (
                queues.map((queue) => (
                  <button
                    key={queue.id}
                    type="button"
                    role="menuitem"
                    disabled={queue.id === item.queueId}
                    onClick={() => {
                      onAssignQueue(item, queue.id);
                      onClose();
                    }}
                  >
                    <span>{queue.name}</span>
                  </button>
                ))
              ) : (
                <span className="download-context-menu__empty">
                  {t("action.noQueues")}
                </span>
              )}
              {onCreateQueue ? (
                <form
                  className="download-context-menu__new-queue"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const name = newQueue.trim();
                    if (!name) return;
                    onCreateQueue(item, name);
                    setNewQueue("");
                    onClose();
                  }}
                >
                  <input
                    value={newQueue}
                    placeholder={t("action.newQueue")}
                    aria-label={t("action.newQueue")}
                    onChange={(event) => setNewQueue(event.target.value)}
                    onKeyDown={(event) => event.stopPropagation()}
                  />
                  <button type="submit" disabled={!newQueue.trim()}>
                    {t("action.createQueue")}
                  </button>
                </form>
              ) : null}
            </div>
          ) : null}

          <button
            type="button"
            role="menuitem"
            aria-expanded={openGroup === "priority"}
            onClick={() =>
              setOpenGroup(
                openGroup === "priority" ? null : "priority",
              )
            }
          >
            <SignalHigh size={15} />
            <span>{t("action.changePriority")}</span>

            <ChevronRight
              size={14}
              className={`download-context-menu__chevron ${
                openGroup === "priority"
                  ? "download-context-menu__chevron--open"
                  : ""
              }`}
            />
          </button>

          {openGroup === "priority" ? (
            <div className="download-context-menu__group">
              {PRIORITIES.map((priority) => (
                <button
                  key={priority}
                  type="button"
                  role="menuitem"
                  disabled={priority === item.priority}
                  onClick={() => {
                    onChangePriority(item, priority);
                    onClose();
                  }}
                >
                  <span>{t(`priority.${priority}` as MessageKey)}</span>
                </button>
              ))}
            </div>
          ) : null}

          {item.queueId ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                onRemoveFromQueue(item);
                onClose();
              }}
            >
              <ListX size={15} />
              <span>{t("action.removeFromQueue")}</span>
            </button>
          ) : null}

          <div className="download-context-menu__divider" />
        </>
      ) : null}

      <button
        type="button"
        role="menuitem"
        onClick={() => {
          onShowDetails(item.id);
          onClose();
        }}
      >
        <FileText size={15} />
        <span>{t("action.details")}</span>
      </button>
      <div className="download-context-menu__divider" />

      <button
        type="button"
        role="menuitem"
        className="download-context-menu__danger"
        disabled={!isRemovable}
        onClick={() => {
          onRemoveFromHistory(item);
        }}
      >
        <Trash2 size={15} />

        <span>
          {hasFile || status === "completed" ? t("action.removeHistory") : t("action.remove")}…
        </span>
      </button>
    </div>
  );
}




