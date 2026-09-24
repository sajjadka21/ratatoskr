import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, Download, Globe2, RotateCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";
import type { DownloadStats } from "../../types/download";
import { chartScale, kindTotals, labelledDays } from "../../utils/stats";

import "./StatsPage.css";

const PERIODS = [7, 30, 90] as const;

/** What was downloaded, when and from where. Every figure is the engine's. */
export function StatsPage({ onError }: { onError: (message: string) => void }) {
  const { t, fmt } = useI18n();
  const [days, setDays] = useState<(typeof PERIODS)[number]>(30);
  const [stats, setStats] = useState<DownloadStats | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    invoke<DownloadStats>("get_download_stats", { days })
      .then((found) => {
        if (!cancelled) setStats(found);
      })
      .catch((reason: unknown) => {
        if (!cancelled) onError(String(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [days, onError]);

  const kinds = useMemo(() => kindTotals(stats?.extensions ?? []), [stats]);

  if (!stats) {
    return <section className="stats-page stats-page--loading" aria-busy="true" />;
  }

  const periodBytes = stats.periodDomesticBytes + stats.periodInternationalBytes;
  const domesticShare = periodBytes > 0 ? (stats.periodDomesticBytes / periodBytes) * 100 : null;

  return (
    <section className="stats-page" aria-busy={loading}>
      <div className="stats-page__intro">
        <p>{t("stats.hint")}</p>
        <div className="stats-page__periods" role="group" aria-label={t("stats.period")}>
          {PERIODS.map((period) => (
            <button
              key={period}
              type="button"
              aria-pressed={days === period}
              className={days === period ? "stats-page__period stats-page__period--active" : "stats-page__period"}
              onClick={() => setDays(period)}
            >
              {t("stats.days", { count: fmt.number(period) })}
            </button>
          ))}
          {loading ? <RotateCw size={14} className="stats-page__spinner" aria-hidden="true" /> : null}
        </div>
      </div>

      <div className="stats-kpis">
        <Kpi
          icon={<CheckCircle2 size={16} />}
          label={t("stats.completed")}
          value={fmt.number(stats.periodCompleted)}
          hint={t("stats.completedAll", { count: fmt.number(stats.allCompleted) })}
        />
        <Kpi
          icon={<Download size={16} />}
          label={t("stats.downloaded")}
          value={fmt.bytes(periodBytes)}
          hint={t("stats.downloadedHint")}
        />
        <Kpi
          icon={<Globe2 size={16} />}
          label={t("stats.domesticShare")}
          value={domesticShare === null ? "—" : fmt.percent(domesticShare)}
          hint={t("stats.domesticShareHint", { bytes: fmt.bytes(stats.periodInternationalBytes) })}
        />
        <Kpi
          icon={<AlertTriangle size={16} />}
          label={t("stats.attention")}
          value={fmt.number(stats.failed)}
          hint={t("stats.attentionHint", { active: fmt.number(stats.active) })}
          tone={stats.failed > 0 ? "warn" : undefined}
        />
      </div>

      <DailyChart stats={stats} />

      <div className="stats-page__grid">
        <div className="stats-card">
          <h3>{t("stats.hosts")}</h3>
          {stats.topHosts.length ? (
            <RankedBars
              rows={stats.topHosts.map((host) => ({
                key: host.name || "other",
                label: host.name ? <bdi className="ltr">{host.name}</bdi> : t("stats.otherHosts"),
                bytes: host.bytes,
                count: host.count,
              }))}
            />
          ) : (
            <p className="stats-card__empty">{t("stats.nothing")}</p>
          )}
        </div>

        <div className="stats-card">
          <h3>{t("stats.kinds")}</h3>
          {kinds.length ? (
            <RankedBars
              rows={kinds.map((kind) => ({
                key: kind.name,
                label: t(`stats.kind.${kind.name}` as MessageKey),
                bytes: kind.bytes,
                count: kind.count,
              }))}
            />
          ) : (
            <p className="stats-card__empty">{t("stats.nothing")}</p>
          )}
          {stats.largest ? (
            <p className="stats-card__largest">
              <span>{t("stats.largest")}</span>
              <bdi title={stats.largest.name}>{stats.largest.name}</bdi>
              <strong className="num">{fmt.bytes(stats.largest.bytes)}</strong>
            </p>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function Kpi({
  icon,
  label,
  value,
  hint,
  tone,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  hint: string;
  tone?: "warn";
}) {
  return (
    <div className={tone ? `stats-kpi stats-kpi--${tone}` : "stats-kpi"}>
      <span className="stats-kpi__label">
        {icon}
        {label}
      </span>
      <strong className="stats-kpi__value num">{value}</strong>
      <span className="stats-kpi__hint">{hint}</span>
    </div>
  );
}

/** Stacked daily bars: domestic at the base, international above. */
function DailyChart({ stats }: { stats: DownloadStats }) {
  const { t, fmt } = useI18n();
  const [hovered, setHovered] = useState<number | null>(null);
  const scale = useMemo(() => chartScale(stats.days), [stats.days]);
  const labelled = useMemo(() => labelledDays(stats.days.length), [stats.days.length]);
  const count = stats.days.length;
  const active = hovered === null ? null : stats.days[hovered];

  return (
    <div className="stats-card stats-card--chart">
      <div className="stats-card__head">
        <h3>{t("stats.daily")}</h3>
        <ul className="stats-legend">
          <li>
            <span className="stats-legend__swatch stats-legend__swatch--domestic" aria-hidden="true" />
            {t("stats.domestic")}
          </li>
          <li>
            <span className="stats-legend__swatch stats-legend__swatch--international" aria-hidden="true" />
            {t("stats.international")}
          </li>
        </ul>
      </div>

      {scale.max === 0 ? (
        <p className="stats-card__empty stats-card__empty--chart">{t("stats.empty")}</p>
      ) : (
        <div className="stats-chart" onMouseLeave={() => setHovered(null)}>
          <div className="stats-chart__plot">
            {scale.ticks.map((tick) => (
              <div key={tick} className="stats-chart__tick" style={{ bottom: `${(tick / scale.max) * 100}%` }}>
                <span className="num">{fmt.bytes(tick)}</span>
              </div>
            ))}
            <div className="stats-chart__bars" style={{ gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))` }}>
              {stats.days.map((day, index) => {
                const total = day.domesticBytes + day.internationalBytes;
                const label = `${fmt.day(day.day)}: ${fmt.bytes(total)}`;
                return (
                  <button
                    key={day.day}
                    type="button"
                    className={hovered === index ? "stats-chart__day stats-chart__day--hovered" : "stats-chart__day"}
                    aria-label={label}
                    onMouseEnter={() => setHovered(index)}
                    onFocus={() => setHovered(index)}
                    onBlur={() => setHovered(null)}
                  >
                    {total > 0 ? (
                      <span className="stats-chart__stack" style={{ height: `${(total / scale.max) * 100}%` }}>
                        {day.internationalBytes > 0 ? (
                          <span
                            className="stats-chart__segment stats-chart__segment--international"
                            style={{ flexGrow: day.internationalBytes }}
                          />
                        ) : null}
                        {day.domesticBytes > 0 ? (
                          <span
                            className="stats-chart__segment stats-chart__segment--domestic"
                            style={{ flexGrow: day.domesticBytes }}
                          />
                        ) : null}
                      </span>
                    ) : null}
                  </button>
                );
              })}
            </div>

            {active && hovered !== null ? (
              <div
                className="stats-tooltip"
                role="status"
                style={
                  // Grows toward the roomier side, in either direction.
                  hovered < count / 2
                    ? { insetInlineStart: `calc(${((hovered + 1) / count) * 100}% + 6px)` }
                    : { insetInlineEnd: `calc(${((count - hovered) / count) * 100}% + 6px)` }
                }
              >
                <strong>{fmt.day(active.day)}</strong>
                <span>
                  <i className="stats-legend__swatch stats-legend__swatch--domestic" aria-hidden="true" />
                  {t("stats.domestic")} <b className="num">{fmt.bytes(active.domesticBytes)}</b>
                </span>
                <span>
                  <i className="stats-legend__swatch stats-legend__swatch--international" aria-hidden="true" />
                  {t("stats.international")} <b className="num">{fmt.bytes(active.internationalBytes)}</b>
                </span>
                <span className="stats-tooltip__files">{t("stats.dayFiles", { count: fmt.number(active.completed) })}</span>
              </div>
            ) : null}
          </div>

          <div className="stats-chart__labels" style={{ gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))` }}>
            {stats.days.map((day, index) => (
              <span key={day.day} className="num">
                {labelled.has(index) ? fmt.shortDay(day.day) : ""}
              </span>
            ))}
          </div>
        </div>
      )}

      <details className="stats-table">
        <summary>{t("stats.showTable")}</summary>
        <table>
          <thead>
            <tr>
              <th>{t("stats.day")}</th>
              <th>{t("stats.domestic")}</th>
              <th>{t("stats.international")}</th>
              <th>{t("stats.completed")}</th>
            </tr>
          </thead>
          <tbody>
            {[...stats.days].reverse().map((day) => (
              <tr key={day.day}>
                <td>{fmt.day(day.day)}</td>
                <td className="num">{fmt.bytes(day.domesticBytes)}</td>
                <td className="num">{fmt.bytes(day.internationalBytes)}</td>
                <td className="num">{fmt.number(day.completed)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}

function RankedBars({
  rows,
}: {
  rows: Array<{ key: string; label: React.ReactNode; bytes: number; count: number }>;
}) {
  const { t, fmt } = useI18n();
  const largest = Math.max(1, ...rows.map((row) => row.bytes));
  return (
    <ol className="stats-ranked">
      {rows.map((row) => (
        <li key={row.key}>
          <span className="stats-ranked__label">{row.label}</span>
          <span className="stats-ranked__value num">
            {fmt.bytes(row.bytes)} · {t("stats.files", { count: fmt.number(row.count) })}
          </span>
          <span className="stats-ranked__track" aria-hidden="true">
            <span style={{ width: `${Math.max(2, (row.bytes / largest) * 100)}%` }} />
          </span>
        </li>
      ))}
    </ol>
  );
}
