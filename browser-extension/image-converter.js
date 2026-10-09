import { blobToDataUrl, FORMATS, outputName } from "./image-converter-core.mjs";

const MAX_IMAGE_BYTES = 40 * 1024 * 1024;
const MAX_IMAGE_PIXELS = 20_000_000;

async function convertAndPrepare(sourceUrl, formatName) {
  const format = FORMATS[formatName];
  if (!format) throw new Error("unsupported output format");
  const response = await fetch(sourceUrl, { credentials: "include", cache: "no-store" });
  if (!response.ok) throw new Error(`image request failed (${response.status})`);
  const declaredSize = Number(response.headers.get("content-length") || 0);
  if (declaredSize > MAX_IMAGE_BYTES) throw new Error("image is too large to convert safely");
  const source = await response.blob();
  if (source.size > MAX_IMAGE_BYTES) throw new Error("image is too large to convert safely");

  const bitmap = await createImageBitmap(source);
  try {
    if (bitmap.width * bitmap.height > MAX_IMAGE_PIXELS) throw new Error("image dimensions are too large to convert safely");
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d", { alpha: formatName !== "jpeg" });
    if (!context) throw new Error("image canvas is unavailable");
    if (formatName === "jpeg") {
      context.fillStyle = "#fff";
      context.fillRect(0, 0, canvas.width, canvas.height);
    }
    context.drawImage(bitmap, 0, 0);
    const converted = await canvas.convertToBlob({ type: format.mime, quality: formatName === "png" ? undefined : 0.92 });
    if (converted.type !== format.mime) throw new Error(`this browser cannot encode ${formatName.toUpperCase()}`);
    const dataUrl = await blobToDataUrl(converted);
    return { dataUrl, filename: outputName(sourceUrl, format.extension) };
  } finally {
    bitmap.close();
  }
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type === "release-image-object-url") {
    URL.revokeObjectURL(message.objectUrl);
    return false;
  }
  if (message?.type !== "convert-image") return false;
  convertAndPrepare(message.sourceUrl, message.format)
    .then((result) => sendResponse({ ok: true, ...result }))
    .catch((error) => sendResponse({ ok: false, error: String(error?.message ?? error) }));
  return true;
});
