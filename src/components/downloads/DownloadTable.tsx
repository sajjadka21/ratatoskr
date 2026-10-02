import {
  ArrowDownNarrowWide,
  ArrowUpNarrowWide,
  MoreHorizontal,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import {
  memo,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type SyntheticEvent,
} from "react";

import { useI18n } from "../../i18n/I18n";
import { isAlarm, noticeText } from "../../utils/notices";
import type { MessageKey } from "../../i18n/messages";
import type { DownloadListItem, TaskAction, TransferMetrics } from "../../types/download";
import { displayName } from "../../utils/fileKind";
import { formatHost } from "../../utils/format";
import { primaryAction } from "../../utils/taskActions";
import { FileBadge } from "../common/FileBadge";
import { Sparkline } from "../common/Sparkline";

import "./DownloadTable.css";

export type SortKey = "added" | "name" | "progress" | "speed" | "size";
export type SortState = { key: SortKey; descending: boolean };

const ROW_HEIGHT = 56;
const OVERSCAN = 8;

const ACTION_ICONS = {
  start: Play,
  resume: Play,
  pause: Pause,
  retry: RotateCcw,
  cancel: RotateCcw,
  restart: RefreshCw,
} as const;

type DownloadTableProps = {
  items: DownloadListItem[];
  metrics: Record<string, TransferMetrics | undefined>;
  history: Record<string, number[]>;
  selectedIds: Set<string>;
  focusedId: string | null;
  queueNames: Map<string, string>;
  nowSeconds: number;
  sort: SortState;
  empty: ReactNode;
  onSort: (key: SortKey) => void;
  onSelect: (item: DownloadListItem, index: number, event: SyntheticEvent) => void;
  onAction: (item: DownloadListItem, action: TaskAction) => void;
  onContextMenu: (item: DownloadListItem, x: number, y: number) => void;
};

/**
 * The download list. Rows have a fixed height and only the visible ones are
 * rendered, so thousands of history entries scroll as smoothly as ten.
 */
export function DownloadTable({
  items,
  metrics,
  history,
  selectedIds,
  focusedId,
  queueNames,
  nowSeconds,
  sort,
  empty,
  onSort,
  onSelect,
  onAction,
  onContextMenu,
}: DownloadTableProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, height: 600 });

  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const update = () => setViewport({ top: element.scrollTop, height: element.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    element.addEventListener("scroll", update, { passive: true });
    return () => {
      observer.disconnect();
      element.removeEventListener("scroll", update);
    };
  }, []);

  // Keep the row the user moved to with the keyboard in view.
  useEffect(() => {
    if (!focusedId) return;
    const index = items.findIndex((item) => item.id === focusedId);
    const element = scrollRef.current;
    if (index < 0 || !element) return;
    const top = index * ROW_HEIGHT;
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + ROW_HEIGHT > element.scrollTop + element.clientHeight) {
      element.scrollTop = top + ROW_HEIGHT - element.clientHeight;
    }
  }, [focusedId, items]);

  const first = Math.max(0, Math.floor(viewport.top / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(items.length, Math.ceil((viewport.top + viewport.height) / ROW_HEIGHT) + OVERSCAN);

  const columns: Array<{ key: SortKey | null; label: MessageKey; className: string }> = [
    { key: "name", label: "table.name", className: "download-table__cell--name" },
    { key: "progress", label: "table.progress", className: "" },
    { key: "speed", label: "table.speed", className: "" },
    { key: null, label: "table.remaining", className: "" },
    { key: null, label: "table.status", className: "" },
  ];

  return (
    <div className="download-table" role="grid" aria-rowcount={items.length} aria-multiselectable="true">
      <div className="download-table__head" role="row">
        <span role="columnheader" aria-hidden="true" />
        {columns.map((column) => {
          const active = column.key !== null && sort.key === column.key;
          const SortIcon = sort.descending ? ArrowDownNarrowWide : ArrowUpNarrowWide;
          return (
            <span
              key={column.label}
              role="columnheader"
              className={column.className}
              aria-sort={active ? (sort.descending ? "descending" : "ascending") : undefined}
            >
              {column.key ? (
                <button
                  type="button"
                  onClick={() => onSort(column.key!)}
                  aria-label={t("table.sortBy", { column: t(column.label) })}
                >
                  {t(column.label)}
                  {active ? <SortIcon size={13} aria-hidden="true" /> : null}
                </button>
              ) : (
                t(column.label)
              )}
            </span>
          );
        })}
        <span role="columnheader" aria-hidden="true" />
      </div>

      <div className="download-table__scroll" ref={scrollRef}>
        {items.length === 0 ? (
          empty
        ) : (
          <div className="download-table__body" style={{ height: items.length * ROW_HEIGHT }}>
            {items.slice(first, last).map((item, offset) => {
              const index = first + offset;
              return (
                <DownloadTableRow
                  key={item.id}
                  item={item}
                  index={index}
                  top={index * ROW_HEIGHT}
                  metrics={metrics[item.id]}
                  history={history[item.id]}
                  selected={selectedIds.has(item.id)}
                  focused={focusedId === item.id}
                  queueName={item.queueId ? queueNames.get(item.queueId) : undefined}
                  nowSeconds={item.status.toLowerCase() === "retrying" ? nowSeconds : 0}
                  onSelect={onSelect}
                  onAction={onAction}
                  onContextMenu={onContextMenu}
                />
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

type RowProps = {
  item: DownloadListItem;
  index: number;
  top: number;
  metrics?: TransferMetrics;
  history?: number[];
  selected: boolean;
  focused: boolean;
  queueName?: string;
  nowSeconds: number;
  onSelect: DownloadTableProps["onSelect"];
  onAction: DownloadTableProps["onAction"];
  onContextMenu: DownloadTableProps["onContextMenu"];
};

const DownloadTableRow = memo(function DownloadTableRow({
  item,
  index,
  top,
  metrics,
  history,
  selected,
  focused,
  queueName,
  nowSeconds,
  onSelect,
  onAction,
  onContextMenu,
}: RowProps) {
  const { t, fmt, language } = useI18n();
  const notice = noticeText(item.errorCode, item.errorMessage, t, language);
  const status = item.status.toLowerCase();
  const transferring = status === "downloading";
  const percent =
    item.totalBytes && item.totalBytes > 0
      ? Math.min(100, (item.downloadedBytes / item.totalBytes) * 100)
      : status === "completed"
        ? 100
        : null;

  const size = item.totalBytes
    ? transferring || status === "paused" || status === "retrying"
      ? t("table.of", { done: fmt.bytes(item.downloadedBytes), total: fmt.bytes(item.totalBytes) })
      : fmt.bytes(item.totalBytes)
    : item.downloadedBytes > 0
      ? fmt.bytes(item.downloadedBytes)
      : null;

  const rate = transferring ? fmt.rate(metrics?.bytesPerSecond ?? null) : null;
  const remaining =
    status === "retrying" && item.retryAt !== null && nowSeconds > 0
      ? item.retryAt - nowSeconds > 0
        ? t("table.retryIn", {
            attempt: fmt.number(item.attempts + 1),
            time: fmt.duration(item.retryAt - nowSeconds) ?? "",
          })
        : t("table.retryingNow")
      : transferring
        ? fmt.duration(metrics?.etaSeconds ?? null)
        : null;

  const action = primaryAction(item);
  const ActionIcon = action ? ACTION_ICONS[action] : null;
  const connections =
    transferring && metrics?.activeConnections
      ? t("table.connections", {
          active: fmt.number(metrics.activeConnections),
          max: fmt.number(metrics.maxConnections ?? metrics.activeConnections),
        })
      : null;

  function openMenu(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    onContextMenu(item, event.clientX, event.clientY);
  }

  return (
    <div
      className={[
        "download-table__row",
        `download-table__row--${status}`,
        selected ? "download-table__row--selected" : "",
        focused ? "download-table__row--focused" : "",
        index % 2 === 1 ? "download-table__row--odd" : "",
      ].join(" ")}
      style={{ transform: `translateY(${top}px)` }}
      role="row"
      aria-rowindex={index + 1}
      aria-selected={selected}
      tabIndex={0}
      onClick={(event) => onSelect(item, index, event)}
      onContextMenu={openMenu}
      onKeyDown={(event: KeyboardEvent) => {
        if (event.key === "Enter") {
          event.preventDefault();
          onSelect(item, index, event);
        }
      }}
    >
      <span role="gridcell" className="download-table__cell">
        <FileBadge item={item} />
      </span>

      <span role="gridcell" className="download-table__cell download-table__cell--name">
        <span className="download-table__name ltr" title={displayName(item)}>
          {displayName(item)}
        </span>
        <span className="download-table__meta">
          <span className="ltr">{formatHost(item.resolvedUrl ?? item.sourceUrl)}</span>
          {size ? <span className="num">{size}</span> : null}
          {queueName ? (
            <span className="download-table__chip">
              {queueName} · {t(`priority.${item.priority}` as MessageKey)}
            </span>
          ) : null}
          {connections ? <span className="download-table__chip download-table__chip--accent num">{connections}</span> : null}
          {notice && (status !== "completed" || isAlarm(item.errorCode)) ? (
            <span
              className={`download-table__message download-table__message--${status === "failed" || isAlarm(item.errorCode) ? "error" : "notice"}`}
              title={notice}
            >
              {notice}
            </span>
          ) : null}
        </span>
      </span>

      <span role="gridcell" className="download-table__cell download-table__cell--progress">
        <span className="download-table__bar" aria-hidden="true">
          <span
            className={percent === null && transferring ? "download-table__fill download-table__fill--unknown" : "download-table__fill"}
            style={{ inlineSize: `${percent ?? (transferring ? 100 : 0)}%` }}
          />
        </span>
        <span className="download-table__percent num">{percent !== null ? fmt.percent(percent) : ""}</span>
      </span>

      <span role="gridcell" className="download-table__cell download-table__cell--speed">
        {rate ? (
          <>
            <span className="num">{rate}</span>
            {history && history.length > 1 ? (
              <Sparkline className="download-table__spark" values={history} width={72} height={16} floor={32 * 1024} />
            ) : null}
          </>
        ) : null}
      </span>

      <span role="gridcell" className="download-table__cell download-table__cell--muted num">
        {remaining ?? ""}
      </span>

      <span role="gridcell" className="download-table__cell">
        <span className={`status-pill status-pill--${status}`}>
          <span className="status-pill__dot" />
          {t(`status.${status}` as MessageKey)}
        </span>
      </span>

      <span role="gridcell" className="download-table__cell download-table__cell--actions">
        {action && ActionIcon ? (
          <button
            type="button"
            className="download-table__action"
            aria-label={t(`action.${action}` as MessageKey)}
            title={t(`action.${action}` as MessageKey)}
            onClick={(event) => {
              event.stopPropagation();
              onAction(item, action);
            }}
          >
            <ActionIcon size={14} strokeWidth={2.2} />
          </button>
        ) : null}
        <button
          type="button"
          className="download-table__more"
          aria-label={t("table.actions")}
          onClick={(event) => {
            event.stopPropagation();
            const rect = event.currentTarget.getBoundingClientRect();
            onContextMenu(item, rect.left, rect.bottom + 6);
          }}
        >
          <MoreHorizontal size={16} />
        </button>
      </span>
    </div>
  );
});
