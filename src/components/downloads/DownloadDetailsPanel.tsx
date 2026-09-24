import {
  Check,
  Copy,
  ExternalLink,
  FolderOpen,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  X,
  XCircle,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import { engineReasonText, noticeText } from "../../utils/notices";
import type { MessageKey } from "../../i18n/messages";
import type { DownloadListItem, TaskAction, TransferMetrics } from "../../types/download";
import { displayName } from "../../utils/fileKind";
import { formatHost } from "../../utils/format";
import { availableActions } from "../../utils/taskActions";
import { FileBadge } from "../common/FileBadge";
import { Sparkline } from "../common/Sparkline";
import { ChecksSection } from "./ChecksSection";
import { MirrorsEditor } from "./MirrorsEditor";

import "./DownloadDetailsPanel.css";

export type ActivityEntry = {
  at: number;
  kind: "status" | "engine";
  status?: string;
  text?: string;
};

type DownloadDetailsPanelProps = {
  item: DownloadListItem | null;
  metrics?: TransferMetrics;
  history?: number[];
  activity: ActivityEntry[];
  queueName?: string;
  onClose: () => void;
  onAction: (item: DownloadListItem, action: TaskAction) => void;
  onRefreshSource: (item: DownloadListItem) => void;
  onError: (message: string) => void;
};

const ACTION_ICONS = {
  start: Play,
  resume: Play,
  pause: Pause,
  retry: RotateCcw,
  cancel: XCircle,
  restart: RefreshCw,
} as const;

const RING = 2 * Math.PI * 30;

export function DownloadDetailsPanel({
  item,
  metrics,
  history,
  activity,
  queueName,
  onClose,
  onAction,
  onRefreshSource,
  onError,
}: DownloadDetailsPanelProps) {
  const { t, fmt, language } = useI18n();
  const [copied, setCopied] = useState<"url" | "path" | null>(null);
  const [ruleExplanation, setRuleExplanation] = useState<string | null>(null);

  useEffect(() => {
    setCopied(null);
    setRuleExplanation(null);
    if (!item) return;
    let cancelled = false;
    void invoke<string | null>("get_download_rule_explanation", { id: item.id })
      .then((value) => !cancelled && setRuleExplanation(value))
      .catch(() => !cancelled && setRuleExplanation(null));
    return () => {
      cancelled = true;
    };
  }, [item?.id]);

  if (!item) return null;

  const current = item;
  const status = current.status.toLowerCase();
  const transferring = status === "downloading";
  const hasFile = Boolean(current.destinationPath) && status === "completed";
  const percent =
    current.totalBytes && current.totalBytes > 0
      ? Math.min(100, (current.downloadedBytes / current.totalBytes) * 100)
      : status === "completed"
        ? 100
        : 0;
  const rate = transferring ? fmt.rate(metrics?.bytesPerSecond ?? null) : null;
  const remaining = transferring ? fmt.duration(metrics?.etaSeconds ?? null) : null;
  const connections = transferring ? metrics?.activeConnections ?? null : null;
  const maxConnections = metrics?.maxConnections ?? connections;

  async function copy(value: string, field: "url" | "path") {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(field);
      window.setTimeout(() => setCopied((now) => (now === field ? null : now)), 1400);
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function run(command: "open_download_file" | "reveal_download_file") {
    try {
      await invoke(command, { id: current.id });
    } catch (reason) {
      onError(
        t(command === "open_download_file" ? "toast.openFailed" : "toast.revealFailed", {
          reason: String(reason),
        }),
      );
    }
  }

  const canRefresh = ["created", "queued", "paused", "failed", "cancelled"].includes(status);

  return (
    <aside className="details" aria-label={t("details.label")}>
      <header className="details__header">
        <FileBadge item={current} size="lg" />
        <div className="details__identity">
          <strong className="ltr" title={displayName(current)}>
            {displayName(current)}
          </strong>
          <span>
            <span className="ltr">{formatHost(current.resolvedUrl ?? current.sourceUrl)}</span>
            {" · "}
            {current.mimeType ?? t("details.unknownType")}
          </span>
        </div>
        <button type="button" className="details__close" onClick={onClose} aria-label={t("details.close")}>
          <X size={17} />
        </button>
      </header>

      <div className="details__body">
        <section className="details__hero">
          <svg width="76" height="76" viewBox="0 0 76 76" className={`details__ring details__ring--${status}`} aria-hidden="true">
            <circle cx="38" cy="38" r="30" className="details__ring-track" />
            <circle
              cx="38"
              cy="38"
              r="30"
              className="details__ring-fill"
              strokeDasharray={`${(RING * percent) / 100} ${RING}`}
              transform="rotate(-90 38 38)"
            />
          </svg>
          <span className="details__ring-label num">{fmt.percent(percent)}</span>
          <div className="details__hero-copy">
            <span className={`status-pill status-pill--${status}`}>
              <span className="status-pill__dot" />
              {t(`status.${status}` as MessageKey)}
            </span>
            {rate ? <strong className="num">{rate}</strong> : null}
            <span className="num">
              {current.totalBytes
                ? t("table.of", { done: fmt.bytes(current.downloadedBytes), total: fmt.bytes(current.totalBytes) })
                : fmt.bytes(current.downloadedBytes)}
            </span>
            {remaining ? <span className="num">{t("details.left", { time: remaining })}</span> : null}
          </div>
        </section>

        {transferring && connections !== null ? (
          <section className="details__section">
            <div className="details__section-head">
              <h3>{t("details.connections")}</h3>
              <span className="num">
                {t("details.connectionsValue", {
                  active: fmt.number(connections),
                  max: fmt.number(maxConnections ?? connections),
                })}
              </span>
            </div>
            <div className="details__lanes" aria-hidden="true">
              {Array.from({ length: Math.max(1, maxConnections ?? connections) }, (_, lane) => (
                <span key={lane} className={lane < connections ? "details__lane details__lane--on" : "details__lane"} />
              ))}
            </div>
            {metrics?.adaptiveReason ? (
              <p className="details__engine">{engineReasonText(metrics.adaptiveReason, t)}</p>
            ) : null}
          </section>
        ) : null}

        {transferring && history && history.length > 1 ? (
          <section className="details__section">
            <h3>{t("details.speedHistory")}</h3>
            <Sparkline className="details__spark" values={history} width={320} height={56} area floor={64 * 1024} />
          </section>
        ) : null}

        <div className="details__actions">
          {availableActions(current).map((action) => {
            const Icon = ACTION_ICONS[action];
            return (
              <button
                key={action}
                type="button"
                className={action === "pause" || action === "resume" || action === "start" || action === "retry" ? "details__primary" : undefined}
                onClick={() => onAction(current, action)}
              >
                <Icon size={14} />
                {t(`action.${action}` as MessageKey)}
              </button>
            );
          })}
          <button type="button" onClick={() => void run("open_download_file")} disabled={!hasFile}>
            <ExternalLink size={14} />
            {t("action.open")}
          </button>
          <button type="button" onClick={() => void run("reveal_download_file")} disabled={!hasFile}>
            <FolderOpen size={14} />
            {t("action.reveal")}
          </button>
          <button type="button" onClick={() => onRefreshSource(current)} disabled={!canRefresh}>
            <RefreshCw size={14} />
            {t("action.refreshSource")}
          </button>
        </div>

        {current.errorMessage ? (
          <div className={`details__failure details__failure--${status === "failed" ? "error" : "notice"}`}>
            <strong>{status === "failed" ? current.errorCode ?? t("details.failed") : t("details.attention")}</strong>
            <span>{noticeText(current.errorCode, current.errorMessage, t, language)}</span>
          </div>
        ) : null}

        {ruleExplanation ? (
          <section className="details__section">
            <h3>{t("details.intake")}</h3>
            <p className="details__note">{ruleExplanation}</p>
          </section>
        ) : null}

        <dl className="details__properties">
          <Property label={t("details.source")}>
            <span className="ltr details__value-clip" title={current.sourceUrl}>
              {current.sourceUrl}
            </span>
            <CopyButton done={copied === "url"} label={t("details.copyUrl")} onClick={() => void copy(current.sourceUrl, "url")} />
          </Property>
          <Property label={t("details.destination")}>
            <span className="ltr details__value-clip" title={current.destinationPath ?? ""}>
              {current.destinationPath ?? "—"}
            </span>
            {current.destinationPath ? (
              <CopyButton done={copied === "path"} label={t("details.copyPath")} onClick={() => void copy(current.destinationPath!, "path")} />
            ) : null}
          </Property>
          <Property label={t("details.size")}>
            <span className="num">{current.totalBytes ? fmt.bytes(current.totalBytes) : t("details.unknown")}</span>
          </Property>
          <Property label={t("details.range")}>
            {current.rangeSupported === null
              ? t("details.rangeUnknown")
              : current.rangeSupported
                ? t("details.rangeYes")
                : t("details.rangeNo")}
          </Property>
          {current.queueId ? (
            <Property label={t("details.queue")}>
              {queueName ?? current.queueId} · {t(`priority.${current.priority}` as MessageKey)}
            </Property>
          ) : null}
          <Property label={t("details.added")}>
            <span className="num">{fmt.date(current.createdAt)}</span>
          </Property>
          {current.completedAt ? (
            <Property label={t("details.completedAt")}>
              <span className="num">{fmt.date(current.completedAt)}</span>
            </Property>
          ) : null}
          {current.attempts > 0 ? (
            <Property label={t("details.attempts")}>
              <span className="num">{fmt.number(current.attempts)}</span>
            </Property>
          ) : null}
        </dl>

        {status !== "completed" ? <MirrorsEditor downloadId={current.id} onError={onError} /> : null}

        <ChecksSection downloadId={current.id} completed={status === "completed"} onError={onError} />

        <section className="details__section">
          <h3>{t("details.activity")}</h3>
          {activity.length ? (
            <ol className="details__timeline">
              {[...activity].reverse().map((entry, index) => (
                <li key={`${entry.at}-${index}`} className={`details__event details__event--${entry.status ?? entry.kind}`}>
                  <time className="num">{fmt.clock(entry.at)}</time>
                  <span>
                    {entry.kind === "status"
                      ? t(`status.${entry.status}` as MessageKey)
                      : entry.text}
                  </span>
                </li>
              ))}
            </ol>
          ) : (
            <p className="details__note">{t("details.activityEmpty")}</p>
          )}
        </section>
      </div>
    </aside>
  );
}

function Property({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="details__property">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function CopyButton({ done, label, onClick }: { done: boolean; label: string; onClick: () => void }) {
  return (
    <button type="button" className="details__copy" onClick={onClick} aria-label={label} title={label}>
      {done ? <Check size={13} /> : <Copy size={13} />}
    </button>
  );
}
