import type { DownloadListItem } from "../../types/download";
import { fileExtension, fileKind } from "../../utils/fileKind";

import "./FileBadge.css";

/** A coloured square with the file's extension: the fastest way to scan a list. */
export function FileBadge({ item, size = "md" }: { item: DownloadListItem; size?: "md" | "lg" }) {
  const extension = fileExtension(item);
  return (
    <span className={`file-badge file-badge--${fileKind(item)} file-badge--${size}`} aria-hidden="true">
      {(extension ?? "file").slice(0, 4).toUpperCase()}
    </span>
  );
}
