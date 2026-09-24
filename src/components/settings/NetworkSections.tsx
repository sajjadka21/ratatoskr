import { CalendarClock, Globe2, Save } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";
import type { NetworkSettings, ProxyMode, TrafficSummary } from "../../types/download";
import {
  bytesToGigabytes,
  domesticShare,
  gigabytesToBytes,
  localDay,
  quotaState,
} from "../../utils/traffic";
import { Switch } from "./Switch";

const MODES: Array<{ value: ProxyMode; label: MessageKey }> = [
  { value: "off", label: "network.modeOff" },
  { value: "system", label: "network.modeSystem" },
  { value: "manual", label: "network.modeManual" },
];

/** Proxy route, direct exceptions, and whether Iranian sites skip the proxy. */
export function NetworkSection({
  onError,
  onSaved,
}: {
  onError: (message: string) => void;
  onSaved: (message: string) => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<NetworkSettings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<NetworkSettings>("get_network_settings")
      .then((settings) => {
        if (!cancelled) setDraft(settings);
      })
      .catch((reason: unknown) => {
        if (!cancelled) onError(String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [onError]);

  if (!draft) return null;

  async function save() {
    if (!draft) return;
    setSaving(true);
    try {
      setDraft(await invoke<NetworkSettings>("set_network_settings", { settings: draft }));
      onSaved(t("network.saved"));
    } catch (reason) {
      onError(String(reason));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("network.title")}</h2>
        <p>{t("network.hint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("network.mode")}</strong>
          </div>
          <div className="settings-page__row-control">
            <div className="settings-page__segmented" role="group" aria-label={t("network.mode")}>
              {MODES.map((mode) => (
                <button
                  key={mode.value}
                  type="button"
                  aria-pressed={draft.mode === mode.value}
                  className={
                    draft.mode === mode.value
                      ? "settings-page__segment settings-page__segment--active"
                      : "settings-page__segment"
                  }
                  onClick={() => setDraft({ ...draft, mode: mode.value })}
                >
                  {t(mode.label)}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <label htmlFor="proxy-url">
              <strong>{t("network.proxyUrl")}</strong>
            </label>
            <span>{t("network.proxyUrlHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <input
              id="proxy-url"
              className="settings-page__text-input"
              dir="ltr"
              spellCheck={false}
              placeholder="socks5://127.0.0.1:10808"
              disabled={draft.mode !== "manual"}
              value={draft.proxyUrl ?? ""}
              onChange={(event) => setDraft({ ...draft, proxyUrl: event.target.value })}
            />
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("network.domesticDirect")}</strong>
            <span>{t("network.domesticDirectHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <Switch
              id="domestic-direct"
              checked={draft.domesticDirect}
              label={t("network.domesticDirect")}
              onChange={(domesticDirect) => setDraft({ ...draft, domesticDirect })}
            />
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <label htmlFor="direct-hosts">
              <strong>{t("network.directHosts")}</strong>
            </label>
            <span>{t("network.directHostsHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <textarea
              id="direct-hosts"
              className="settings-page__textarea"
              dir="ltr"
              rows={3}
              spellCheck={false}
              placeholder={"nas.local\nintranet.example"}
              value={draft.directHosts}
              onChange={(event) => setDraft({ ...draft, directHosts: event.target.value })}
            />
          </div>
        </div>
      </div>

      <div className="settings-page__actions">
        <button
          type="button"
          className="settings-page__primary-button"
          disabled={saving}
          onClick={() => void save()}
        >
          <Save size={14} /> {t("network.save")}
        </button>
      </div>
    </div>
  );
}

/** Domestic and international usage, the quota and its counting period. */
export function TrafficSection({
  onError,
  onSaved,
  onSummary,
}: {
  onError: (message: string) => void;
  onSaved: (message: string) => void;
  onSummary?: (summary: TrafficSummary) => void;
}) {
  const { t, fmt } = useI18n();
  const [summary, setSummary] = useState<TrafficSummary | null>(null);
  const [quotaText, setQuotaText] = useState("");
  const [network, setNetwork] = useState<NetworkSettings | null>(null);
  const [domesticHosts, setDomesticHosts] = useState("");

  const accept = useCallback(
    (next: TrafficSummary) => {
      setSummary(next);
      setQuotaText(bytesToGigabytes(next.internationalQuota));
      onSummary?.(next);
    },
    [onSummary],
  );

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<TrafficSummary>("get_traffic_summary"),
      invoke<NetworkSettings>("get_network_settings"),
    ])
      .then(([nextSummary, nextNetwork]) => {
        if (cancelled) return;
        accept(nextSummary);
        setNetwork(nextNetwork);
        setDomesticHosts(nextNetwork.domesticHosts);
      })
      .catch((reason: unknown) => {
        if (!cancelled) onError(String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [accept, onError]);

  if (!summary) return null;

  async function setQuota(quota: number | null, periodStart: string | null) {
    try {
      accept(await invoke<TrafficSummary>("set_traffic_quota", { quota, periodStart }));
    } catch (reason) {
      onError(String(reason));
    }
  }

  function applyQuota() {
    const bytes = gigabytesToBytes(quotaText);
    if (bytes === null) {
      onError(t("traffic.quotaInvalid"));
      return;
    }
    void setQuota(bytes, summary!.explicitPeriod ? summary!.periodStart : null);
  }

  async function saveDomesticHosts() {
    if (!network) return;
    try {
      const saved = await invoke<NetworkSettings>("set_network_settings", {
        settings: { ...network, domesticHosts },
      });
      setNetwork(saved);
      setDomesticHosts(saved.domesticHosts);
      onSaved(t("network.saved"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  const quota = quotaState(summary);
  const share = domesticShare(summary.periodDomesticBytes, summary.periodInternationalBytes);
  const periodLabel = summary.explicitPeriod
    ? t("traffic.periodSince", { date: fmt.day(summary.periodStart) })
    : t("traffic.periodRolling");
  const bytes = (value: number) => fmt.bytes(value);

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("traffic.title")}</h2>
        <p>{t("traffic.hint")}</p>
      </div>

      <div className="settings-page__group traffic-meter">
        <div className="traffic-meter__head">
          <span className="traffic-meter__period">
            <CalendarClock size={14} aria-hidden="true" /> {periodLabel}
          </span>
        </div>

        <div className="traffic-meter__figures">
          <div className="traffic-meter__figure traffic-meter__figure--domestic">
            <span>{t("traffic.domestic")}</span>
            <strong className="num">{bytes(summary.periodDomesticBytes)}</strong>
          </div>
          <div className="traffic-meter__figure traffic-meter__figure--international">
            <span>{t("traffic.international")}</span>
            <strong className="num">{bytes(summary.periodInternationalBytes)}</strong>
          </div>
        </div>

        <div
          className="traffic-meter__split"
          role="img"
          aria-label={`${t("traffic.domestic")} ${bytes(summary.periodDomesticBytes)}, ${t("traffic.international")} ${bytes(summary.periodInternationalBytes)}`}
        >
          <span style={{ inlineSize: `${share * 100}%` }} />
        </div>

        {quota ? (
          <div className={`traffic-meter__quota traffic-meter__quota--${quota.level}`}>
            <div className="traffic-meter__quota-text">
              <span className="num">
                {t("traffic.quotaUsed", { used: bytes(quota.used), quota: bytes(quota.quota) })}
              </span>
              <span className="num">
                {quota.level === "reached"
                  ? t("traffic.quotaReached")
                  : t("traffic.quotaLeft", { left: bytes(quota.left) })}
              </span>
            </div>
            <div className="traffic-meter__bar">
              <span style={{ inlineSize: `${quota.ratio * 100}%` }} />
            </div>
          </div>
        ) : null}

        <dl className="traffic-meter__small">
          <div>
            <dt>{t("traffic.today")}</dt>
            <dd className="num">
              {t("traffic.split", {
                domestic: bytes(summary.todayDomesticBytes),
                international: bytes(summary.todayInternationalBytes),
              })}
            </dd>
          </div>
          <div>
            <dt>{t("traffic.month")}</dt>
            <dd className="num">
              {t("traffic.split", {
                domestic: bytes(summary.monthDomesticBytes),
                international: bytes(summary.monthInternationalBytes),
              })}
            </dd>
          </div>
        </dl>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("traffic.quota")}</strong>
            <span>{t("traffic.quotaHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--wrap">
            <label className="settings-page__inline-field">
              <input
                id="international-quota"
                inputMode="decimal"
                dir="ltr"
                placeholder="20"
                value={quotaText}
                onChange={(event) => setQuotaText(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyQuota();
                }}
              />
              <span>{t("traffic.gb")}</span>
            </label>
            <button type="button" className="settings-page__secondary-button" onClick={applyQuota}>
              {t("traffic.apply")}
            </button>
            {summary.internationalQuota ? (
              <button
                type="button"
                className="settings-page__secondary-button"
                onClick={() => void setQuota(null, summary.explicitPeriod ? summary.periodStart : null)}
              >
                {t("traffic.removeQuota")}
              </button>
            ) : (
              <span className="settings-page__muted">{t("traffic.quotaNone")}</span>
            )}
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("traffic.period")}</strong>
            <span>{t("traffic.periodHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--wrap">
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={() => void setQuota(summary.internationalQuota, localDay())}
            >
              {t("traffic.newPeriod")}
            </button>
            {summary.explicitPeriod ? (
              <button
                type="button"
                className="settings-page__secondary-button"
                onClick={() => void setQuota(summary.internationalQuota, null)}
              >
                {t("traffic.rolling")}
              </button>
            ) : null}
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <label htmlFor="domestic-hosts">
              <strong>{t("traffic.domesticHosts")}</strong>
            </label>
            <span>{t("traffic.domesticHostsHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <textarea
              id="domestic-hosts"
              className="settings-page__textarea"
              dir="ltr"
              rows={3}
              spellCheck={false}
              placeholder={"aparat.com\narvancloud.com"}
              value={domesticHosts}
              onChange={(event) => setDomesticHosts(event.target.value)}
            />
            <button
              type="button"
              className="settings-page__secondary-button"
              onClick={() => void saveDomesticHosts()}
            >
              <Globe2 size={14} /> {t("traffic.saveHosts")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
