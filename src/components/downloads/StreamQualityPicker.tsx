import { Film } from "lucide-react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { useI18n } from "../../i18n/I18n";
import type { StreamVariant } from "../../types/download";
import { isStreamLink, variantLabel } from "../../utils/streams";

/**
 * Shown under the link when it is an HLS playlist: lists the qualities it
 * offers and swaps the link for the chosen one. Qualities whose sound is a
 * separate track are shown but cannot be picked.
 */
export function StreamQualityPicker({
  url,
  onChoose,
}: {
  url: string;
  onChoose: (variantUrl: string) => void;
}) {
  const { t, fmt } = useI18n();
  const [variants, setVariants] = useState<StreamVariant[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const stream = isStreamLink(url);

  useEffect(() => {
    setVariants(null);
    setFailed(null);
    if (!stream) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      invoke<StreamVariant[]>("list_stream_variants", { url: url.trim() })
        .then((found) => {
          if (!cancelled) setVariants(found);
        })
        .catch((reason: unknown) => {
          if (!cancelled) setFailed(String(reason));
        });
    }, 350);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [stream, url]);

  if (!stream) return null;

  return (
    <div className="add-download-modal__stream">
      <Film size={14} aria-hidden="true" />
      {failed ? (
        <span className="add-download-modal__stream-note">{failed}</span>
      ) : !variants ? (
        <span className="add-download-modal__stream-note">{t("stream.checking")}</span>
      ) : variants.length <= 1 ? (
        <span className="add-download-modal__stream-note">{t("stream.single")}</span>
      ) : (
        <label>
          <span>{t("stream.quality")}</span>
          <select
            value=""
            onChange={(event) => {
              if (event.target.value) onChoose(event.target.value);
            }}
          >
            <option value="">{t("stream.best")}</option>
            {variants.map((variant) => (
              <option key={variant.uri} value={variant.uri} disabled={variant.needsMuxing}>
                {variantLabel(variant, fmt)}
                {variant.needsMuxing ? ` — ${t("stream.needsMuxing")}` : ""}
              </option>
            ))}
          </select>
        </label>
      )}
    </div>
  );
}
