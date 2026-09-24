import { useEffect, useRef, useState } from "react";

import type { DownloadListItem, TransferMetricsMap } from "../types/download";

const SAMPLES = 60;

export type ThroughputHistory = {
  /** Total bytes per second, one sample per second, oldest first. */
  total: number[];
  /** Per download, only for downloads that are moving or recently moved. */
  byTask: Record<string, number[]>;
};

/**
 * Samples the engine's live rates once a second so the band and rows can
 * draw real history. Stops sampling (and costs nothing) while nothing moves.
 */
export function useThroughputHistory(
  downloads: DownloadListItem[],
  metrics: TransferMetricsMap,
): ThroughputHistory {
  const latest = useRef({ downloads, metrics });
  latest.current = { downloads, metrics };
  const [history, setHistory] = useState<ThroughputHistory>({ total: [], byTask: {} });

  const active = downloads.some((item) => item.status.toLowerCase() === "downloading");

  useEffect(() => {
    if (!active) {
      // One more zero sample makes the chart settle on idle instead of freezing mid-air.
      setHistory((current) =>
        current.total.length && current.total[current.total.length - 1] !== 0
          ? { ...current, total: [...current.total, 0].slice(-SAMPLES) }
          : current,
      );
      return;
    }

    const timer = window.setInterval(() => {
      const { downloads: items, metrics: live } = latest.current;
      let total = 0;
      const moving = new Set<string>();
      for (const item of items) {
        if (item.status.toLowerCase() !== "downloading") continue;
        const rate = live[item.id]?.bytesPerSecond ?? 0;
        total += rate;
        moving.add(item.id);
      }

      setHistory((current) => {
        const byTask: Record<string, number[]> = {};
        for (const id of moving) {
          const rate = live[id]?.bytesPerSecond ?? 0;
          byTask[id] = [...(current.byTask[id] ?? []), rate].slice(-SAMPLES);
        }
        return { total: [...current.total, total].slice(-SAMPLES), byTask };
      });
    }, 1000);

    return () => window.clearInterval(timer);
  }, [active]);

  return history;
}
