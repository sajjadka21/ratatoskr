import assert from "node:assert/strict";
import test from "node:test";
import { blobToDataUrl, MAX_TRANSFER_BYTES, outputName } from "../image-converter-core.mjs";

test("converts image bytes to a data URL without changing their contents", async () => {
  const bytes = Uint8Array.from({ length: 80_123 }, (_value, index) => index % 251);
  const dataUrl = await blobToDataUrl(new Blob([bytes], { type: "image/webp" }));
  const [header, base64] = dataUrl.split(",", 2);

  assert.equal(header, "data:image/webp;base64");
  assert.deepEqual(Buffer.from(base64, "base64"), Buffer.from(bytes));
});

test("refuses results too large for safe extension messaging", async () => {
  const blob = { size: MAX_TRANSFER_BYTES + 1 };
  await assert.rejects(() => blobToDataUrl(blob), /too large to transfer safely/);
});

test("builds a safe filename with the selected output extension", () => {
  assert.equal(outputName("https://cdn.example/a%20photo.webp?token=secret", "jpg"), "a photo.jpg");
  assert.equal(outputName("https://cdn.example/", "png"), "image.png");
  assert.equal(outputName("https://cdn.example/a%2F..%2Fbad.webp", "png"), "a_.._bad.png");
});
