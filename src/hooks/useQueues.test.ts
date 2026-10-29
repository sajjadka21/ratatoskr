import { expect, it } from "vitest";

import type { DownloadQueue } from "../types/download";
import { localizeQueue } from "./useQueues";

const queue: DownloadQueue = {
  id: "default", name: "Default Queue", enabled: true, state: "running", sortOrder: 0,
  maxConcurrent: 3, maxConcurrentPerHost: null, defaultPriority: "normal", createdAt: 0, updatedAt: 0,
};

it("names the untouched built-in queue in the interface language and leaves every other name alone", () => {
  expect(localizeQueue(queue, "صف پیش‌فرض").name).toBe("صف پیش‌فرض");
  expect(localizeQueue({ ...queue, name: "My downloads" }, "صف پیش‌فرض").name).toBe("My downloads");
  expect(localizeQueue({ ...queue, id: "night" }, "صف پیش‌فرض").name).toBe("Default Queue");
  expect(localizeQueue(queue, undefined).name).toBe("Default Queue");
});
