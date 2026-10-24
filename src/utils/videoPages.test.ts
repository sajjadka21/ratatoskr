import { describe, expect, it } from "vitest";

import {
  isVideoPage,
  videoQualityOf,
  withVideoQuality,
  withVideoQualityForAll,
  youtubePlaylistUrl,
} from "./videoPages";

describe("video pages", () => {
  it("offers the complete playlist without silently turning a video into a batch", () => {
    expect(youtubePlaylistUrl("https://www.youtube.com/watch?v=abc&list=PL_123&index=8")).toBe("https://www.youtube.com/playlist?list=PL_123");
    expect(youtubePlaylistUrl("https://youtu.be/abc?list=PL-123")).toBe("https://www.youtube.com/playlist?list=PL-123");
    expect(youtubePlaylistUrl("https://notyoutube.com/watch?list=PL123")).toBeNull();
    expect(youtubePlaylistUrl("https://www.youtube.com/watch?v=abc")).toBeNull();
  });
  it("recognizes the same pages the engine sends to yt-dlp", () => {
    expect(isVideoPage("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).toBe(true);
    expect(isVideoPage("https://youtu.be/dQw4w9WgXcQ#rud-quality=720")).toBe(true);
    expect(isVideoPage("https://x.com/someone/status/1")).toBe(true);
    expect(isVideoPage("https://www.pornhub.com/view_video.php?viewkey=6abbc213c20ff")).toBe(true);
    expect(isVideoPage("https://www.pinterest.com/pin/123456789/")).toBe(true);
    expect(isVideoPage("https://cdn.pornhub.com/video/file.mp4")).toBe(false);
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
