import { describe, expect, it } from "vitest";

import {
  isVideoPage,
  videoQualityOf,
  withVideoQuality,
  withVideoQualityForAll,
} from "./videoPages";

describe("video pages", () => {
  it("recognizes the same pages the engine sends to yt-dlp", () => {
    expect(isVideoPage("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).toBe(true);
    expect(isVideoPage("https://youtu.be/dQw4w9WgXcQ#rud-quality=720")).toBe(true);
    expect(isVideoPage("https://x.com/someone/status/1")).toBe(true);
    expect(isVideoPage("https://x.com/someone")).toBe(false);
    expect(isVideoPage("https://cdn.aparat.com/video/file.mp4")).toBe(false);
    expect(isVideoPage("https://example.com/watch?v=1")).toBe(false);
    expect(isVideoPage("not a link")).toBe(false);
  });

  it("keeps the chosen quality in the fragment", () => {
    const url = "https://youtu.be/x";
    expect(videoQualityOf(url)).toBeNull();
    expect(videoQualityOf(withVideoQuality(url, 720))).toBe(720);
    expect(videoQualityOf(withVideoQuality(`${url}#rud-quality=480`, "audio"))).toBe("audio");
    expect(withVideoQuality(url, "best")).toBe("https://youtu.be/x#rud-quality=best");
  });

  it("sets the quality of every video in a list and leaves files alone", () => {
    const text = "https://youtu.be/a\nhttps://example.com/file.zip\nhttps://youtu.be/b#rud-quality=360";
    expect(withVideoQualityForAll(text, 1080)).toBe(
      "https://youtu.be/a#rud-quality=1080\nhttps://example.com/file.zip\nhttps://youtu.be/b#rud-quality=1080",
    );
  });
});
