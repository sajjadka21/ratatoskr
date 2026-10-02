import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../i18n/I18n";

import "./PartInfo.css";

export type DownloadPart = {
  index: number;
  start: number;
  total: number;
  downloaded: number;
  status: "pending" | "downloading" | "completed" | string;
};

/** How much of each part is done, as 0–1. */
export function partFraction(part: DownloadPart): number {
  if (part.total <= 0) return 0;
  return Math.min(1, Math.max(0, part.downloaded / part.total));
}

/** The width of each part in the strip: its share of the whole file, never zero. */
export function partShares(parts: DownloadPart[]): number[] {
  const sum = parts.reduce((all, part) => all + Math.max(part.total, 0), 0);
  return parts.map((part) => (sum > 0 ? Math.max(part.total, 0) / sum : 1 / Math.max(parts.length, 1)));
}

/**
 * The "Part info" view: one block per connection's part of the file, filled
 * as it arrives, plus a table. The numbers come from the backend, which owns
 * the segments; this only draws them.
 */
export function PartInfo({ id, live }: { id: string; live: boolean }) {
  const { t, fmt } = useI18n();
  const [open, setOpen] = useState(false);
  const [parts, setParts] = useState<DownloadPart[]>([]);

  useEffect(() => {
    let cancelled = false;
    const load = () =>
      invoke<DownloadPart[]>("get_download_parts", { downloadId: id })
        .then((rows) => {
          if (!cancelled) setParts(rows);
        })
        .catch(() => {});
    void load();
    if (!live) return () => { cancelled = true; };
    const timer = window.setInterval(load, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [id, live]);

  if (parts.length === 0) return null;
  const shares = partShares(parts);

  return (
    <section className="parts">
      <div className="parts__strip" role="img" aria-label={t("mini.partInfo")}>
        {parts.map((part, position) => (
          <span
            key={part.index}
            className={`parts__block parts__block--${part.status}`}
            style={{ flexGrow: shares[position] }}
          >
            <i style={{ inlineSize: `${partFraction(part) * 100}%` }} />
          </span>
        ))}
      </div>
      <button type="button" className="parts__toggle" onClick={() => setOpen((value) => !value)} aria-expanded={open}>
        {open ? <ChevronUp size={15} /> : <ChevronDown size={15} />}
        {t("mini.partInfo")} · {parts.length}
      </button>
      {open ? (
        <div className="parts__scroll">
        <table className="parts__table">
          <thead>
            <tr>
              <th>#</th>
              <th>{t("mini.partStatus")}</th>
              <th>{t("mini.partDownloaded")}</th>
              <th>{t("mini.partTotal")}</th>
            </tr>
          </thead>
          <tbody>
            {parts.map((part) => (
              <tr key={part.index}>
                <td>{part.index + 1}</td>
                <td>{t(`mini.part.${part.status === "completed" ? "completed" : part.status === "downloading" ? "downloading" : "pending"}` as "mini.part.pending")}</td>
                <td>{fmt.bytes(part.downloaded)}</td>
                <td>{fmt.bytes(part.total)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        </div>
      ) : null}
    </section>
  );
}
