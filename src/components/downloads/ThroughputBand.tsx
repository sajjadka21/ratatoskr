import { Gauge, Pause, Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import { Sparkline } from "../common/Sparkline";

import "./ThroughputBand.css";

const KIB = 1024;
const LIMIT_PRESETS: Array<number | null> = [null, 256 * KIB, 1024 * KIB, 2048 * KIB, 5120 * KIB];

type ThroughputBandProps = {
  bytesPerSecond: number;
  history: number[];
  activeCount: number;
  queuedCount: number;
  etaSeconds: number | null;
  resumableCount: number;
  speedLimit: number | null;
  onSetSpeedLimit: (limit: number | null) => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
};

/**
 * The top band of the download list: total speed as a big number, the last
 * minute as a live area chart, and the controls that act on everything.
 */
export function ThroughputBand({
  bytesPerSecond,
  history,
  activeCount,
  queuedCount,
  etaSeconds,
  resumableCount,
  speedLimit,
  onSetSpeedLimit,
  onPauseAll,
  onResumeAll,
}: ThroughputBandProps) {
  const { t, fmt } = useI18n();
  const chartRef = useRef<HTMLDivElement>(null);
  const [chartWidth, setChartWidth] = useState(600);

  useEffect(() => {
    const element = chartRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) =>
      setChartWidth(Math.max(120, Math.round(entry.contentRect.width))),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const moving = activeCount > 0;
  const rate = fmt.rate(bytesPerSecond);
  const [value, unit] = moving && rate ? splitValue(rate) : [t("band.idle"), ""];
  const eta = fmt.duration(etaSeconds);
  const presets = speedLimit !== null && !LIMIT_PRESETS.includes(speedLimit)
    ? [...LIMIT_PRESETS, speedLimit]
    : LIMIT_PRESETS;

  return (
    <section className={`throughput-band ${moving ? "throughput-band--moving" : ""}`} aria-label={t("band.label")}>
      <div className="throughput-band__figure">
        <span className="throughput-band__caption">{t("band.speedNow")}</span>
        <div className="throughput-band__value">
          <strong className="num">{value}</strong>
          {unit ? <span>{unit}</span> : null}
        </div>
        <span className="throughput-band__summary num">
          {t(eta && moving ? "band.summaryEta" : "band.summary", {
            active: fmt.number(activeCount),
            queued: fmt.number(queuedCount),
            eta: eta ?? "",
          })}
        </span>
      </div>

      <div className="throughput-band__chart" ref={chartRef}>
        <Sparkline values={history} width={chartWidth} height={84} area floor={64 * KIB} />
      </div>

      <div className="throughput-band__controls">
        <label className="throughput-band__limit">
          <Gauge size={15} aria-hidden="true" />
          <span className="visually-hidden">{t("band.limit")}</span>
          <select
            id="global-speed-limit"
            value={speedLimit ?? ""}
            onChange={(event) =>
              onSetSpeedLimit(event.target.value === "" ? null : Number(event.target.value))
            }
          >
            {presets.map((preset) => (
              <option key={preset ?? "none"} value={preset ?? ""}>
                {preset === null ? t("band.unlimited") : t("band.limitedTo", { rate: fmt.rate(preset) ?? "" })}
              </option>
            ))}
          </select>
        </label>
        <div className="throughput-band__buttons">
          <button type="button" onClick={onPauseAll} disabled={activeCount === 0}>
            <Pause size={14} aria-hidden="true" />
            {t("band.pauseAll")}
          </button>
          <button type="button" onClick={onResumeAll} disabled={resumableCount === 0}>
            <Play size={14} aria-hidden="true" />
            {t("band.resumeAll")}
          </button>
        </div>
      </div>
    </section>
  );
}

/** "11.4 MB/s" → ["11.4", "MB/s"] so the number can be set large. */
function splitValue(text: string): [string, string] {
  const index = text.indexOf(" ");
  return index < 0 ? [text, ""] : [text.slice(0, index), text.slice(index + 1)];
}
