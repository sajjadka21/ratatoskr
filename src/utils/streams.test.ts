import { describe, expect, it } from "vitest";

import { createFormatter } from "../i18n/format";
import { heightLabel, isStreamLink, variantLabel } from "./streams";

describe("streams", () => {
  it("recognises playlist links only by their path", () => {
    expect(isStreamLink("https://cdn.example.com/show/master.m3u8")).toBe(true);
    expect(isStreamLink("https://cdn.example.com/show/master.M3U8?token=a")).toBe(true);
    expect(isStreamLink("https://cdn.example.com/video.mp4?format=m3u8")).toBe(false);
    expect(isStreamLink("ftp://example.com/a.m3u8")).toBe(false);
    expect(isStreamLink("not a link")).toBe(false);
  });

  it("writes heights without a thousands separator", () => {
    expect(heightLabel(1080, "en")).toBe("1080p");
    expect(heightLabel(1080, "fa")).toBe("۱۰۸۰p");
  });

  it("describes a quality by height and bitrate", () => {
    const fmt = createFormatter("en");
    expect(
      variantLabel({ uri: "u", height: 720, width: 1280, bandwidth: 2_400_000, needsMuxing: false }, fmt),
    ).toBe("720p · 2.4 Mb/s");
    expect(
      variantLabel({ uri: "u", height: null, width: null, bandwidth: null, needsMuxing: false }, fmt),
    ).toBe("—");
  });
});
