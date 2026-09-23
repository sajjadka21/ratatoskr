import { describe, expect, it } from "vitest";

import {
  LARGE_BATCH,
  MAX_DROPPED_FILE_BYTES,
  batchAction,
  readDroppedText,
} from "./linkgrabber";

describe("batchAction", () => {
  it("uses the queue the user chose", () => {
    expect(batchAction(500, "night", "default").action).toEqual({
      kind: "queue",
      queueId: "night",
    });
  });

  it("starts a small selection directly", () => {
    const result = batchAction(LARGE_BATCH, null, "default");

    expect(result.action).toEqual({ kind: "start-now" });
    expect(result.note).toBeNull();
  });

  it("routes a large selection through the default queue and says so", () => {
    const result = batchAction(LARGE_BATCH + 1, null, "default");

    expect(result.action).toEqual({ kind: "queue", queueId: "default" });
    expect(result.note).toContain("Default Queue");
    expect(result.label).toContain(String(LARGE_BATCH + 1));
  });

  it("still starts when there is no queue to route through", () => {
    expect(batchAction(LARGE_BATCH + 1, null, null).action).toEqual({
      kind: "start-now",
    });
  });
});

function file(name: string, content: string, type = "", size = content.length) {
  return { name, type, size, text: async () => content };
}

function dropped(files: ReturnType<typeof file>[], data: Record<string, string> = {}) {
  return { files, getData: (format: string) => data[format] ?? "" };
}

describe("readDroppedText", () => {
  it("reads every dropped text file", async () => {
    await expect(
      readDroppedText(
        dropped([
          file("links.txt", "https://a.test/1.zip"),
          file("page.html", '<a href="https://a.test/2.zip">2</a>', "text/html"),
        ]),
      ),
    ).resolves.toBe('https://a.test/1.zip\n<a href="https://a.test/2.zip">2</a>');
  });

  it("refuses files that are not text", async () => {
    await expect(
      readDroppedText(dropped([file("movie.mkv", "binary", "video/x-matroska")])),
    ).rejects.toThrow("not a text file");
  });

  it("refuses text files too large to be a link list", async () => {
    await expect(
      readDroppedText(dropped([file("huge.txt", "x", "text/plain", MAX_DROPPED_FILE_BYTES + 1)])),
    ).rejects.toThrow("larger than 2 MB");
  });

  it("uses a dragged link when no file was dropped", async () => {
    await expect(
      readDroppedText(dropped([], { "text/uri-list": "https://a.test/f.zip" })),
    ).resolves.toBe("https://a.test/f.zip");
  });
});
