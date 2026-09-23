import {
  ClipboardPaste,
  FolderOpen,
  Keyboard,
  Plus,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import type {
  CompletionAction,
  DownloadCategory,
  DownloadPriority,
  DownloadQueue,
  DownloadRule,
  DownloadSettings,
  QueueSchedule,
} from "../../types/download";
import {
  formatMinuteOfDay,
  kibPerSecondToBytes,
  parseMinuteOfDay,
  bytesToKibPerSecond,
} from "../../utils/settingsFormat";

import "./SettingsPage.css";

export type AddDownloadInputMode = "clipboard" | "manual";

type SettingsPageProps = {
  inputMode: AddDownloadInputMode;
  saving: boolean;
  error: string | null;
  onInputModeChange: (mode: AddDownloadInputMode) => void;
  queues: DownloadQueue[];
  downloadSettings: DownloadSettings | null;
  onDownloadSettingsChange: (settings: DownloadSettings) => void;
  onError: (message: string) => void;
  onSaved: (message: string) => void;
};

/// Opens the system folder picker. Returns null when the user cancels.
export async function pickFolder(title: string, current?: string | null) {
  const selected = await openDialog({
    directory: true,
    multiple: false,
    title,
    defaultPath: current ?? undefined,
  });
  return typeof selected === "string" ? selected : null;
}

export function SettingsPage({
  inputMode,
  saving,
  error,
  onInputModeChange,
  queues,
  downloadSettings,
  onDownloadSettingsChange,
  onError,
  onSaved,
}: SettingsPageProps) {
  const [schedules, setSchedules] = useState<QueueSchedule[]>([]);
  const [categories, setCategories] = useState<DownloadCategory[]>([]);
  const [rules, setRules] = useState<DownloadRule[]>([]);

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
        if (!cancelled) onError(`Could not load settings: ${String(cause)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [onError]);

  return (
    <section className="settings-page">
      <DownloadsSection
        settings={downloadSettings}
        onChange={onDownloadSettingsChange}
        onError={onError}
      />

      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>Add Download</h2>
          <p>Choose how Add Download gets its initial links.</p>
        </div>

        <div className="settings-page__group">
          <div className="settings-page__options">
            <InputModeOption
              active={inputMode === "clipboard"}
              disabled={saving}
              icon={<ClipboardPaste size={19} />}
              title="Clipboard"
              description="Extract valid HTTP and HTTPS links from the clipboard when Add Download opens."
              onSelect={() => onInputModeChange("clipboard")}
            />
            <InputModeOption
              active={inputMode === "manual"}
              disabled={saving}
              icon={<Keyboard size={19} />}
              title="Manual"
              description="Open Add Download empty and paste or type the links yourself."
              onSelect={() => onInputModeChange("manual")}
            />
          </div>
          <div className="settings-page__hint">
            In both modes, one valid link becomes a single download and several
            unique links become a batch.
          </div>
          {error ? <div className="settings-page__error">{error}</div> : null}
        </div>
      </div>

      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>Queue schedules</h2>
          <p>
            Start a queue at a time of day and choose what happens when it
            finishes. Times use this computer&apos;s clock.
          </p>
        </div>
        {queues.length === 0 ? (
          <div className="settings-page__hint">
            Create a queue first to give it a schedule.
          </div>
        ) : (
          <div className="settings-page__stack">
            {queues.map((queue) => (
              <ScheduleEditor
                key={queue.id}
                queue={queue}
                schedule={
                  schedules.find((item) => item.queueId === queue.id) ?? null
                }
                onSaved={(saved) => {
                  setSchedules((current) => [
                    ...current.filter((item) => item.queueId !== saved.queueId),
                    saved,
                  ]);
                  onSaved(`Schedule for “${queue.name}” saved`);
                }}
                onError={onError}
              />
            ))}
          </div>
        )}
      </div>

      <RulesSection
        rules={rules}
        categories={categories}
        queues={queues}
        onRulesChange={setRules}
        onError={onError}
        onSaved={onSaved}
      />
    </section>
  );
}

function InputModeOption({
  active,
  disabled,
  icon,
  title,
  description,
  onSelect,
}: {
  active: boolean;
  disabled: boolean;
  icon: React.ReactNode;
  title: string;
  description: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      className={`settings-page__option ${
        active ? "settings-page__option--active" : ""
      }`}
      disabled={disabled}
      aria-pressed={active}
      onClick={onSelect}
    >
      <div className="settings-page__option-icon">{icon}</div>
      <div className="settings-page__option-copy">
        <strong>{title}</strong>
        <span>{description}</span>
      </div>
      <span className="settings-page__radio">
        <span />
      </span>
    </button>
  );
}

// ---------------------------------------------------------------------------
// Downloads: folder, speed limit, sleep
// ---------------------------------------------------------------------------

const SPEED_PRESETS: Array<{ label: string; kib: number | null }> = [
  { label: "Unlimited", kib: null },
  { label: "256 KB/s", kib: 256 },
  { label: "1 MB/s", kib: 1024 },
  { label: "5 MB/s", kib: 5 * 1024 },
];

function DownloadsSection({
  settings,
  onChange,
  onError,
}: {
  settings: DownloadSettings | null;
  onChange: (settings: DownloadSettings) => void;
  onError: (message: string) => void;
}) {
  const currentKib = bytesToKibPerSecond(settings?.globalSpeedLimit ?? null);
  const [customKib, setCustomKib] = useState("");

  useEffect(() => {
    setCustomKib(currentKib === null ? "" : String(currentKib));
  }, [currentKib]);

  async function run(command: string, args: Record<string, unknown>) {
    try {
      onChange(await invoke<DownloadSettings>(command, args));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function chooseFolder() {
    try {
      const folder = await pickFolder(
        "Choose the default download folder",
        settings?.defaultDirectory ?? settings?.systemDirectory,
      );
      if (folder) {
        await run("set_default_download_directory", { directory: folder });
      }
    } catch (reason) {
      onError(`Could not open the folder picker: ${String(reason)}`);
    }
  }

  function applyCustom() {
    const kib = Number(customKib);
    if (!customKib.trim() || !Number.isFinite(kib) || kib <= 0) {
      onError("Enter a speed above 0 KB/s, or choose Unlimited.");
      return;
    }
    void run("set_global_speed_limit", {
      bytesPerSecond: kibPerSecondToBytes(kib),
    });
  }

  const isPreset = SPEED_PRESETS.some((preset) => preset.kib === currentKib);

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>Downloads</h2>
        <p>Where files go and how much bandwidth downloads may use.</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>Default folder</strong>
            <span>
              Used when no category or rule names a folder.
            </span>
          </div>
          <div className="settings-page__row-control">
            <code className="settings-page__path" title={settings?.defaultDirectory ?? settings?.systemDirectory ?? ""}>
              {settings?.defaultDirectory ??
                settings?.systemDirectory ??
                "System Downloads folder"}
            </code>
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={() => void chooseFolder()}
            >
              <FolderOpen size={14} /> Change…
            </button>
            {settings?.defaultDirectory ? (
              <button
                type="button"
                className="settings-page__icon-button"
                aria-label="Use the system Downloads folder"
                title="Use the system Downloads folder"
                onClick={() =>
                  void run("set_default_download_directory", { directory: null })
                }
              >
                <RotateCcw size={14} />
              </button>
            ) : null}
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>Speed limit</strong>
            <span>Shared by every download. Applies at once, even to running ones.</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--wrap">
            <div className="settings-page__segmented" role="group" aria-label="Speed limit">
              {SPEED_PRESETS.map((preset) => (
                <button
                  key={preset.label}
                  type="button"
                  aria-pressed={preset.kib === currentKib}
                  className={
                    preset.kib === currentKib
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() =>
                    void run("set_global_speed_limit", {
                      bytesPerSecond:
                        preset.kib === null ? null : kibPerSecondToBytes(preset.kib),
                    })
                  }
                >
                  {preset.label}
                </button>
              ))}
            </div>
            <label className="settings-page__inline-field">
              <span className={isPreset ? "" : "settings-page__custom-active"}>Custom</span>
              <input
                id="custom-speed-limit"
                type="number"
                min={1}
                step={1}
                inputMode="numeric"
                value={customKib}
                onChange={(event) => setCustomKib(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyCustom();
                }}
              />
              <span>KB/s</span>
            </label>
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={applyCustom}
            >
              Apply
            </button>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>Keep the computer awake</strong>
            <span>Stops Windows from sleeping while a download is running.</span>
          </div>
          <div className="settings-page__row-control">
            <label className="settings-page__switch">
              <input
                id="prevent-sleep"
                type="checkbox"
                checked={settings?.preventSleep ?? true}
                onChange={(event) =>
                  void run("set_prevent_sleep", { enabled: event.target.checked })
                }
              />
              <span aria-hidden="true" />
              <span className="settings-page__visually-hidden">Keep the computer awake</span>
            </label>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const ALL_DAYS = 0b0111_1111;

const COMPLETION_ACTIONS: Array<{ value: CompletionAction; label: string }> = [
  { value: "none", label: "Do nothing" },
  { value: "notify", label: "Notify me" },
  { value: "exit_app", label: "Close Download Manager" },
  { value: "sleep", label: "Sleep" },
  { value: "hibernate", label: "Hibernate" },
  { value: "shutdown", label: "Shut down" },
];

type ScheduleDraft = {
  enabled: boolean;
  days: "every" | "selected";
  weekdaysMask: number;
  start: string;
  end: string;
  completionAction: CompletionAction;
  preventSleep: boolean;
};

function draftFrom(schedule: QueueSchedule | null): ScheduleDraft {
  const start = schedule?.windowStartMinute ?? 2 * 60;
  const end = schedule?.windowEndMinute ?? 7 * 60;
  return {
    enabled: schedule?.enabled ?? false,
    days: schedule?.kind === "weekdays" ? "selected" : "every",
    weekdaysMask:
      schedule && schedule.weekdaysMask > 0 ? schedule.weekdaysMask : ALL_DAYS,
    start: formatMinuteOfDay(start),
    end: formatMinuteOfDay(end),
    completionAction: schedule?.completionAction ?? "none",
    preventSleep: schedule?.preventSleep ?? true,
  };
}

function ScheduleEditor({
  queue,
  schedule,
  onSaved,
  onError,
}: {
  queue: DownloadQueue;
  schedule: QueueSchedule | null;
  onSaved: (schedule: QueueSchedule) => void;
  onError: (message: string) => void;
}) {
  const [draft, setDraft] = useState(() => draftFrom(schedule));
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(draftFrom(schedule));
  }, [schedule]);

  const idPrefix = `schedule-${queue.id}`;
  const isPowerAction = ["sleep", "hibernate", "shutdown", "exit_app"].includes(
    draft.completionAction,
  );

  async function save() {
    const start = parseMinuteOfDay(draft.start);
    const end = parseMinuteOfDay(draft.end);
    if (start === null || end === null) {
      onError("Enter times as HH:MM, for example 02:00.");
      return;
    }
    if (draft.days === "selected" && draft.weekdaysMask === 0) {
      onError("Choose at least one day.");
      return;
    }

    setSaving(true);
    try {
      const now = Math.floor(Date.now() / 1000);
      const saved = await invoke<QueueSchedule>("set_queue_schedule", {
        queueId: queue.id,
        enabled: draft.enabled,
        kind: draft.days === "every" ? "daily" : "weekdays",
        startAt: schedule?.startAt && schedule.startAt < now ? schedule.startAt : 0,
        stopAt: null,
        weekdaysMask: draft.days === "every" ? ALL_DAYS : draft.weekdaysMask,
        intervalSeconds: null,
        completionAction: draft.completionAction,
        preventSleep: draft.preventSleep,
        updatedAt: now,
        windowStartMinute: start,
        windowEndMinute: end,
      });
      onSaved(saved);
    } catch (reason) {
      onError(`Could not save the schedule: ${String(reason)}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="settings-page__group settings-page__schedule-editor">
      <div className="settings-page__schedule-head">
        <label className="settings-page__switch">
          <input
            id={`${idPrefix}-enabled`}
            type="checkbox"
            checked={draft.enabled}
            onChange={(event) =>
              setDraft({ ...draft, enabled: event.target.checked })
            }
          />
          <span aria-hidden="true" />
          <span className="settings-page__visually-hidden">
            Run {queue.name} on a schedule
          </span>
        </label>
        <strong>{queue.name}</strong>
        <span className="settings-page__muted">
          {draft.enabled
            ? `Runs ${draft.start}–${draft.end}${
                draft.days === "every" ? " every day" : " on selected days"
              }`
            : "Not scheduled"}
        </span>
      </div>

      <div className="settings-page__schedule-grid" aria-disabled={!draft.enabled}>
        <label className="settings-page__field">
          <span>From</span>
          <input
            id={`${idPrefix}-start`}
            type="time"
            value={draft.start}
            onChange={(event) => setDraft({ ...draft, start: event.target.value })}
          />
        </label>
        <label className="settings-page__field">
          <span>Until</span>
          <input
            id={`${idPrefix}-end`}
            type="time"
            value={draft.end}
            onChange={(event) => setDraft({ ...draft, end: event.target.value })}
          />
        </label>
        <label className="settings-page__field">
          <span>Days</span>
          <select
            id={`${idPrefix}-days`}
            value={draft.days}
            onChange={(event) =>
              setDraft({
                ...draft,
                days: event.target.value as ScheduleDraft["days"],
              })
            }
          >
            <option value="every">Every day</option>
            <option value="selected">Selected days</option>
          </select>
        </label>
        <label className="settings-page__field">
          <span>When the queue finishes</span>
          <select
            id={`${idPrefix}-action`}
            value={draft.completionAction}
            onChange={(event) =>
              setDraft({
                ...draft,
                completionAction: event.target.value as CompletionAction,
              })
            }
          >
            {COMPLETION_ACTIONS.map((action) => (
              <option key={action.value} value={action.value}>
                {action.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      {draft.days === "selected" ? (
        <div className="settings-page__weekdays" role="group" aria-label="Days">
          {WEEKDAYS.map((label, index) => {
            const on = (draft.weekdaysMask & (1 << index)) !== 0;
            return (
              <button
                key={label}
                type="button"
                aria-pressed={on}
                className={
                  on
                    ? "settings-page__day settings-page__day--active"
                    : "settings-page__day"
                }
                onClick={() =>
                  setDraft({
                    ...draft,
                    weekdaysMask: draft.weekdaysMask ^ (1 << index),
                  })
                }
              >
                {label}
              </button>
            );
          })}
        </div>
      ) : null}

      <div className="settings-page__schedule-foot">
        <label className="settings-page__checkbox">
          <input
            id={`${idPrefix}-awake`}
            type="checkbox"
            checked={draft.preventSleep}
            onChange={(event) =>
              setDraft({ ...draft, preventSleep: event.target.checked })
            }
          />
          Keep the computer awake while this queue runs
        </label>
        {isPowerAction ? (
          <span className="settings-page__muted">
            You get 60 seconds to cancel. Skipped if other downloads are still running.
          </span>
        ) : null}
        <button
          type="button"
          className="settings-page__primary-button"
          disabled={saving}
          onClick={() => void save()}
        >
          {saving ? "Saving…" : "Save schedule"}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

type RuleDraft = {
  id: string;
  name: string;
  enabled: boolean;
  domain: string;
  extension: string;
  urlPattern: string;
  minSizeMb: string;
  destinationDirectory: string;
  queueId: string;
  priority: "" | DownloadPriority;
  maxConnections: string;
  speedCapKib: string;
};

const EMPTY_RULE: RuleDraft = {
  id: "",
  name: "",
  enabled: true,
  domain: "",
  extension: "",
  urlPattern: "",
  minSizeMb: "",
  destinationDirectory: "",
  queueId: "",
  priority: "",
  maxConnections: "",
  speedCapKib: "",
};

const MIB = 1024 * 1024;

function ruleToDraft(rule: DownloadRule): RuleDraft {
  return {
    id: rule.id,
    name: rule.name,
    enabled: rule.enabled,
    domain: rule.domain ?? "",
    extension: rule.extension ?? "",
    urlPattern: rule.urlPattern ?? "",
    minSizeMb: rule.minSize ? String(Math.round(rule.minSize / MIB)) : "",
    destinationDirectory: rule.destinationDirectory ?? "",
    queueId: rule.queueId ?? "",
    priority: rule.priority ?? "",
    maxConnections: rule.maxConnections ? String(rule.maxConnections) : "",
    speedCapKib: rule.speedCap ? String(bytesToKibPerSecond(rule.speedCap)) : "",
  };
}

function positive(value: string): number | null {
  const number = Number(value);
  return value.trim() && Number.isFinite(number) && number > 0 ? number : null;
}

function draftToRule(draft: RuleDraft, sortOrder: number): DownloadRule {
  const text = (value: string) => (value.trim() ? value.trim() : null);
  const minMb = positive(draft.minSizeMb);
  const speed = positive(draft.speedCapKib);
  const connections = positive(draft.maxConnections);
  return {
    id: draft.id,
    name: draft.name.trim(),
    enabled: draft.enabled,
    sortOrder,
    domain: text(draft.domain),
    urlPattern: text(draft.urlPattern),
    extension: text(draft.extension.replace(/^\./, "")),
    mimePattern: null,
    minSize: minMb ? Math.round(minMb * MIB) : null,
    maxSize: null,
    categoryId: null,
    destinationDirectory: text(draft.destinationDirectory),
    queueId: text(draft.queueId),
    priority: draft.priority || null,
    maxConnections: connections ? Math.round(connections) : null,
    maxHostConcurrency: null,
    speedCap: speed ? kibPerSecondToBytes(speed) : null,
    browserTakeoverAllowed: null,
  };
}

function describeRule(rule: DownloadRule, queues: DownloadQueue[]): string {
  const when = [
    rule.domain ? `from ${rule.domain}` : null,
    rule.extension ? `.${rule.extension} files` : null,
    rule.urlPattern ? `URL like ${rule.urlPattern}` : null,
    rule.minSize ? `over ${Math.round(rule.minSize / MIB)} MB` : null,
  ].filter(Boolean);
  const then = [
    rule.destinationDirectory ? `save to ${rule.destinationDirectory}` : null,
    rule.queueId
      ? `queue in ${queues.find((queue) => queue.id === rule.queueId)?.name ?? "a queue"}`
      : null,
    rule.priority ? `${rule.priority.replace("_", " ")} priority` : null,
    rule.maxConnections ? `at most ${rule.maxConnections} connections` : null,
    rule.speedCap ? `limit to ${bytesToKibPerSecond(rule.speedCap)} KB/s` : null,
  ].filter(Boolean);
  return `${when.join(", ") || "Any download"} → ${then.join(", ") || "no change"}`;
}

function RulesSection({
  rules,
  categories,
  queues,
  onRulesChange,
  onError,
  onSaved,
}: {
  rules: DownloadRule[];
  categories: DownloadCategory[];
  queues: DownloadQueue[];
  onRulesChange: (rules: DownloadRule[]) => void;
  onError: (message: string) => void;
  onSaved: (message: string) => void;
}) {
  const [draft, setDraft] = useState<RuleDraft | null>(null);

  async function save() {
    if (!draft) return;
    if (!draft.name.trim()) {
      onError("Give the rule a name.");
      return;
    }
    const index = rules.findIndex((rule) => rule.id === draft.id);
    const sortOrder = index >= 0 ? rules[index].sortOrder : rules.length;
    try {
      const saved = await invoke<DownloadRule>("save_download_rule", {
        rule: draftToRule(draft, sortOrder),
      });
      onRulesChange(
        index >= 0
          ? rules.map((rule) => (rule.id === saved.id ? saved : rule))
          : [...rules, saved],
      );
      setDraft(null);
      onSaved(`Rule “${saved.name}” saved`);
    } catch (reason) {
      onError(`Could not save the rule: ${String(reason)}`);
    }
  }

  async function toggle(rule: DownloadRule) {
    try {
      const saved = await invoke<DownloadRule>("save_download_rule", {
        rule: { ...rule, enabled: !rule.enabled },
      });
      onRulesChange(rules.map((item) => (item.id === saved.id ? saved : item)));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function remove(rule: DownloadRule) {
    try {
      await invoke("delete_download_rule", { id: rule.id });
      onRulesChange(rules.filter((item) => item.id !== rule.id));
      if (draft?.id === rule.id) setDraft(null);
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function chooseRuleFolder() {
    if (!draft) return;
    try {
      const folder = await pickFolder("Folder for this rule", draft.destinationDirectory || null);
      if (folder) setDraft({ ...draft, destinationDirectory: folder });
    } catch (reason) {
      onError(`Could not open the folder picker: ${String(reason)}`);
    }
  }

  const field = (key: keyof RuleDraft) => ({
    id: `rule-${key}`,
    value: draft ? String(draft[key]) : "",
    onChange: (
      event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>,
    ) => draft && setDraft({ ...draft, [key]: event.target.value }),
  });

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading settings-page__section-heading--row">
        <div>
          <h2>Rules</h2>
          <p>
            Rules run in order; the first match decides. Without a match, the
            file&apos;s category ({categories.length} configured) decides the folder.
          </p>
        </div>
        <button
          type="button"
          className="settings-page__secondary-button"
          onClick={() => setDraft({ ...EMPTY_RULE })}
        >
          <Plus size={14} /> New rule
        </button>
      </div>

      {rules.length === 0 && !draft ? (
        <div className="settings-page__hint">
          No rules yet. Example: send everything from a university site to a
          Lectures folder, or limit large ISO files to 2 MB/s.
        </div>
      ) : null}

      {rules.length > 0 ? (
        <ul className="settings-page__rule-list">
          {rules.map((rule) => (
            <li key={rule.id} className="settings-page__rule">
              <label className="settings-page__switch">
                <input
                  id={`rule-enabled-${rule.id}`}
                  type="checkbox"
                  checked={rule.enabled}
                  onChange={() => void toggle(rule)}
                />
                <span aria-hidden="true" />
                <span className="settings-page__visually-hidden">
                  Enable {rule.name}
                </span>
              </label>
              <button
                type="button"
                className="settings-page__rule-body"
                onClick={() => setDraft(ruleToDraft(rule))}
              >
                <strong>{rule.name}</strong>
                <span>{describeRule(rule, queues)}</span>
              </button>
              <button
                type="button"
                className="settings-page__icon-button"
                aria-label={`Delete rule ${rule.name}`}
                onClick={() => void remove(rule)}
              >
                <Trash2 size={14} />
              </button>
            </li>
          ))}
        </ul>
      ) : null}

      {draft ? (
        <div className="settings-page__group settings-page__rule-editor">
          <label className="settings-page__field settings-page__field--wide">
            <span>Name</span>
            <input {...field("name")} placeholder="University lectures" />
          </label>

          <fieldset className="settings-page__fieldset">
            <legend>When the download…</legend>
            <label className="settings-page__field">
              <span>comes from (domain)</span>
              <input {...field("domain")} placeholder="example.com" />
            </label>
            <label className="settings-page__field">
              <span>has the extension</span>
              <input {...field("extension")} placeholder="iso" />
            </label>
            <label className="settings-page__field">
              <span>URL matches</span>
              <input {...field("urlPattern")} placeholder="*/lectures/*" />
            </label>
            <label className="settings-page__field">
              <span>is larger than (MB)</span>
              <input {...field("minSizeMb")} type="number" min={1} />
            </label>
          </fieldset>

          <fieldset className="settings-page__fieldset">
            <legend>…then</legend>
            <label className="settings-page__field settings-page__field--wide">
              <span>save to folder</span>
              <div className="settings-page__input-with-button">
                <input {...field("destinationDirectory")} placeholder="Keep the category folder" />
                <button
                  type="button"
                  className="settings-page__secondary-button"
                  onClick={() => void chooseRuleFolder()}
                >
                  <FolderOpen size={14} /> Choose…
                </button>
              </div>
            </label>
            <label className="settings-page__field">
              <span>add to queue</span>
              <select {...field("queueId")}>
                <option value="">Don&apos;t queue</option>
                {queues.map((queue) => (
                  <option key={queue.id} value={queue.id}>
                    {queue.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="settings-page__field">
              <span>priority</span>
              <select {...field("priority")}>
                <option value="">Unchanged</option>
                <option value="very_high">Very high</option>
                <option value="high">High</option>
                <option value="normal">Normal</option>
                <option value="low">Low</option>
              </select>
            </label>
            <label className="settings-page__field">
              <span>max connections</span>
              <input {...field("maxConnections")} type="number" min={1} max={64} placeholder="Automatic" />
            </label>
            <label className="settings-page__field">
              <span>speed limit (KB/s)</span>
              <input {...field("speedCapKib")} type="number" min={1} placeholder="No limit" />
            </label>
          </fieldset>

          <div className="settings-page__schedule-foot">
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={() => setDraft(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="settings-page__primary-button"
              onClick={() => void save()}
            >
              Save rule
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
