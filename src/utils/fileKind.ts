import type { DownloadListItem } from "../types/download";

export type FileKind = "video" | "audio" | "image" | "archive" | "app" | "document" | "disk" | "other";

const EXTENSION_KIND: Record<string, FileKind> = {
  mp4: "video", mkv: "video", mov: "video", avi: "video", webm: "video", m4v: "video", ts: "video",
  mp3: "audio", flac: "audio", wav: "audio", aac: "audio", m4a: "audio", ogg: "audio", opus: "audio",
  png: "image", jpg: "image", jpeg: "image", gif: "image", webp: "image", svg: "image", avif: "image",
  zip: "archive", rar: "archive", "7z": "archive", tar: "archive", gz: "archive", bz2: "archive", xz: "archive", zst: "archive",
  exe: "app", msi: "app", msix: "app", appx: "app", apk: "app", dmg: "app", deb: "app", rpm: "app",
  pdf: "document", doc: "document", docx: "document", xls: "document", xlsx: "document", ppt: "document",
  pptx: "document", txt: "document", csv: "document", epub: "document",
  iso: "disk", img: "disk", vhd: "disk", vhdx: "disk",
};

/** Short upper-case extension for the badge, or null when there is none. */
export function fileExtension(item: DownloadListItem): string | null {
  const name = item.filename ?? safePath(item.resolvedUrl ?? item.sourceUrl);
  const match = /\.([a-z0-9]{1,5})$/i.exec(name ?? "");
  return match ? match[1].toLowerCase() : null;
}

export function fileKind(item: DownloadListItem): FileKind {
  const mime = item.mimeType?.toLowerCase() ?? "";
  if (mime.startsWith("video/")) return "video";
  if (mime.startsWith("audio/")) return "audio";
  if (mime.startsWith("image/")) return "image";
  const extension = fileExtension(item);
  return (extension && EXTENSION_KIND[extension]) || "other";
}

export function displayName(item: DownloadListItem): string {
  return item.filename ?? safePath(item.resolvedUrl ?? item.sourceUrl) ?? item.sourceUrl;
}

function safePath(url: string): string | null {
  try {
    const last = new URL(url).pathname.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : null;
  } catch {
    return null;
  }
}
