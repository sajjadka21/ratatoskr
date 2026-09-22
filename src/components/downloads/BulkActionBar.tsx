import { Pause, Play, RotateCcw, Trash2, X, XCircle } from "lucide-react";

import type { DownloadPriority, DownloadQueue } from "../../types/download";

import "./BulkActionBar.css";

export type BulkAction = "start" | "pause" | "resume" | "retry" | "cancel" | "remove";

type BulkActionBarProps = {
  count: number;
  queues: DownloadQueue[];
  onAction: (action: BulkAction) => void;
  onQueue: (queueId: string) => void;
  onPriority: (priority: DownloadPriority) => void;
  onClear: () => void;
};

const ACTIONS: Array<{ action: BulkAction; label: string; icon: typeof Play }> = [
  { action: "start", label: "Start", icon: Play },
  { action: "pause", label: "Pause", icon: Pause },
  { action: "resume", label: "Resume", icon: Play },
  { action: "retry", label: "Retry", icon: RotateCcw },
  { action: "cancel", label: "Cancel", icon: XCircle },
  { action: "remove", label: "Remove", icon: Trash2 },
];

const PRIORITIES: Array<{ value: DownloadPriority; label: string }> = [
  { value: "very_high", label: "Very high" },
  { value: "high", label: "High" },
  { value: "normal", label: "Normal" },
  { value: "low", label: "Low" },
];

export function BulkActionBar({
  count,
  queues,
  onAction,
  onQueue,
  onPriority,
  onClear,
}: BulkActionBarProps) {
  return (
    <div className="bulk-action-bar" role="toolbar" aria-label="Bulk download actions">
      <strong>{count} selected</strong>

      {ACTIONS.map(({ action, label, icon: Icon }) => (
        <button key={action} type="button" onClick={() => onAction(action)}>
          <Icon size={14} />
          {label}
        </button>
      ))}

      <label className="bulk-action-bar__select">
        <span>Queue</span>
        <select defaultValue="" onChange={(event) => event.target.value && onQueue(event.target.value)}>
          <option value="">Move to…</option>
          {queues.map((queue) => (
            <option key={queue.id} value={queue.id}>{queue.name}</option>
          ))}
        </select>
      </label>

      <label className="bulk-action-bar__select">
        <span>Priority</span>
        <select defaultValue="" onChange={(event) => event.target.value && onPriority(event.target.value as DownloadPriority)}>
          <option value="">Set…</option>
          {PRIORITIES.map((priority) => (
            <option key={priority.value} value={priority.value}>{priority.label}</option>
          ))}
        </select>
      </label>

      <button type="button" className="bulk-action-bar__close" onClick={onClear} aria-label="Clear selection">
        <X size={15} />
      </button>
    </div>
  );
}
