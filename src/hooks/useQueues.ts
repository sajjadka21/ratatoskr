import { useCallback, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  DownloadListItem,
  DownloadPriority,
  DownloadQueue,
} from "../types/download";

type QueueRunnerEvent = {
  kind: "queueUpdated" | "taskProgress" | "taskUpdated";
  queue: DownloadQueue | null;
  download: DownloadListItem | null;
  downloadId: string | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
};

type UseQueuesOptions = {
  upsertDownloads: (records: DownloadListItem[]) => void;
  updateDownloadProgress: (
    downloadId: string,
    downloadedBytes: number,
    totalBytes: number | null,
  ) => void;
  refreshDownloads: () => Promise<void>;
};

export function useQueues({
  upsertDownloads,
  updateDownloadProgress,
  refreshDownloads,
}: UseQueuesOptions) {
  const [queues, setQueues] = useState<DownloadQueue[]>([]);
  const queueChannels = useRef(
    new Map<string, Channel<QueueRunnerEvent>>(),
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

  function applyQueueEvent(event: QueueRunnerEvent) {
    if (event.kind === "queueUpdated" && event.queue) {
      upsertQueues([event.queue]);
      if (event.queue.state === "stopped") {
        queueChannels.current.delete(event.queue.id);
      }
      return;
    }

    if (event.kind === "taskUpdated" && event.download) {
      upsertDownloads([event.download]);
      return;
    }

    if (
      event.kind === "taskProgress" &&
      event.downloadId &&
      event.downloadedBytes !== null
    ) {
      updateDownloadProgress(
        event.downloadId,
        event.downloadedBytes,
        event.totalBytes,
      );
    }
  }

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
    const onEvent = new Channel<QueueRunnerEvent>();
    onEvent.onmessage = applyQueueEvent;
    queueChannels.current.set(queueId, onEvent);
    try {
      const queue = await invoke<DownloadQueue>("start_queue", {
        queueId,
        onEvent,
      });
      upsertQueues([queue]);
    } catch (reason) {
      queueChannels.current.delete(queueId);
      throw reason;
    }
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
    reorderQueue,
    moveQueuedDownload,
    removeFromQueue,
    changePriority,
  };
}
