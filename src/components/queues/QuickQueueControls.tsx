import { CalendarClock, ChevronDown, CircleStop, ListOrdered, Play, Settings2 } from "lucide-react";
import type { DownloadQueue } from "../../types/download";
import { useI18n } from "../../i18n/I18n";

type Props = {
  queues: DownloadQueue[]; selected: DownloadQueue | undefined;
  busy: boolean; ready: boolean;
  onSelect: (id: string) => void; onToggle: () => void; onManage: () => void; onSchedule: () => void;
};

export function QuickQueueControls({ queues, selected, busy, ready, onSelect, onToggle, onManage, onSchedule }: Props) {
  const { t } = useI18n();
  const running = selected?.state === "running";
  const label = t(running ? "queues.stop" : "queues.start");
  return <details className="quick-queue" onBlur={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) event.currentTarget.open = false;
  }} onKeyDown={(event) => { if (event.key === "Escape") { event.currentTarget.open = false; event.currentTarget.querySelector("summary")?.focus(); } }}>
    <summary aria-label={t("queues.control")} title={t("queues.control")}><ListOrdered size={16} /><span>{t("queues.control")}</span><ChevronDown size={14} /></summary>
    <div className="quick-queue__panel">
    <select aria-label={t("queues.quickSelect")} value={selected?.id ?? ""}
      disabled={busy || !queues.length} onChange={(event) => onSelect(event.target.value)}>
      {!queues.length ? <option value="">{t("queues.quickSelect")}</option> : null}
      {queues.map((queue) => <option key={queue.id} value={queue.id}>{queue.name}</option>)}
    </select>
    <button type="button" onClick={onToggle} disabled={busy || !ready || !selected || !selected.enabled || running}
      aria-label={label} title={`${label} · Ctrl Shift Q`} aria-busy={busy}>
      <Play size={15} /><span>{t("queues.start")}</span>
    </button>
    <button type="button" onClick={onToggle} disabled={busy || !ready || !selected || !running} aria-label={t("queues.stop")}>
      <CircleStop size={15} /><span>{t("queues.stop")}</span>
    </button>
    <button type="button" className="quick-queue__manage" onClick={onManage} disabled={!selected}
      aria-label={t("queues.manageShortcut")} title={t("queues.manageShortcut")}>
      <Settings2 size={15} /><span>{t("queues.manageShortcut")}</span>
    </button>
    <button type="button" className="quick-queue__manage" onClick={onSchedule} disabled={!selected}
      aria-label={t("queues.scheduleShortcut")} title={t("queues.scheduleShortcut")}>
      <CalendarClock size={15} /><span>{t("queues.scheduleShortcut")}</span>
    </button>
    </div>
  </details>;
}
