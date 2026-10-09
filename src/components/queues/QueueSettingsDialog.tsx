import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import type { DownloadQueue, QueueSchedule } from "../../types/download";
import { useI18n } from "../../i18n/I18n";
import { ScheduleEditor } from "../settings/SettingsPage";
import "./QueueSettingsDialog.css";

export function QueueSettingsDialog({ queue, onClose, onLimits, onEnabled }: {
  queue: DownloadQueue; onClose: () => void;
  onLimits: (id: string, concurrent: number, perHost: number | null) => Promise<void>;
  onEnabled: (id: string, enabled: boolean) => Promise<void>;
}) {
  const { t, dir } = useI18n();
  const ref = useRef<HTMLDialogElement>(null);
  const [schedule, setSchedule] = useState<QueueSchedule | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    return () => { if (dialog.open) dialog.close(); };
  }, []);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setLoadFailed(false);
    invoke<QueueSchedule[]>("list_queue_schedules").then((rows) => {
      if (active) { setSchedule(rows.find((row) => row.queueId === queue.id) ?? null); setLoading(false); }
    }).catch((reason) => { if (active) { setError(String(reason)); setLoadFailed(true); setLoading(false); } });
    return () => { active = false; };
  }, [queue.id]);
  async function save(action: () => Promise<void>) {
    setBusy(true); setError(null);
    try { await action(); } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }
  return <dialog ref={ref} className="queue-settings-dialog settings-page" dir={dir} aria-labelledby="queue-settings-title" onCancel={onClose}>
    <header><h2 id="queue-settings-title">{queue.name}</h2><button autoFocus type="button" onClick={onClose} aria-label={t("add.close")}><X size={20} /></button></header>
    {error ? <p role="alert">{error}</p> : null}
    <div className="queue-settings-dialog__limits">
      <label>{t("queues.concurrent")}<select value={queue.maxConcurrent} disabled={busy} onChange={(event) => void save(() => onLimits(queue.id, Number(event.target.value), queue.maxConcurrentPerHost))}>
        {[...new Set([1, 2, 3, 4, 5, 8, 10, 16, queue.maxConcurrent])].sort((a,b) => a-b).map((value) => <option key={value}>{value}</option>)}
      </select></label>
      <label>{t("queues.enabled")}<input type="checkbox" checked={queue.enabled} disabled={busy} onChange={(event) => void save(() => onEnabled(queue.id, event.target.checked))} /></label>
    </div>
    {!loading && !loadFailed ? <ScheduleEditor queue={queue} schedule={schedule} t={t} onSaved={(saved) => { setSchedule(saved); setError(null); }} onError={setError} /> : null}
    <footer><button type="button" onClick={onClose}>{t("add.close")}</button></footer>
  </dialog>;
}
