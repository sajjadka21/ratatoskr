import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";

const PRESETS = [256 * 1024, 1024 * 1024, 5 * 1024 * 1024];

/**
 * A speed limit for this download alone. The global limit in Settings still
 * applies on top; a running download follows a change straight away.
 */
export function SpeedLimitSection({
  downloadId,
  onError,
}: {
  downloadId: string;
  onError: (message: string) => void;
}) {
  const { t, fmt } = useI18n();
  const [limit, setLimit] = useState<number | null>(null);
  const [custom, setCustom] = useState("");

  useEffect(() => {
    let cancelled = false;
    setCustom("");
    invoke<number | null>("get_download_speed_limit", { id: downloadId })
      .then((value) => {
        if (!cancelled) setLimit(value);
      })
      .catch(() => {
        if (!cancelled) setLimit(null);
      });
    return () => {
      cancelled = true;
    };
  }, [downloadId]);

  async function apply(bytesPerSecond: number | null) {
    try {
      setLimit(await invoke<number | null>("set_download_speed_limit", { id: downloadId, bytesPerSecond }));
      setCustom("");
    } catch (reason) {
      onError(String(reason));
    }
  }

  const customValue = Number(custom.replace(/[^\d]/g, ""));
  const isPreset = limit === null || PRESETS.includes(limit);

  return (
    <section className="details__section">
      <div className="details__section-head">
        <h3>{t("taskLimit.title")}</h3>
        <span className="num">{limit ? fmt.rate(limit) : t("band.unlimited")}</span>
      </div>
      <div className="details__chips" role="group" aria-label={t("taskLimit.title")}>
        <button type="button" aria-pressed={limit === null} onClick={() => void apply(null)}>
          {t("band.unlimited")}
        </button>
        {PRESETS.map((preset) => (
          <button key={preset} type="button" aria-pressed={limit === preset} onClick={() => void apply(preset)}>
            {fmt.rate(preset)}
          </button>
        ))}
        {!isPreset && limit ? (
          <button type="button" aria-pressed>
            {fmt.rate(limit)}
          </button>
        ) : null}
      </div>
      <div className="details__mirror-add">
        <input
          inputMode="numeric"
          value={custom}
          placeholder={t("taskLimit.custom")}
          aria-label={t("taskLimit.custom")}
          onChange={(event) => setCustom(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && customValue > 0) void apply(customValue * 1024);
          }}
        />
        <button
          type="button"
          className="details__text-button"
          disabled={!(customValue > 0)}
          onClick={() => void apply(customValue * 1024)}
        >
          {t("taskLimit.apply")}
        </button>
      </div>
      <p className="details__note">{t("taskLimit.hint")}</p>
    </section>
  );
}
