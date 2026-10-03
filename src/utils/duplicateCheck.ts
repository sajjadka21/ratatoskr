import type { DownloadListItem } from "../types/download";
import { extractHttpUrls } from "./downloadLinks";

const ignored = new Set(["web", "dl", "webdl", "webrip", "bluray", "bdrip", "h264", "h265", "x264", "x265", "hevc", "avc", "aac", "proper", "repack"]);

function filenameFromUrl(value: string): string | null {
  try {
    const parts = new URL(value).pathname.split("/").filter(Boolean);
    const name = decodeURIComponent(parts[parts.length - 1] ?? "");
    return /\.[a-z0-9]{2,8}$/i.test(name) ? name : null;
  } catch {
    return null;
  }
}

function tokens(value: string): { words: string[]; extension: string; numbers: string[] } {
  const name = value.normalize("NFKC").replace(/\s*\(\d+\)(?=\.[^.]+$|$)/, "").toLocaleLowerCase();
  const extension = name.match(/\.([a-z0-9]{2,8})$/)?.[1] ?? "";
  const stem = extension ? name.slice(0, -(extension.length + 1)) : name;
  const words = stem.split(/[^\p{L}\p{N}]+/u).filter((part) => part && !ignored.has(part) && !/^\d{3,4}p$/.test(part));
  return { words, extension, numbers: words.filter((part) => /^\d+$/.test(part)) };
}

/** Conservative title similarity for a direct file URL; it only raises an informational duplicate warning. */
export function likelySameFilename(left: string, right: string): boolean {
  const a = tokens(left);
  const b = tokens(right);
  if (!a.extension || a.extension !== b.extension || !a.words.length || !b.words.length) return false;
  if (a.numbers.join("|") !== b.numbers.join("|")) return false;
  const rows = Array.from({ length: a.words.length + 1 }, () => Array<number>(b.words.length + 1).fill(0));
  for (let i = 1; i <= a.words.length; i++) {
    for (let j = 1; j <= b.words.length; j++) {
      rows[i]![j] = a.words[i - 1] === b.words[j - 1]
        ? rows[i - 1]![j - 1]! + 1
        : Math.max(rows[i - 1]![j]!, rows[i]![j - 1]!);
    }
  }
  return rows[a.words.length]![b.words.length]! / Math.max(a.words.length, b.words.length) >= 0.8;
}

export function findLikelyFilenameDuplicates(text: string, downloads: DownloadListItem[]): Map<string, DownloadListItem[]> {
  const matches = new Map<string, DownloadListItem[]>();
  for (const link of extractHttpUrls(text)) {
    const candidate = filenameFromUrl(link);
    if (!candidate) continue;
    const found = downloads.filter((download) => download.filename && likelySameFilename(candidate, download.filename));
    if (found.length) matches.set(link.split("#")[0]!, found);
  }
  return matches;
}
