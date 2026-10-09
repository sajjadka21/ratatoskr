import { describe, expect, it } from "vitest";

import { likelySameFilename } from "./duplicateCheck";

describe("likelySameFilename", () => {
  it("recognizes the same episode when the server adds common release tags", () => {
    expect(likelySameFilename(
      "The.Mentalist.S04E01.720p.WEB-DL.x265.mkv",
      "The Mentalist S04E01.mkv",
    )).toBe(true);
  });

  it("keeps different episodes and file types separate", () => {
    expect(likelySameFilename("series.S04E01.mkv", "series.S04E02.mkv")).toBe(false);
    expect(likelySameFilename("series.S04E01.mkv", "series.S04E01.mp4")).toBe(false);
  });
});
