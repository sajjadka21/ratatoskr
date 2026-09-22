import { describe, expect, it } from "vitest";

import type { DownloadListItem } from "../types/download";
import { availableActions, primaryAction } from "./taskActions";

function item(status: string): DownloadListItem {
  return {
    id: "task-1",
    sourceUrl: "https://example.com/file.bin",
    resolvedUrl: null,
    filename: null,
    destinationPath: null,
    mimeType: null,
    etag: null,
    lastModified: null,
    rangeSupported: null,
    totalBytes: null,
    downloadedBytes: 0,
    status,
    queueId: null,
    priority: "normal",
    queuePosition: null,
    createdAt: 0,
    startedAt: null,
    completedAt: null,
    attempts: 0,
    retryAt: null,
    errorCode: null,
    errorMessage: null,
  };
}

describe("task actions", () => {
  it("offers restart for paused and failed tasks but not final completed history", () => {
    expect(availableActions(item("paused"))).toContain("restart");
    expect(availableActions(item("failed"))).toContain("restart");
    expect(availableActions(item("completed"))).not.toContain("restart");
  });

  it("keeps one predictable primary action per row", () => {
    expect(primaryAction(item("downloading"))).toBe("pause");
    expect(primaryAction(item("failed"))).toBe("retry");
    expect(primaryAction(item("completed"))).toBeNull();
  });
});
