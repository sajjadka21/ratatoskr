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

import { useI18n, type Translate } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";
import type { Formatter } from "../../i18n/format";
import type {
  CompletionAction,
  DownloadCategory,
  DownloadPriority,
  DownloadQueue,
  DownloadRule,
  DownloadSettings,
  QueueSchedule,
  UiPreferences,
} from "../../types/download";
import {
  formatMinuteOfDay,
  kibPerSecondToBytes,
  parseMinuteOfDay,
  bytesToKibPerSecond,
} from "../../utils/settingsFormat";

import { Switch } from "./Switch";
import { BRAND_THEMES, BRAND_THEME_SWATCHES } from "../../types/download";
import { SHOW_WELCOME_EVENT } from "../../utils/appEvents";
import { UpdateSection } from "./UpdateSection";
import { BrowserSection } from "./BrowserSection";
import { AfterDownloadSection } from "./AfterDownloadSection";
import { BackupSection, DiagnosticsSection } from "./MaintenanceSections";
import { EngineSection, NetworkSection, TrafficSection } from "./NetworkSections";

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
  uiPreferences: UiPreferences;
  onUiPreferencesChange: (preferences: UiPreferences) => void;
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
  uiPreferences,
  onUiPreferencesChange,
}: SettingsPageProps) {
  const { t, fmt } = useI18n();
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
        if (!cancelled) onError(t("settings.loadFailed", { reason: String(cause) }));
      });
    return () => {
      cancelled = true;
    };
  }, [onError, t]);

  return (
    <section className="settings-page">
      <AppearanceSection
        preferences={uiPreferences}
        onChange={onUiPreferencesChange}
        onError={onError}
      />

      <DownloadsSection
        settings={downloadSettings}
        onChange={onDownloadSettingsChange}
        onError={onError}
      />

      <EngineSection onError={onError} onSaved={onSaved} />

      <AfterDownloadSection onError={onError} onSaved={onSaved} />

      <TrafficSection onError={onError} onSaved={onSaved} />

      <NetworkSection onError={onError} onSaved={onSaved} />

      <BackupSection onError={onError} onSaved={onSaved} />

      <DiagnosticsSection onError={onError} onSaved={onSaved} />

      <BrowserSection onError={onError} />

      <UpdateSection onError={onError} />

      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>{t("settings.addDownload")}</h2>
          <p>{t("settings.addDownloadHint")}</p>
        </div>

        <div className="settings-page__group">
          <div className="settings-page__options">
            <InputModeOption
              active={inputMode === "clipboard"}
              disabled={saving}
              icon={<ClipboardPaste size={19} />}
              title={t("settings.clipboard")}
              description={t("settings.clipboardHint")}
              onSelect={() => onInputModeChange("clipboard")}
            />
            <InputModeOption
              active={inputMode === "manual"}
              disabled={saving}
              icon={<Keyboard size={19} />}
              title={t("settings.manual")}
              description={t("settings.manualHint")}
              onSelect={() => onInputModeChange("manual")}
            />
          </div>
          <div className="settings-page__hint">
            {t("settings.inputHint")}
          </div>
          <IntakeWindowRow onError={onError} />
          <ClipboardWatchRow onError={onError} />
          {error ? <div className="settings-page__error">{error}</div> : null}
        </div>
      </div>

      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>{t("settings.schedules")}</h2>
          <p>{t("settings.schedulesHint")}</p>
        </div>
        {queues.length === 0 ? (
          <div className="settings-page__hint">
            {t("settings.noQueues")}
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
                  onSaved(t("settings.scheduleSaved", { name: queue.name }));
                }}
                onError={onError}
                t={t}
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
        t={t}
        fmt={fmt}
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
// Appearance: language, theme, close behaviour
// ---------------------------------------------------------------------------

function AppearanceSection({
  preferences,
  onChange,
  onError,
}: {
  preferences: UiPreferences;
  onChange: (preferences: UiPreferences) => void;
  onError: (message: string) => void;
}) {
  const { t } = useI18n();
  const themes: Array<{ value: UiPreferences["theme"]; label: MessageKey }> = [
    { value: "dark", label: "settings.themeDark" },
    { value: "light", label: "settings.themeLight" },
    { value: "system", label: "settings.themeSystem" },
  ];

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("settings.appearance")}</h2>
        <p>{t("settings.appearanceHint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.language")}</strong>
            <span>{t("settings.languageHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__segmented" role="group" aria-label={t("settings.language")}>
              {(["fa", "en"] as const).map((language) => (
                <button
                  key={language}
                  type="button"
                  lang={language}
                  aria-pressed={preferences.language === language}
                  className={
                    preferences.language === language
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() => onChange({ ...preferences, language })}
                >
                  {language === "fa" ? "فارسی" : "English"}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.theme")}</strong>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__segmented" role="group" aria-label={t("settings.theme")}>
              {themes.map((theme) => (
                <button
                  key={theme.value}
                  type="button"
                  aria-pressed={preferences.theme === theme.value}
                  className={
                    preferences.theme === theme.value
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() => onChange({ ...preferences, theme: theme.value })}
                >
                  {t(theme.label)}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.brandTheme")}</strong>
            <span>{t("settings.brandThemeHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__themes" role="group" aria-label={t("settings.brandTheme")}>
              {BRAND_THEMES.map((theme) => (
                <button
                  key={theme}
                  type="button"
                  aria-pressed={preferences.theme === theme}
                  className={
                    preferences.theme === theme
                      ? "settings-page__theme settings-page__theme--active"
                      : "settings-page__theme"
                  }
                  onClick={() => onChange({ ...preferences, theme })}
                >
                  <span
                    className="settings-page__theme-swatch"
                    style={{
                      background: BRAND_THEME_SWATCHES[theme][0],
                      borderColor: BRAND_THEME_SWATCHES[theme][1],
                    }}
                  >
                    <i style={{ background: BRAND_THEME_SWATCHES[theme][1] }} />
                  </span>
                  {t(`settings.theme.${theme}` as MessageKey)}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.closeToTray")}</strong>
            <span>{t("settings.closeToTrayHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <Switch
              id="close-to-tray"
              checked={preferences.closeToTray}
              label={t("settings.closeToTray")}
              onChange={(closeToTray) => onChange({ ...preferences, closeToTray })}
            />
          </div>
        </div>

        <div className="settings-page__row settings-page__row--divided">
          <div className="settings-page__row-label">
            <strong>{t("welcome.reopen")}</strong>
            <span>{t("welcome.reopenHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={() => window.dispatchEvent(new Event(SHOW_WELCOME_EVENT))}
            >
              {t("welcome.reopenButton")}
            </button>
          </div>
        </div>

        <BackendSwitchRow
          id="start-with-windows"
          label={t("settings.startWithWindows")}
          hint={t("settings.startWithWindowsHint")}
          read="get_start_with_windows"
          write="set_start_with_windows"
          divided={false}
          onError={onError}
        />

        <BackendSwitchRow
          id="drop-box"
          label={t("settings.dropBox")}
          hint={t("settings.dropBoxHint")}
          read="get_drop_box"
          write="set_drop_box"
          divided
          onError={onError}
        />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Downloads: folder, speed limit, sleep
// ---------------------------------------------------------------------------

const SPEED_PRESETS_KIB: Array<number | null> = [null, 256, 1024, 5 * 1024];
const CONNECTION_PRESETS = [1, 2, 4, 8, 16, 32];

function DownloadsSection({
  settings,
  onChange,
  onError,
}: {
  settings: DownloadSettings | null;
  onChange: (settings: DownloadSettings) => void;
  onError: (message: string) => void;
}) {
  const { t, fmt } = useI18n();
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
        t("settings.chooseDefaultFolder"),
        settings?.defaultDirectory ?? settings?.systemDirectory,
      );
      if (folder) {
        await run("set_default_download_directory", { directory: folder });
      }
    } catch (reason) {
      onError(t("settings.pickerFailed", { reason: String(reason) }));
    }
  }

  function applyCustom() {
    const kib = Number(customKib);
    if (!customKib.trim() || !Number.isFinite(kib) || kib <= 0) {
      onError(t("settings.speedInvalid"));
      return;
    }
    void run("set_global_speed_limit", { bytesPerSecond: kibPerSecondToBytes(kib) });
  }

  const isPreset = SPEED_PRESETS_KIB.includes(currentKib);

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("settings.downloads")}</h2>
        <p>{t("settings.downloadsHint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.defaultFolder")}</strong>
            <span>{t("settings.defaultFolderHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <code
              className="settings-page__path"
              title={settings?.defaultDirectory ?? settings?.systemDirectory ?? ""}
            >
              {settings?.defaultDirectory ?? settings?.systemDirectory ?? t("settings.systemFolder")}
            </code>
            <button type="button" className="settings-page__secondary-button" onClick={() => void chooseFolder()}>
              <FolderOpen size={14} /> {t("settings.change")}
            </button>
            {settings?.defaultDirectory ? (
              <button
                type="button"
                className="settings-page__icon-button"
                aria-label={t("settings.useSystemFolder")}
                title={t("settings.useSystemFolder")}
                onClick={() => void run("set_default_download_directory", { directory: null })}
              >
                <RotateCcw size={14} />
              </button>
            ) : null}
          </div>
        </div>

        <BackendSwitchRow
          id="keep-server-time"
          label={t("settings.keepServerTime")}
          hint={t("settings.keepServerTimeHint")}
          read="get_keep_server_time"
          write="set_keep_server_time"
          divided={false}
          onError={onError}
        />

        <BackendSwitchRow
          id="finish-sound"
          label={t("settings.finishSound")}
          hint={t("settings.finishSoundHint")}
          read="get_finish_sound"
          write="set_finish_sound"
          divided={false}
          onError={onError}
        />

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.speedLimit")}</strong>
            <span>{t("settings.speedLimitHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--wrap">
            <div className="settings-page__segmented" role="group" aria-label={t("settings.speedLimit")}>
              {SPEED_PRESETS_KIB.map((kib) => (
                <button
                  key={kib ?? "none"}
                  type="button"
                  aria-pressed={kib === currentKib}
                  className={
                    kib === currentKib
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() =>
                    void run("set_global_speed_limit", {
                      bytesPerSecond: kib === null ? null : kibPerSecondToBytes(kib),
                    })
                  }
                >
                  <span className="num">
                    {kib === null ? t("settings.unlimited") : fmt.rate(kibPerSecondToBytes(kib))}
                  </span>
                </button>
              ))}
            </div>
            <label className="settings-page__inline-field">
              <span className={isPreset ? "" : "settings-page__custom-active"}>{t("settings.custom")}</span>
              <input
                id="custom-speed-limit"
                type="number"
                min={1}
                step={1}
                inputMode="numeric"
                dir="ltr"
                value={customKib}
                onChange={(event) => setCustomKib(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyCustom();
                }}
              />
              <span>{t("settings.kibPerSecond")}</span>
            </label>
            <button type="button" className="settings-page__secondary-button" onClick={applyCustom}>
              {t("settings.apply")}
            </button>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.connections")}</strong>
            <span>{t("settings.connectionsHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__segmented" role="group" aria-label={t("settings.connections")}>
              {CONNECTION_PRESETS.map((connections) => (
                <button
                  key={connections}
                  type="button"
                  aria-pressed={settings?.maxConnections === connections}
                  className={
                    settings?.maxConnections === connections
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() => void run("set_max_connections", { connections })}
                >
                  <span className="num">{fmt.number(connections)}</span>
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("settings.keepAwake")}</strong>
            <span>{t("settings.keepAwakeHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <Switch
              id="prevent-sleep"
              checked={settings?.preventSleep ?? true}
              label={t("settings.keepAwake")}
              onChange={(enabled) => void run("set_prevent_sleep", { enabled })}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

const ALL_DAYS = 0b0111_1111;
/** Weekday bits (0 = Sunday) in the order each language's week starts. */
const WEEK_ORDER = { en: [1, 2, 3, 4, 5, 6, 0], fa: [6, 0, 1, 2, 3, 4, 5] } as const;

const COMPLETION_ACTIONS: CompletionAction[] = ["none", "notify", "exit_app", "sleep", "hibernate", "shutdown"];

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
    weekdaysMask: schedule && schedule.weekdaysMask > 0 ? schedule.weekdaysMask : ALL_DAYS,
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
  t,
}: {
  queue: DownloadQueue;
  schedule: QueueSchedule | null;
  onSaved: (schedule: QueueSchedule) => void;
  onError: (message: string) => void;
  t: Translate;
}) {
  const { language } = useI18n();
  const [draft, setDraft] = useState(() => draftFrom(schedule));
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(draftFrom(schedule));
  }, [schedule]);

  const idPrefix = `schedule-${queue.id}`;
  const isPowerAction = ["sleep", "hibernate", "shutdown", "exit_app"].includes(draft.completionAction);

  async function save() {
    const start = parseMinuteOfDay(draft.start);
    const end = parseMinuteOfDay(draft.end);
    if (start === null || end === null) {
      onError(t("settings.timeInvalid"));
      return;
    }
    if (draft.days === "selected" && draft.weekdaysMask === 0) {
      onError(t("settings.dayRequired"));
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
      onError(t("settings.scheduleFailed", { reason: String(reason) }));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="settings-page__group settings-page__schedule-editor">
      <div className="settings-page__schedule-head">
        <Switch
          id={`${idPrefix}-enabled`}
          checked={draft.enabled}
          label={t("settings.scheduleOn", { name: queue.name })}
          onChange={(enabled) => setDraft({ ...draft, enabled })}
        />
        <strong>{queue.name}</strong>
        <span className="settings-page__muted num">
          {draft.enabled
            ? t(draft.days === "every" ? "settings.runsEveryDay" : "settings.runsSelectedDays", {
                start: draft.start,
                end: draft.end,
              })
            : t("settings.notScheduled")}
        </span>
      </div>

      <div className="settings-page__schedule-grid" aria-disabled={!draft.enabled}>
        <label className="settings-page__field">
          <span>{t("settings.from")}</span>
          <input
            id={`${idPrefix}-start`}
            type="time"
            dir="ltr"
            value={draft.start}
            onChange={(event) => setDraft({ ...draft, start: event.target.value })}
          />
        </label>
        <label className="settings-page__field">
          <span>{t("settings.until")}</span>
          <input
            id={`${idPrefix}-end`}
            type="time"
            dir="ltr"
            value={draft.end}
            onChange={(event) => setDraft({ ...draft, end: event.target.value })}
          />
        </label>
        <label className="settings-page__field">
          <span>{t("settings.days")}</span>
          <select
            id={`${idPrefix}-days`}
            value={draft.days}
            onChange={(event) => setDraft({ ...draft, days: event.target.value as ScheduleDraft["days"] })}
          >
            <option value="every">{t("settings.everyDay")}</option>
            <option value="selected">{t("settings.selectedDays")}</option>
          </select>
        </label>
        <label className="settings-page__field">
          <span>{t("settings.whenFinished")}</span>
          <select
            id={`${idPrefix}-action`}
            value={draft.completionAction}
            onChange={(event) =>
              setDraft({ ...draft, completionAction: event.target.value as CompletionAction })
            }
          >
            {COMPLETION_ACTIONS.map((action) => (
              <option key={action} value={action}>
                {t(`settings.action.${action}` as MessageKey)}
              </option>
            ))}
          </select>
        </label>
      </div>

      {draft.days === "selected" ? (
        <div className="settings-page__weekdays" role="group" aria-label={t("settings.days")}>
          {WEEK_ORDER[language].map((day) => {
            const on = (draft.weekdaysMask & (1 << day)) !== 0;
            return (
              <button
                key={day}
                type="button"
                aria-pressed={on}
                className={on ? "settings-page__day settings-page__day--active" : "settings-page__day"}
                onClick={() => setDraft({ ...draft, weekdaysMask: draft.weekdaysMask ^ (1 << day) })}
              >
                {t(`settings.day.${day}` as MessageKey)}
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
            onChange={(event) => setDraft({ ...draft, preventSleep: event.target.checked })}
          />
          {t("settings.queueKeepAwake")}
        </label>
        {isPowerAction ? <span className="settings-page__muted">{t("settings.powerHint")}</span> : null}
        <button type="button" className="settings-page__primary-button" disabled={saving} onClick={() => void save()}>
          {saving ? t("settings.saving") : t("settings.saveSchedule")}
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

function describeRule(rule: DownloadRule, queues: DownloadQueue[], t: Translate, fmt: Formatter): string {
  const when = [
    rule.domain ? t("settings.describe.from", { domain: rule.domain }) : null,
    rule.extension ? t("settings.describe.extension", { extension: rule.extension }) : null,
    rule.urlPattern ? t("settings.describe.url", { pattern: rule.urlPattern }) : null,
    rule.minSize ? t("settings.describe.over", { size: fmt.bytes(rule.minSize) }) : null,
  ].filter(Boolean);
  const then = [
    rule.destinationDirectory ? t("settings.describe.saveTo", { folder: rule.destinationDirectory }) : null,
    rule.queueId
      ? t("settings.describe.queue", {
          queue: queues.find((queue) => queue.id === rule.queueId)?.name ?? t("settings.describe.aQueue"),
        })
      : null,
    rule.priority
      ? t("settings.describe.priority", { priority: t(`priority.${rule.priority}` as MessageKey) })
      : null,
    rule.maxConnections ? t("settings.describe.connections", { count: fmt.number(rule.maxConnections) }) : null,
    rule.speedCap ? t("settings.describe.limit", { rate: fmt.rate(rule.speedCap) ?? "" }) : null,
  ].filter(Boolean);
  const separator = fmt.language === "fa" ? "، " : ", ";
  const arrow = fmt.language === "fa" ? " ← " : " → ";
  return `${when.join(separator) || t("settings.describe.any")}${arrow}${then.join(separator) || t("settings.describe.nothing")}`;
}

function RulesSection({
  rules,
  categories,
  queues,
  onRulesChange,
  onError,
  onSaved,
  t,
  fmt,
}: {
  rules: DownloadRule[];
  categories: DownloadCategory[];
  queues: DownloadQueue[];
  onRulesChange: (rules: DownloadRule[]) => void;
  onError: (message: string) => void;
  onSaved: (message: string) => void;
  t: Translate;
  fmt: Formatter;
}) {
  const [draft, setDraft] = useState<RuleDraft | null>(null);

  async function save() {
    if (!draft) return;
    if (!draft.name.trim()) {
      onError(t("settings.ruleNameRequired"));
      return;
    }
    const index = rules.findIndex((rule) => rule.id === draft.id);
    const sortOrder = index >= 0 ? rules[index].sortOrder : rules.length;
    try {
      const saved = await invoke<DownloadRule>("save_download_rule", {
        rule: draftToRule(draft, sortOrder),
      });
      onRulesChange(index >= 0 ? rules.map((rule) => (rule.id === saved.id ? saved : rule)) : [...rules, saved]);
      setDraft(null);
      onSaved(t("settings.ruleSaved", { name: saved.name }));
    } catch (reason) {
      onError(t("settings.ruleFailed", { reason: String(reason) }));
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
      const folder = await pickFolder(t("settings.ruleFolderTitle"), draft.destinationDirectory || null);
      if (folder) setDraft({ ...draft, destinationDirectory: folder });
    } catch (reason) {
      onError(t("settings.pickerFailed", { reason: String(reason) }));
    }
  }

  const field = (key: keyof RuleDraft, ltr = false) => ({
    id: `rule-${key}`,
    dir: ltr ? "ltr" : undefined,
    value: draft ? String(draft[key]) : "",
    onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      draft && setDraft({ ...draft, [key]: event.target.value }),
  });

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading settings-page__section-heading--row">
        <div>
          <h2>{t("settings.rules")}</h2>
          <p>{t("settings.rulesHint", { count: fmt.number(categories.length) })}</p>
        </div>
        <button type="button" className="settings-page__secondary-button" onClick={() => setDraft({ ...EMPTY_RULE })}>
          <Plus size={14} /> {t("settings.newRule")}
        </button>
      </div>

      {rules.length === 0 && !draft ? <div className="settings-page__hint">{t("settings.rulesEmpty")}</div> : null}

      {rules.length > 0 ? (
        <ul className="settings-page__rule-list">
          {rules.map((rule) => (
            <li key={rule.id} className="settings-page__rule">
              <Switch
                id={`rule-enabled-${rule.id}`}
                checked={rule.enabled}
                label={t("settings.enableRule", { name: rule.name })}
                onChange={() => void toggle(rule)}
              />
              <button type="button" className="settings-page__rule-body" onClick={() => setDraft(ruleToDraft(rule))}>
                <strong>{rule.name}</strong>
                <span>{describeRule(rule, queues, t, fmt)}</span>
              </button>
              <button
                type="button"
                className="settings-page__icon-button"
                aria-label={t("settings.deleteRule", { name: rule.name })}
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
            <span>{t("settings.ruleName")}</span>
            <input {...field("name")} placeholder={t("settings.ruleNamePlaceholder")} />
          </label>

          <fieldset className="settings-page__fieldset">
            <legend>{t("settings.ruleWhen")}</legend>
            <label className="settings-page__field">
              <span>{t("settings.ruleDomain")}</span>
              <input {...field("domain", true)} placeholder="example.com" />
            </label>
            <label className="settings-page__field">
              <span>{t("settings.ruleExtension")}</span>
              <input {...field("extension", true)} placeholder="iso" />
            </label>
            <label className="settings-page__field">
              <span>{t("settings.ruleUrl")}</span>
              <input {...field("urlPattern", true)} placeholder="*/lectures/*" />
            </label>
            <label className="settings-page__field">
              <span>{t("settings.ruleMinSize")}</span>
              <input {...field("minSizeMb", true)} type="number" min={1} />
            </label>
          </fieldset>

          <fieldset className="settings-page__fieldset">
            <legend>{t("settings.ruleThen")}</legend>
            <label className="settings-page__field settings-page__field--wide">
              <span>{t("settings.ruleFolder")}</span>
              <div className="settings-page__input-with-button">
                <input {...field("destinationDirectory", true)} placeholder={t("settings.ruleFolderPlaceholder")} />
                <button type="button" className="settings-page__secondary-button" onClick={() => void chooseRuleFolder()}>
                  <FolderOpen size={14} /> {t("settings.choose")}
                </button>
              </div>
            </label>
            <label className="settings-page__field">
              <span>{t("settings.ruleQueue")}</span>
              <select {...field("queueId")}>
                <option value="">{t("settings.ruleNoQueue")}</option>
                {queues.map((queue) => (
                  <option key={queue.id} value={queue.id}>
                    {queue.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="settings-page__field">
              <span>{t("settings.rulePriority")}</span>
              <select {...field("priority")}>
                <option value="">{t("settings.ruleUnchanged")}</option>
                {(["very_high", "high", "normal", "low"] as const).map((priority) => (
                  <option key={priority} value={priority}>
                    {t(`priority.${priority}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="settings-page__field">
              <span>{t("settings.ruleConnections")}</span>
              <input {...field("maxConnections", true)} type="number" min={1} max={64} placeholder={t("settings.ruleAutomatic")} />
            </label>
            <label className="settings-page__field">
              <span>{t("settings.ruleSpeed")}</span>
              <input {...field("speedCapKib", true)} type="number" min={1} placeholder={t("settings.ruleNoLimit")} />
            </label>
          </fieldset>

          <div className="settings-page__schedule-foot">
            <button type="button" className="settings-page__secondary-button" onClick={() => setDraft(null)}>
              {t("settings.cancel")}
            </button>
            <button type="button" className="settings-page__primary-button" onClick={() => void save()}>
              {t("settings.saveRule")}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** Whether a download link copied anywhere opens the Add download dialog. */
export function ClipboardWatchRow({ onError }: { onError: (message: string) => void }) {
  const { t } = useI18n();
  return (
    <BackendSwitchRow
      id="clipboard-watch"
      label={t("settings.clipboardWatch")}
      hint={t("settings.clipboardWatchHint")}
      read="get_clipboard_watch"
      write="set_clipboard_watch"
      onError={onError}
    />
  );
}

/** Where a download from the browser or the clipboard shows up. */
export function IntakeWindowRow({ onError }: { onError: (message: string) => void }) {
  const { t } = useI18n();
  const [value, setValue] = useState<"compact" | "main" | null>(null);

  useEffect(() => {
    invoke<"compact" | "main">("get_intake_window")
      .then(setValue)
      .catch(() => {});
  }, []);

  if (value === null) return null;

  const choose = (next: "compact" | "main") => {
    const previous = value;
    setValue(next);
    invoke<"compact" | "main">("set_intake_window", { value: next })
      .then(setValue)
      .catch((reason) => {
        setValue(previous);
        onError(String(reason));
      });
  };

  return (
    <div className="settings-page__row settings-page__row--divided">
      <div className="settings-page__row-label">
        <strong>{t("settings.intakeWindow")}</strong>
        <span>{t("settings.intakeWindowHint")}</span>
      </div>
      <div className="settings-page__row-control">
        <div className="settings-page__segmented" role="group" aria-label={t("settings.intakeWindow")}>
          {(["compact", "main"] as const).map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={value === option}
              className={
                value === option
                  ? "settings-page__segment settings-page__segment--active"
                  : "settings-page__segment"
              }
              onClick={() => choose(option)}
            >
              {option === "compact" ? t("settings.intakeCompact") : t("settings.intakeMain")}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/** An on/off setting kept by the application, not in the preferences. */
export function BackendSwitchRow({
  id,
  label,
  hint,
  read,
  write,
  divided = true,
  onError,
}: {
  id: string;
  label: string;
  hint: string;
  read: string;
  write: string;
  divided?: boolean;
  onError?: (message: string) => void;
}) {
  const [enabled, setEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<boolean | null>(read)
      .then((value) => {
        if (!cancelled) setEnabled(value);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [read]);

  if (enabled === null) return null;

  return (
    <div className={`settings-page__row ${divided ? "settings-page__row--divided" : ""}`}>
      <div className="settings-page__row-label">
        <strong>{label}</strong>
        <span>{hint}</span>
      </div>
      <div className="settings-page__row-control">
        <Switch
          id={id}
          checked={enabled}
          label={label}
          onChange={(next) => {
            setEnabled(next);
            invoke<boolean>(write, { enabled: next })
              .then(setEnabled)
              .catch((reason) => {
                setEnabled(!next);
                onError?.(String(reason));
              });
          }}
        />
      </div>
    </div>
  );
}
