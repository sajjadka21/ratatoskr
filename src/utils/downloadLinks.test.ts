import { describe, expect, it } from "vitest";

import { extractHttpUrls } from "./downloadLinks";

describe("extractHttpUrls", () => {
  it("finds links inside surrounding text", () => {
    expect(
      extractHttpUrls(
        "Grab it from https://example.com/file.bin before tomorrow.",
      ),
    ).toEqual(["https://example.com/file.bin"]);
  });

  it("deduplicates the same link", () => {
    expect(
      extractHttpUrls(
        "https://example.com/a.bin\nhttps://example.com/a.bin",
      ),
    ).toEqual(["https://example.com/a.bin"]);
  });

  it("keeps several distinct links in order", () => {
    expect(
      extractHttpUrls(
        "https://example.com/a.bin and http://example.org/b.bin",
      ),
    ).toEqual([
      "https://example.com/a.bin",
      "http://example.org/b.bin",
    ]);
  });

  it("ignores schemes the engine does not accept", () => {
    expect(
      extractHttpUrls("file:///c:/secret.bin ftp://example.com/x.bin"),
    ).toEqual([]);
  });

  it("drops trailing punctuation that belongs to the sentence", () => {
    expect(extractHttpUrls("(see https://example.com/a.bin).")).toEqual([
      "https://example.com/a.bin",
    ]);
  });

  it("returns nothing for text without links", () => {
    expect(extractHttpUrls("no links here")).toEqual([]);
    expect(extractHttpUrls("")).toEqual([]);
  });
});
