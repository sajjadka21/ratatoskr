/** Accountless handoff. The QR is generated locally and carries only a public URL. */
export function mobileHandoffUrl(source: string): string | null {
  if (!source || /[\u0000-\u0020\u007f]/.test(source) || /%(?![\da-f]{2})/i.test(source)) return null;
  const authority = source.match(/^https?:\/\/([^/?#]+)/i)?.[1];
  if (!authority || authority.includes("@")) return null;
  const rawHost = authority.startsWith("[") ? authority.split("]")[0]!.slice(1) : authority.split(":")[0]!;
  if (/^[\d.]+$/.test(rawHost) && !/^(?:0|[1-9]\d{0,2})(?:\.(?:0|[1-9]\d{0,2})){3}$/.test(rawHost)) return null;
  if (/^0x/i.test(rawHost)) return null;
  try {
    const url = new URL(source);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) return null;
    const host = url.hostname.toLowerCase().replace(/\.$/, "");
    if (host === "localhost" || /\.(local|internal|localhost|lan)$/.test(host)) return null;
    if (host.startsWith("[")) {
      const ip = host.slice(1, -1);
      if (ip === "::" || ip === "::1" || /^(fc|fd|fe[89ab]|ff)/.test(ip)) return null;
      // Mapped IPv4 literals use the same policy as ordinary IPv4.
      if (ip.startsWith("::ffff:")) {
        const parts = ip.slice(7).split(":");
        if (parts.length !== 2) return null;
        const value = parts.map(p => parseInt(p, 16));
        if (!publicV4([value[0]! >> 8, value[0]! & 255, value[1]! >> 8, value[1]! & 255])) return null;
      }
    } else if (/^[\d.]+$/.test(host)) {
      if (!publicV4(host.split(".").map(Number))) return null;
    } else if (!host.includes(".")) return null;
    for (const key of url.searchParams.keys()) {
      if (["access_token", "authorization", "password", "sessionid", "cookie"].includes(key.toLowerCase())) return null;
    }
    url.hash = "";
    const payload = "ratatoskr://add?url=" + encodeURIComponent(url.href);
    return new TextEncoder().encode(payload).length <= 2000 ? payload : null;
  } catch { return null; }
}

function publicV4(parts: number[]): boolean {
  if (parts.length !== 4 || parts.some(n => !Number.isInteger(n) || n < 0 || n > 255)) return false;
  const [a, b] = parts;
  return !(a === 0 || a === 10 || a === 127 || a! >= 224 ||
    (a === 169 && b === 254) || (a === 172 && b! >= 16 && b! <= 31) ||
    (a === 192 && b === 168) || (a === 100 && b! >= 64 && b! <= 127) ||
    (a === 198 && (b === 18 || b === 19)));
}
