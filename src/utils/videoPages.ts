/**
 * Video pages (YouTube and similar sites) that are downloaded with yt-dlp.
 * Mirrors `handles` in crates/dm-core/src/ytdlp.rs.
 */
const SITES: [string, string[]][] = [
  ["youtube.com", []],
  ["youtu.be", []],
  ["youtube-nocookie.com", []],
  ["vimeo.com", []],
  ["dailymotion.com", []],
  ["twitch.tv", []],
  ["pornhub.com", []],
  ["pinterest.com", ["/pin/"]],
  ["x.com", ["/status/"]],
  ["twitter.com", ["/status/"]],
  ["instagram.com", ["/p/", "/reel/", "/reels/", "/tv/"]],
  ["facebook.com", ["/videos/", "/watch", "/reel/", "/share/v/", "/share/r/"]],
  ["fb.watch", []],
  ["tiktok.com", ["/video/", "/t/"]],
  ["vm.tiktok.com", []],
  ["reddit.com", ["/comments/"]],
  ["v.redd.it", []],
  ["soundcloud.com", []],
  ["aparat.com", ["/v/"]],
  ["bilibili.com", ["/video/"]],
];

const FILE_EXTENSIONS = new Set([
  "mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a", "aac", "ogg", "opus", "flac", "wav", "m3u8",
  "mpd", "ts", "zip", "rar", "7z", "exe", "msi", "iso", "pdf", "jpg", "jpeg", "png", "gif",
]);

export function isVideoPage(url: string): boolean {
  let parsed: URL;
  try {
    parsed = new URL(url.trim());
  } catch {
    return false;
  }
  if (!/^https?:$/.test(parsed.protocol)) return false;
  const host = parsed.hostname.toLowerCase();
  const path = parsed.pathname.toLowerCase();
  const known = SITES.some(
    ([site, paths]) =>
      (host === site || host.endsWith(`.${site}`)) &&
      (paths.length === 0 || paths.some((part) => path.includes(part))),
  );
  if (!known) return false;
  const last = path.split("/").filter(Boolean).pop() ?? "";
  const dot = last.lastIndexOf(".");
  return dot < 0 || !FILE_EXTENSIONS.has(last.slice(dot + 1));
}

/** A height such as 720, the best there is, or the sound alone. */
export type VideoQuality = number | "best" | "audio";

/** The quality chosen in a link, or null when the setting applies. */
export function videoQualityOf(url: string): VideoQuality | null {
  const fragment = url.trim().split("#")[1];
  if (!fragment) return null;
  const value = fragment
    .split("&")
    .find((pair) => pair.startsWith("rud-quality="))
    ?.slice("rud-quality=".length);
  if (value === "best" || value === "audio") return value;
  const height = Number(value);
  return Number.isInteger(height) && height > 0 ? height : null;
}

/**
 * The link with a quality attached. The engine reads it from the fragment,
 * which is never sent to the site.
 */
export function withVideoQuality(url: string, quality: VideoQuality): string {
  const base = url.trim().split("#")[0];
  return `${base}#rud-quality=${quality}`;
}

/**
 * Every video page in a list of links (one per line) set to `quality`;
 * other lines are left as they are.
 */
export function withVideoQualityForAll(text: string, quality: VideoQuality): string {
  return text
    .split(/\r?\n/)
    .map((line) => (isVideoPage(line.trim().split("#")[0]) ? withVideoQuality(line, quality) : line))
    .join("\n");
}
