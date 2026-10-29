import { useCallback, useEffect, useMemo, useState } from "react";
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
  activeConnections: number | null;
  maxConnections: number | null;
  adaptiveReason: string | null;
};

type UseQueuesOptions = {
  upsertDownloads: (records: DownloadListItem[]) => void;
  updateDownloadProgress: (
    downloadId: string,
    downloadedBytes: number,
    totalBytes: number | null,
    bytesPerSecond?: number | null,
    etaSeconds?: number | null,
    activeConnections?: number | null,
    maxConnections?: number | null,
    adaptiveReason?: string | null,
  ) => void;
  clearLiveMetrics: (downloadId: string) => void;
  refreshDownloads: () => Promise<void>;
  /** A row a queue changed: handled like any other engine update. */
  onTaskUpdated: (record: DownloadListItem) => void;
  /** The built-in queue's name in the interface language. */
  defaultQueueName?: string;
};

/** The name the backend gives the built-in queue until the user renames it. */
export const STORED_DEFAULT_QUEUE_NAME = "Default Queue";

/** Shows the untouched built-in queue in the interface language; stored names never change. */
export function localizeQueue(queue: DownloadQueue, defaultQueueName: string | undefined): DownloadQueue {
  return defaultQueueName && queue.id === "default" && queue.name === STORED_DEFAULT_QUEUE_NAME
    ? { ...queue, name: defaultQueueName }
    : queue;
}

export function useQueues({
  upsertDownloads,
  updateDownloadProgress,
  clearLiveMetrics,
  refreshDownloads,
  onTaskUpdated,
  defaultQueueName,
}: UseQueuesOptions) {
  const [storedQueues, setQueues] = useState<DownloadQueue[]>([]);
  const queues = useMemo(
    () => storedQueues.map((queue) => localizeQueue(queue, defaultQueueName)),
    [storedQueues, defaultQueueName],
  );

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
          onTaskUpdated(payload.download);
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
            payload.activeConnections,
            payload.maxConnections,
            payload.adaptiveReason,
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
    onTaskUpdated,
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

  async function setQueueLimits(queueId: string, maxConcurrent: number, maxConcurrentPerHost: number | null) {
    upsertQueues([await invoke<DownloadQueue>("set_queue_limits", { queueId, maxConcurrent, maxConcurrentPerHost })]);
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
    setQueueLimits,
    reorderQueue,
    moveQueuedDownload,
    removeFromQueue,
    changePriority,
  };
}
