import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../context-menu-routing.js", import.meta.url), "utf8");
const context = { URL };
vm.runInNewContext(source, context);
const { imageDownloadTarget } = context.RatatoskrContextRouting;

test("image context uses the displayed image even when it is wrapped in a link", () => {
  assert.equal(imageDownloadTarget({
    mediaType: "image",
    srcUrl: "https://cdn.example/photo.webp?signature=abc",
    linkUrl: "https://example.com/gallery/photo"
  }), "https://cdn.example/photo.webp?signature=abc");
});

test("direct image links are sent to Ratatoskr's download confirmation", () => {
  assert.equal(imageDownloadTarget({ linkUrl: "https://cdn.example/photo.jpg?size=large" }),
    "https://cdn.example/photo.jpg?size=large");
});

test("non-image links remain ordinary downloads", () => {
  assert.equal(imageDownloadTarget({ linkUrl: "https://cdn.example/archive.zip" }), null);
  assert.equal(imageDownloadTarget({ linkUrl: "https://example.com/page" }), null);
});
