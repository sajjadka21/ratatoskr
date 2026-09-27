import type { DownloadListItem } from "../../types/download";
import { fileExtension, fileKind, kindOfExtension } from "../../utils/fileKind";

import "./FileBadge.css";

/**
 * A coloured square with the file's extension: the fastest way to scan a
 * list. Takes a download, or just a file name before there is one.
 */
export function FileBadge({
  item,
  name,
  size = "md",
}: {
  item?: DownloadListItem;
  name?: string;
  size?: "md" | "lg";
}) {
  const fromName = /\.([a-z0-9]{1,5})$/i.exec(name ?? "")?.[1]?.toLowerCase() ?? null;
  const extension = item ? fileExtension(item) : fromName;
  const kind = item ? fileKind(item) : kindOfExtension(fromName ?? "");
  return (
    <span className={`file-badge file-badge--${kind} file-badge--${size}`} aria-hidden="true">
      {(extension ?? "file").slice(0, 4).toUpperCase()}
    </span>
  );
}
