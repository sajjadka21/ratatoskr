import {
  ClipboardPaste,
  Keyboard,
} from "lucide-react";

import "./SettingsPage.css";

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  DownloadCategory,
  DownloadQueue,
  DownloadRule,
  QueueSchedule,
} from "../../types/download";

export type AddDownloadInputMode =
  | "clipboard"
  | "manual";

type SettingsPageProps = {
  inputMode: AddDownloadInputMode;
  saving: boolean;
  error: string | null;
  onInputModeChange: (
    mode: AddDownloadInputMode,
  ) => void;
  queues: DownloadQueue[];
};

export function SettingsPage({
  inputMode,
  saving,
  error,
  onInputModeChange,
  queues,
}: SettingsPageProps) {
  const [schedules, setSchedules] = useState<QueueSchedule[]>([]);
  const [categories, setCategories] = useState<DownloadCategory[]>([]);
  const [rules, setRules] = useState<DownloadRule[]>([]);
  const [scheduleError, setScheduleError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      invoke<QueueSchedule[]>("list_queue_schedules"),
      invoke<DownloadCategory[]>("list_categories"),
      invoke<DownloadRule[]>("list_download_rules"),
    ])
      .then(([nextSchedules, nextCategories, nextRules]) => {
        if (cancelled) return;
        setSchedules(nextSchedules);
        setCategories(nextCategories);
        setRules(nextRules);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setScheduleError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function toggleSchedule(queue: DownloadQueue) {
    const existing = schedules.find((schedule) => schedule.queueId === queue.id);
    const now = Math.floor(Date.now() / 1000);
    const next: QueueSchedule = existing ?? {
      queueId: queue.id,
      enabled: false,
      kind: "daily",
      startAt: now,
      stopAt: null,
      weekdaysMask: 0b0111_1111,
      intervalSeconds: null,
      completionAction: "none",
      preventSleep: false,
      updatedAt: now,
    };
    try {
      const saved = await invoke<QueueSchedule>("set_queue_schedule", {
        ...next,
        enabled: !next.enabled,
        updatedAt: now,
      });
      setSchedules((current) => [
        ...current.filter((schedule) => schedule.queueId !== queue.id),
        saved,
      ]);
      setScheduleError(null);
    } catch (cause) {
      setScheduleError(String(cause));
    }
  }
  return (
    <section className="settings-page">
      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>Add Download</h2>

          <p>
            Choose how Add Download gets its initial links.
          </p>
        </div>

        <div className="settings-page__group">
          <div className="settings-page__group-heading">
            <strong>Input mode</strong>

            <span>
              Single and batch detection is always automatic.
            </span>
          </div>

          <div className="settings-page__options">
            <button
              type="button"
              className={`settings-page__option ${
                inputMode === "clipboard"
                  ? "settings-page__option--active"
                  : ""
              }`}
              disabled={saving}
              onClick={() =>
                onInputModeChange("clipboard")
              }
            >
              <div className="settings-page__option-icon">
                <ClipboardPaste size={19} />
              </div>

              <div className="settings-page__option-copy">
                <strong>Clipboard</strong>

                <span>
                  Automatically extract valid HTTP and HTTPS links from the clipboard when Add Download opens.
                </span>
              </div>

              <span className="settings-page__radio">
                <span />
              </span>
            </button>

            <button
              type="button"
              className={`settings-page__option ${
                inputMode === "manual"
                  ? "settings-page__option--active"
                  : ""
              }`}
              disabled={saving}
              onClick={() =>
                onInputModeChange("manual")
              }
            >
              <div className="settings-page__option-icon">
                <Keyboard size={19} />
              </div>

              <div className="settings-page__option-copy">
                <strong>Manual</strong>

                <span>
                  Open Add Download empty and paste or type the links yourself.
                </span>
              </div>

              <span className="settings-page__radio">
                <span />
              </span>
            </button>
          </div>

          <div className="settings-page__hint">
            In both modes, one valid link becomes a single download and multiple unique links automatically become a batch.
          </div>

          {saving ? (
            <div className="settings-page__status">
              Saving...
            </div>
          ) : null}

          {error ? (
            <div className="settings-page__error">
              {error}
            </div>
          ) : null}
        </div>
      </div>

      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>Queue schedules</h2>
          <p>Run queues automatically while keeping execution in the Rust engine.</p>
        </div>
        <div className="settings-page__schedule-list">
          {queues.length === 0 ? <span className="settings-page__hint">Create a queue to configure a schedule.</span> : null}
          {queues.map((queue) => {
            const schedule = schedules.find((item) => item.queueId === queue.id);
            return (
              <div className="settings-page__schedule" key={queue.id}>
                <div>
                  <strong>{queue.name}</strong>
                  <span>{schedule ? `${schedule.kind} · ${schedule.completionAction}` : "No schedule"}</span>
                </div>
                <button type="button" className="settings-page__secondary-button" onClick={() => void toggleSchedule(queue)}>
                  {schedule?.enabled ? "Disable" : "Enable daily"}
                </button>
              </div>
            );
          })}
        </div>
        {scheduleError ? <div className="settings-page__error">{scheduleError}</div> : null}
      </div>

      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>Categories and rules</h2>
          <p>These backend-owned records determine destination and queue intake decisions.</p>
        </div>
        <div className="settings-page__catalog-grid">
          <div><strong>Categories</strong><span>{categories.length} configured</span></div>
          <div><strong>Rules</strong><span>{rules.filter((rule) => rule.enabled).length} enabled</span></div>
        </div>
      </div>
    </section>
  );
}
