import { useEffect, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronsDown,
  ChevronsUp,
  CircleStop,
  GripVertical,
  Layers3,
  Play,
  Plus,
  Unlink,
} from "lucide-react";

import type {
  DownloadListItem,
  DownloadPriority,
  DownloadQueue,
} from "../../types/download";

import "./QueuePage.css";

type QueuePageProps = {
  queues: DownloadQueue[];
  downloads: DownloadListItem[];
  onCreateQueue: (input: {
    name: string;
    maxConcurrent: number;
    maxConcurrentPerHost: number | null;
    defaultPriority: DownloadPriority;
  }) => Promise<DownloadQueue>;
  onStartQueue: (queueId: string) => Promise<void>;
  onStopQueue: (queueId: string) => Promise<void>;
  onSetQueueEnabled: (
    queueId: string,
    enabled: boolean,
  ) => Promise<void>;
  onReorder: (queueId: string, orderedIds: string[]) => Promise<void>;
  onMove: (downloadId: string, queueId: string) => Promise<void>;
  onRemove: (downloadId: string) => Promise<void>;
  onPriority: (
    downloadId: string,
    priority: DownloadPriority,
  ) => Promise<void>;
};

const priorities: DownloadPriority[] = [
  "very_high",
  "high",
  "normal",
  "low",
];

const priorityRank: Record<DownloadPriority, number> = {
  very_high: 0,
  high: 1,
  normal: 2,
  low: 3,
};

