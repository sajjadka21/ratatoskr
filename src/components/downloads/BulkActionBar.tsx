import { Pause, Play, RotateCcw, Trash2, X, XCircle } from "lucide-react";

import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";
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

const ACTIONS: Array<{ action: BulkAction; icon: typeof Play }> = [
  { action: "start", icon: Play },
  { action: "pause", icon: Pause },
  { action: "resume", icon: Play },
  { action: "retry", icon: RotateCcw },
  { action: "cancel", icon: XCircle },
  { action: "remove", icon: Trash2 },
];

const PRIORITIES: DownloadPriority[] = ["very_high", "high", "normal", "low"];

export function BulkActionBar({ count, queues, onAction, onQueue, onPriority, onClear }: BulkActionBarProps) {
  const { t, fmt } = useI18n();

  return (
    <div className="bulk-action-bar" role="toolbar" aria-label={t("bulk.label")}>
      <strong className="num">{t("table.selected", { count: fmt.number(count) })}</strong>
      {ACTIONS.map(({ action, icon: Icon }) => (
        <button key={action} type="button" onClick={() => onAction(action)}>
          <Icon size={14} />
          {t(`action.${action}` as MessageKey)}
        </button>
      ))}
      <label className="bulk-action-bar__select">
        <span>{t("bulk.queue")}</span>
        <select value="" onChange={(event) => event.target.value && onQueue(event.target.value)}>
          <option value="">{t("bulk.moveTo")}</option>
          {queues.map((queue) => (
            <option key={queue.id} value={queue.id}>
              {queue.name}
            </option>
          ))}
        </select>
      </label>
      <label className="bulk-action-bar__select">
        <span>{t("bulk.priority")}</span>
        <select value="" onChange={(event) => event.target.value && onPriority(event.target.value as DownloadPriority)}>
          <option value="">{t("bulk.set")}</option>
          {PRIORITIES.map((priority) => (
            <option key={priority} value={priority}>
              {t(`priority.${priority}` as MessageKey)}
            </option>
          ))}
        </select>
      </label>
      <button type="button" className="bulk-action-bar__close" onClick={onClear} aria-label={t("bulk.clear")}>
        <X size={15} />
      </button>
    </div>
  );
}
