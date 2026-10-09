export const FORMATS = {
  png: { mime: "image/png", extension: "png" },
  jpeg: { mime: "image/jpeg", extension: "jpg" },
  webp: { mime: "image/webp", extension: "webp" }
};

// Chrome serializes extension messages as JSON and limits them to 64 MiB.
// Keeping the binary result below 36 MiB leaves room for base64 expansion.
export const MAX_TRANSFER_BYTES = 36 * 1024 * 1024;
export const MAX_TRANSFER_MESSAGE_CHARS = 48 * 1024 * 1024;

export function outputName(sourceUrl, extension) {
  let name = "image";
  try {
    const pathname = new URL(sourceUrl).pathname;
    name = decodeURIComponent(pathname.slice(pathname.lastIndexOf("/") + 1)) || name;
  } catch { /* Keep the safe fallback. */ }
  name = name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").replace(/[. ]+$/g, "");
  const dot = name.lastIndexOf(".");
  if (dot > 0) name = name.slice(0, dot);
  return `${(name || "image").slice(0, 160)}.${extension}`;
}

export async function blobToDataUrl(blob) {
  if (blob.size > MAX_TRANSFER_BYTES) throw new Error("converted image is too large to transfer safely");

  const bytes = new Uint8Array(await blob.arrayBuffer());
  const chunkSize = 3 * 16_384;
  const chunks = [];
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    const end = Math.min(offset + chunkSize, bytes.length);
    let binary = "";
    for (let index = offset; index < end; index += 1) binary += String.fromCharCode(bytes[index]);
    chunks.push(btoa(binary));
  }

  const dataUrl = `data:${blob.type || "application/octet-stream"};base64,${chunks.join("")}`;
  if (dataUrl.length > MAX_TRANSFER_MESSAGE_CHARS) throw new Error("converted image is too large to transfer safely");
  return dataUrl;
}
