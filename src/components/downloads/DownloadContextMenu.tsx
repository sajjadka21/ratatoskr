import {
  ChevronRight,
  Copy,
  ExternalLink,
  FileText,
  FolderOpen,
  Layers3,
  Link2,
  ListX,
  Play,
  SignalHigh,
  Trash2,
} from "lucide-react";

import {
  openPath,
  revealItemInDir,
} from "@tauri-apps/plugin-opener";

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
} from "../../types/download";

import "./DownloadContextMenu.css";

type DownloadContextMenuProps = {
  item: DownloadListItem | null;
  queues: DownloadQueue[];
  x: number;
  y: number;
  onClose: () => void;
  onShowDetails: (id: string) => void;
  onStart: (id: string) => void;
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
};

/// Which expandable group is open. Only one can be open at a time so the menu
/// never grows past the window on a small screen.
type OpenGroup = "queue" | "priority" | null;

const PRIORITIES: Array<{
  value: DownloadPriority;
  label: string;
}> = [
  { value: "very_high", label: "Very high" },
  { value: "high", label: "High" },
  { value: "normal", label: "Normal" },
  { value: "low", label: "Low" },
];

export function DownloadContextMenu({
  item,
  queues,
  x,
  y,
  onClose,
  onShowDetails,
  onStart,
  onAssignQueue,
  onRemoveFromQueue,
  onChangePriority,
  onRemoveFromHistory,
}: DownloadContextMenuProps) {
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

  const canQueue = ["created", "queued"].includes(status);

  async function handleOpenFile() {
    if (!destinationPath) return;

    try {
      await openPath(destinationPath);
      onClose();
    } catch (reason) {
      console.error(
        "Could not open file:",
        reason,
      );
    }
  }

  async function handleReveal() {
    if (!destinationPath) return;

    try {
      await revealItemInDir(destinationPath);
      onClose();
    } catch (reason) {
      console.error(
        "Could not reveal file:",
        reason,
      );
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
      {status === "created" ? (
        <>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              onStart(item.id);
              onClose();
            }}
          >
            <Play size={15} />
            <span>Start Download</span>
          </button>

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
        <span>Open File</span>
      </button>

      <button
        type="button"
        role="menuitem"
        disabled={!hasFile}
        onClick={() => void handleReveal()}
      >
        <FolderOpen size={15} />
        <span>Show in Folder</span>
      </button>

      <div className="download-context-menu__divider" />

      <button
        type="button"
        role="menuitem"
        onClick={() =>
          void copyText(item.sourceUrl, "url")
        }
      >
        <Link2 size={15} />

        <span>
          {copied === "url"
            ? "URL Copied"
            : "Copy URL"}
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
          {copied === "path"
            ? "Path Copied"
            : "Copy Path"}
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
              {item.queueId
                ? "Change Queue"
                : "Add to Queue"}
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
                  No queues yet
                </span>
              )}
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
            <span>Change Priority</span>

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
                  key={priority.value}
                  type="button"
                  role="menuitem"
                  disabled={priority.value === item.priority}
                  onClick={() => {
                    onChangePriority(item, priority.value);
                    onClose();
                  }}
                >
                  <span>{priority.label}</span>
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
              <span>Remove from Queue</span>
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
        <span>Details</span>
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
          {hasFile || status === "completed"
            ? "Remove from History..."
            : "Remove Task..."}
        </span>
      </button>
    </div>
  );
}