export function QueuePage({
  queues,
  downloads,
  onCreateQueue,
  onStartQueue,
  onStopQueue,
  onSetQueueEnabled,
  onReorder,
  onMove,
  onRemove,
  onPriority,
}: QueuePageProps) {
  const [selectedQueueId, setSelectedQueueId] = useState("default");
  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [maxConcurrent, setMaxConcurrent] = useState(3);
  const [perHost, setPerHost] = useState(2);
  const [defaultPriority, setDefaultPriority] =
    useState<DownloadPriority>("normal");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draggedId, setDraggedId] = useState<string | null>(null);

  useEffect(() => {
    if (queues.some((queue) => queue.id === selectedQueueId)) return;
    setSelectedQueueId(queues[0]?.id ?? "");
  }, [queues, selectedQueueId]);

  const selectedQueue =
    queues.find((queue) => queue.id === selectedQueueId) ?? null;

  const queueDownloads = useMemo(
    () =>
      downloads
        .filter(
          (download) =>
            download.queueId === selectedQueueId &&
            download.status.toLowerCase() === "queued",
        )
        .sort(
          (left, right) =>
            priorityRank[left.priority] - priorityRank[right.priority] ||
            (left.queuePosition ?? Number.MAX_SAFE_INTEGER) -
              (right.queuePosition ?? Number.MAX_SAFE_INTEGER) ||
            left.id.localeCompare(right.id),
        ),
    [downloads, selectedQueueId],
  );

  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function createQueue() {
    const queueName = name.trim();
    if (!queueName || maxConcurrent < 1 || perHost < 1) return;
    await run(async () => {
      const queue = await onCreateQueue({
        name: queueName,
        maxConcurrent,
        maxConcurrentPerHost: perHost,
        defaultPriority,
      });
      setSelectedQueueId(queue.id);
      setName("");
      setCreateOpen(false);
    });
  }

  async function moveInOrder(downloadId: string, targetIndex: number) {
    const currentIndex = queueDownloads.findIndex(
      (download) => download.id === downloadId,
    );
    if (currentIndex < 0) return;
    const boundedTarget = Math.max(
      0,
      Math.min(targetIndex, queueDownloads.length - 1),
    );
    if (currentIndex === boundedTarget) return;
    const ordered = queueDownloads.map((download) => download.id);
    ordered.splice(currentIndex, 1);
    ordered.splice(boundedTarget, 0, downloadId);
    await run(() => onReorder(selectedQueueId, ordered));
  }

  return (
    <div className="queue-page">
      <aside className="queue-page__rail">
        <div className="queue-page__rail-heading">
          <div>
            <span>Queue library</span>
            <strong>{queues.length} queues</strong>
          </div>
          <button
            type="button"
            aria-label="Create queue"
            onClick={() => setCreateOpen((current) => !current)}
          >
            <Plus size={15} />
          </button>
        </div>

        {createOpen ? (
          <div className="queue-page__create">
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Queue name"
            />
            <div className="queue-page__create-grid">
              <label>
                Concurrent
                <input
                  type="number"
                  min={1}
                  value={maxConcurrent}
                  onChange={(event) =>
                    setMaxConcurrent(Number(event.target.value))
                  }
                />
              </label>
              <label>
                Per host
                <input
                  type="number"
                  min={1}
                  value={perHost}
                  onChange={(event) => setPerHost(Number(event.target.value))}
                />
              </label>
            </div>
            <select
              value={defaultPriority}
              onChange={(event) =>
                setDefaultPriority(event.target.value as DownloadPriority)
              }
            >
              {priorities.map((priority) => (
                <option key={priority} value={priority}>
                  {priority.replace("_", " ")} priority
                </option>
              ))}
            </select>
            <button
              type="button"
              className="queue-page__create-submit"
              disabled={busy || !name.trim()}
              onClick={() => void createQueue()}
            >
              Create queue
            </button>
          </div>
        ) : null}

        <div className="queue-page__queue-list">
          {queues.map((queue) => {
            const count = downloads.filter(
              (download) =>
                download.queueId === queue.id &&
                download.status.toLowerCase() === "queued",
            ).length;
            return (
              <button
                key={queue.id}
                type="button"
                className={
                  queue.id === selectedQueueId
                    ? "queue-page__queue queue-page__queue--selected"
                    : "queue-page__queue"
                }
                onClick={() => setSelectedQueueId(queue.id)}
              >
                <Layers3 size={16} />
                <span>
                  <strong>{queue.name}</strong>
                  <small>
                    {queue.state} · {count} waiting
                  </small>
                </span>
              </button>
            );
          })}
        </div>
      </aside>

      <section className="queue-page__content">
        {selectedQueue ? (
          <>
            <header className="queue-page__header">
              <div>
                <span
                  className={`queue-page__state queue-page__state--${
                    selectedQueue.enabled
                      ? selectedQueue.state
                      : "disabled"
                  }`}
                >
                  {selectedQueue.enabled
                    ? selectedQueue.state
                    : "disabled"}
                </span>
                <h2>{selectedQueue.name}</h2>
                <p>
                  {selectedQueue.maxConcurrent} concurrent · {selectedQueue.maxConcurrentPerHost ?? "No"} per host · {selectedQueue.defaultPriority.replace("_", " ")} default
                </p>
              </div>
              <div className="queue-page__header-actions">
                <label
                  className="queue-page__enabled"
                  title="A disabled queue never schedules work, even when started."
                >
                  <input
                    type="checkbox"
                    checked={selectedQueue.enabled}
                    disabled={busy}
                    onChange={(event) =>
                      void run(() =>
                        onSetQueueEnabled(
                          selectedQueue.id,
                          event.target.checked,
                        ),
                      )
                    }
                  />
                  Enabled
                </label>

                <button
                  type="button"
                  className="queue-page__runner-button"
                  disabled={busy || !selectedQueue.enabled}
                  onClick={() =>
                    void run(() =>
                      selectedQueue.state === "running"
                        ? onStopQueue(selectedQueue.id)
                        : onStartQueue(selectedQueue.id),
                    )
                  }
                >
                  {selectedQueue.state === "running" ? (
                    <CircleStop size={16} />
                  ) : (
                    <Play size={16} />
                  )}
                  {selectedQueue.state === "running" ? "Stop queue" : "Start queue"}
                </button>
              </div>
            </header>

            {error ? <div className="queue-page__error">{error}</div> : null}

            <div className="queue-page__task-heading">
              <strong>Waiting tasks</strong>
              <span>Priority is evaluated before saved order.</span>
            </div>

            {queueDownloads.length ? (
              <div className="queue-page__tasks">
                {queueDownloads.map((download, index) => (
                  <article
                    key={download.id}
                    className="queue-page__task"
                    draggable={!busy}
                    onDragStart={() => setDraggedId(download.id)}
                    onDragEnd={() => setDraggedId(null)}
                    onDragOver={(event) => event.preventDefault()}
                    onDrop={() => {
                      if (draggedId) void moveInOrder(draggedId, index);
                      setDraggedId(null);
                    }}
                  >
                    <GripVertical size={16} />
                    <div className="queue-page__task-copy">
                      <strong>
                        {download.filename ?? download.sourceUrl}
                      </strong>
                      <span>{new URL(download.sourceUrl).hostname}</span>
                    </div>
                    <select
                      value={download.priority}
                      disabled={busy}
                      onChange={(event) =>
                        void run(() =>
                          onPriority(
                            download.id,
                            event.target.value as DownloadPriority,
                          ),
                        )
                      }
                      aria-label="Task priority"
                    >
                      {priorities.map((priority) => (
                        <option key={priority} value={priority}>
                          {priority.replace("_", " ")}
                        </option>
                      ))}
                    </select>
                    <select
                      value={selectedQueue.id}
                      disabled={busy}
                      onChange={(event) =>
                        void run(() => onMove(download.id, event.target.value))
                      }
                      aria-label="Move task to queue"
                    >
                      {queues.map((queue) => (
                        <option key={queue.id} value={queue.id}>
                          {queue.name}
                        </option>
                      ))}
                    </select>
                    <div className="queue-page__order-actions">
                      <button type="button" disabled={busy || index === 0} onClick={() => void moveInOrder(download.id, 0)} aria-label="Move to top"><ChevronsUp size={14} /></button>
                      <button type="button" disabled={busy || index === 0} onClick={() => void moveInOrder(download.id, index - 1)} aria-label="Move up"><ArrowUp size={14} /></button>
                      <button type="button" disabled={busy || index === queueDownloads.length - 1} onClick={() => void moveInOrder(download.id, index + 1)} aria-label="Move down"><ArrowDown size={14} /></button>
                      <button type="button" disabled={busy || index === queueDownloads.length - 1} onClick={() => void moveInOrder(download.id, queueDownloads.length - 1)} aria-label="Move to bottom"><ChevronsDown size={14} /></button>
                      <button type="button" disabled={busy} onClick={() => void run(() => onRemove(download.id))} aria-label="Remove from queue"><Unlink size={14} /></button>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="queue-page__empty">
                <Layers3 size={26} />
                <strong>No waiting tasks</strong>
                <span>Add downloads to this queue from the Add Download menu.</span>
              </div>
            )}
          </>
        ) : (
          <div className="queue-page__empty">No queue selected.</div>
        )}
      </section>
    </div>
  );
}
