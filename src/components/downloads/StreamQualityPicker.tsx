import { Film } from "lucide-react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { useI18n } from "../../i18n/I18n";
import type { FfmpegStatus, StreamVariant } from "../../types/download";
import { isStreamLink, variantLabel, withQuality } from "../../utils/streams";

/**
 * Shown under the link when it is an HLS or DASH stream: lists the
 * qualities it offers and names the chosen one in the link. Qualities
 * whose sound is a separate track need FFmpeg; without it they are shown
 * but cannot be picked.
 */
export function StreamQualityPicker({
  url,
  onChoose,
}: {
  url: string;
  onChoose: (nextUrl: string) => void;
}) {
  const { t, fmt } = useI18n();
  const [variants, setVariants] = useState<StreamVariant[] | null>(null);
  const [canJoin, setCanJoin] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const stream = isStreamLink(url);
  const address = url.trim().split("#")[0];

  useEffect(() => {
    setVariants(null);
    setFailed(null);
    if (!stream) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      Promise.all([
        invoke<StreamVariant[]>("list_stream_variants", { url: address }),
        invoke<FfmpegStatus>("get_ffmpeg_status").catch(() => null),
      ])
        .then(([found, ffmpeg]) => {
          if (cancelled) return;
          setVariants(found);
          setCanJoin(Boolean(ffmpeg?.foundPath));
        })
        .catch((reason: unknown) => {
          if (!cancelled) setFailed(String(reason));
        });
    }, 350);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [stream, address]);

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
              const chosen = variants[Number(event.target.value)];
              if (!chosen) return;
              onChoose(chosen.height ? withQuality(address, chosen.height) : chosen.uri);
            }}
          >
            <option value="">{t("stream.best")}</option>
            {variants.map((variant, index) => {
              const blocked = variant.needsMuxing && !canJoin;
              return (
                <option key={`${variant.uri}-${index}`} value={index} disabled={blocked}>
                  {variantLabel(variant, fmt)}
                  {blocked ? ` — ${t("stream.needsMuxing")}` : ""}
                </option>
              );
            })}
          </select>
        </label>
      )}
    </div>
  );
}
