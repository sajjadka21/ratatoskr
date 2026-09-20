import {
  Copy,
  ExternalLink,
  FileText,
  FolderOpen,
  Link2,
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

import type { DownloadListItem } from "../../types/download";

import "./DownloadContextMenu.css";

type DownloadContextMenuProps = {
  item: DownloadListItem | null;
  x: number;
  y: number;
  onClose: () => void;
  onShowDetails: (id: string) => void;
};

export function DownloadContextMenu({
  item,
  x,
  y,
  onClose,
  onShowDetails,
}: DownloadContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  const [position, setPosition] = useState({
    left: x,
    top: y,
  });

  const [copied, setCopied] = useState<
    "url" | "path" | null
  >(null);

  useLayoutEffect(() => {
    const menu = menuRef.current;

    if (!menu || !item) {
      return;
    }

    const rect = menu.getBoundingClientRect();

    const padding = 10;

    const left = Math.max(
      padding,
      Math.min(
        x,
        window.innerWidth - rect.width - padding,
      ),
    );

    const top = Math.max(
      padding,
      Math.min(
        y,
        window.innerHeight - rect.height - padding,
      ),
    );

    setPosition({ left, top });
  }, [x, y, item]);

  useEffect(() => {
    if (!item) {
      return;
    }

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

  if (!item) {
    return null;
  }

  const status = item.status.toLowerCase();

  const destinationPath = item.destinationPath;

  const hasFile =
    Boolean(destinationPath) &&
    status === "completed";

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
    </div>
  );
}
