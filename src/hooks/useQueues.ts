import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  DownloadListItem,
  DownloadPriority,
  DownloadQueue,
} from "../types/download";

const QUEUE_RUNNER_EVENT = "queue-runner-event";

type QueueRunnerEvent = {
  kind: "queueUpdated" | "taskProgress" | "taskUpdated";
  queueId: string;
  queue: DownloadQueue | null;
  download: DownloadListItem | null;
  downloadId: string | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
};

type UseQueuesOptions = {
  upsertDownloads: (records: DownloadListItem[]) => void;
  updateDownloadProgress: (
    downloadId: string,
    downloadedBytes: number,
    totalBytes: number | null,
    bytesPerSecond?: number | null,
    etaSeconds?: number | null,
  ) => void;
  clearLiveMetrics: (downloadId: string) => void;
  refreshDownloads: () => Promise<void>;
};

export function useQueues({
  upsertDownloads,
  updateDownloadProgress,
  clearLiveMetrics,
  refreshDownloads,
}: UseQueuesOptions) {
  const [queues, setQueues] = useState<DownloadQueue[]>([]);

  const upsertQueues = useCallback((records: DownloadQueue[]) => {
    setQueues((current) => {
      const next = new Map(current.map((queue) => [queue.id, queue]));
      for (const record of records) next.set(record.id, record);
      return [...next.values()].sort(
        (left, right) =>
          left.sortOrder - right.sortOrder || left.name.localeCompare(right.name),
      );
    });
  }, []);

  const refreshQueues = useCallback(async () => {
    setQueues(await invoke<DownloadQueue[]>("list_queues"));
  }, []);

  // Runner events are published to the window, so a queue this session never
  // started - one resumed when the app launched - reports its progress too.
  useEffect(() => {
    const subscription = listen<QueueRunnerEvent>(
      QUEUE_RUNNER_EVENT,
      ({ payload }) => {
        if (payload.kind === "queueUpdated" && payload.queue) {
          upsertQueues([payload.queue]);
          return;
        }

        if (payload.kind === "taskUpdated" && payload.download) {
          clearLiveMetrics(payload.download.id);
          upsertDownloads([payload.download]);
          return;
        }

        if (
          payload.kind === "taskProgress" &&
          payload.downloadId &&
          payload.downloadedBytes !== null
        ) {
          updateDownloadProgress(
            payload.downloadId,
            payload.downloadedBytes,
            payload.totalBytes,
            payload.bytesPerSecond,
            payload.etaSeconds,
          );
        }
      },
    );

    return () => {
      void subscription.then((unlisten) => unlisten());
    };
  }, [
    upsertQueues,
    upsertDownloads,
    updateDownloadProgress,
    clearLiveMetrics,
  ]);

  async function createQueue(input: {
    name: string;
    maxConcurrent: number;
    maxConcurrentPerHost: number | null;
    defaultPriority: DownloadPriority;
  }) {
    const queue = await invoke<DownloadQueue>("create_queue", input);
    upsertQueues([queue]);
    return queue;
  }

  async function startQueue(queueId: string) {
    upsertQueues([
      await invoke<DownloadQueue>("start_queue", { queueId }),
    ]);
  }

  async function setQueueEnabled(queueId: string, enabled: boolean) {
    upsertQueues([
      await invoke<DownloadQueue>("set_queue_enabled", {
        queueId,
        enabled,
      }),
    ]);
  }

  async function stopQueue(queueId: string) {
    const queue = await invoke<DownloadQueue>("stop_queue", { queueId });
    upsertQueues([queue]);
  }

  async function reorderQueue(queueId: string, orderedIds: string[]) {
    await invoke<void>("reorder_queue_downloads", { queueId, orderedIds });
    await refreshDownloads();
  }

  async function moveQueuedDownload(downloadId: string, queueId: string) {
    const record = await invoke<DownloadListItem>("move_queued_download", {
      id: downloadId,
      queueId,
    });
    upsertDownloads([record]);
  }

  async function removeFromQueue(downloadId: string) {
    const record = await invoke<DownloadListItem>("remove_download_from_queue", {
      id: downloadId,
    });
    upsertDownloads([record]);
  }

  async function changePriority(
    downloadId: string,
    priority: DownloadPriority,
  ) {
    const record = await invoke<DownloadListItem>("set_download_priority", {
      id: downloadId,
      priority,
    });
    upsertDownloads([record]);
  }

  return {
    queues,
    refreshQueues,
    createQueue,
    startQueue,
    stopQueue,
    setQueueEnabled,
    reorderQueue,
    moveQueuedDownload,
    removeFromQueue,
    changePriority,
  };
}
