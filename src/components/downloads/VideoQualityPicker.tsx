import { invoke } from "@tauri-apps/api/core";
import { Clapperboard, ListVideo, Music } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type { EngineSettings, VideoProbe } from "../../types/download";
import { heightLabel } from "../../utils/streams";
import { isVideoPage, videoQualityOf, type VideoQuality } from "../../utils/videoPages";

/** Qualities offered before (or without) looking the video up. */
const COMMON_HEIGHTS = [1080, 720, 480, 360];

/** Lookups already made in this session, by link. */
const probes = new Map<string, VideoProbe>();

type Lookup =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "done"; probe: VideoProbe }
  | { state: "failed"; reason: string };

/**
 * Shown in the Add dialog when the links include video pages: choose the
 * quality (or the sound alone) for these videos now, whatever the default in
 * Settings. For a single video it looks the video up and shows what each
 * quality weighs; for a playlist it offers to add every video in it.
 */
export function VideoQualityPicker({
  text,
  quality,
  onQualityChange,
  onReplaceLinks,
  onPlaylist,
  onBusy,
}: {
  text: string;
  /** Chosen here; null until the user picks one. */
  quality: VideoQuality | null;
  onQualityChange: (quality: VideoQuality) => void;
  /** Replaces the links, for adding every video of a playlist. */
  onReplaceLinks: (text: string) => void;
  /** The videos of a playlist link, once known; null otherwise. */
  onPlaylist?: (links: string[] | null) => void;
  /** Whether the link is still being looked up. */
  onBusy?: (busy: boolean) => void;
}) {
  const { t, fmt } = useI18n();
  const [fallback, setFallback] = useState<VideoQuality>("best");
  const [lookup, setLookup] = useState<Lookup>({ state: "idle" });

  const lines = useMemo(
    () => text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean),
    [text],
  );
  const videos = lines.filter((line) => isVideoPage(line.split("#")[0]));
  const single = lines.length === 1 && videos.length === 1 ? videos[0].split("#")[0] : null;

  useEffect(() => {
    invoke<EngineSettings>("get_engine_settings")
      .then((settings) => setFallback(settings.streamMaxHeight ?? "best"))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!single) {
      setLookup({ state: "idle" });
      return;
    }
    const known = probes.get(single);
    if (known) {
      setLookup({ state: "done", probe: known });
      return;
    }
    setLookup({ state: "loading" });
    let cancelled = false;
    const timer = window.setTimeout(() => {
      invoke<VideoProbe>("probe_video", { url: single })
        .then((probe) => {
          probes.set(single, probe);
          if (!cancelled) setLookup({ state: "done", probe });
        })
        .catch((reason: unknown) => {
          if (!cancelled) setLookup({ state: "failed", reason: String(reason) });
        });
    }, 450);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [single]);

  const probe = lookup.state === "done" ? lookup.probe : null;
  const playlistLinks = probe?.isPlaylist ? probe.entries.map((entry) => entry.url) : null;
  const playlistKey = playlistLinks?.join("\n") ?? null;
  useEffect(() => {
    onPlaylist?.(playlistKey ? playlistKey.split("\n") : null);
  }, [playlistKey, onPlaylist]);
  const busy = lookup.state === "loading";
  useEffect(() => {
    onBusy?.(busy);
  }, [busy, onBusy]);
  // Nothing lingers in the dialog once the picker is gone.
  useEffect(
    () => () => {
      onPlaylist?.(null);
      onBusy?.(false);
    },
    [onPlaylist, onBusy],
  );

  if (videos.length === 0) return null;

  const isList = Boolean(probe?.isPlaylist);
  if (isList && probe && probe.entries.length === 0) {
    return (
      <div className="video-picker">
        <div className="video-picker__heading">
          <ListVideo size={15} aria-hidden="true" />
          <strong>{probe.title ?? t("video.playlist")}</strong>
        </div>
        <p className="video-picker__note">{t("video.playlistEmpty")}</p>
      </div>
    );
  }

  const inLinks = videos.map((line) => videoQualityOf(line));
  const sameInLinks = inLinks.every((chosen) => chosen === inLinks[0]);
  const current: VideoQuality | null =
    quality ?? (sameInLinks ? (inLinks[0] ?? fallback) : null);

  type Choice = { quality: VideoQuality; label: string; bytes?: number | null };
  const heights = probe && !isList
    ? probe.qualities
    : COMMON_HEIGHTS.map((height) => ({ height, bytes: null as number | null }));
  const choices: Choice[] = [
    {
      quality: "best",
      label: t("video.best"),
      bytes: probe?.qualities[0]?.bytes ?? null,
    },
    ...heights.map((option) => ({
      quality: option.height,
      label: heightLabel(option.height, fmt.language),
      bytes: option.bytes,
    })),
    { quality: "audio", label: t("video.audio"), bytes: probe?.audioBytes ?? null },
  ];
  // A default from Settings that this video does not have still shows up,
  // so the current choice is never invisible.
  if (current !== null && !choices.some((choice) => choice.quality === current)) {
    choices.splice(1, 0, {
      quality: current,
      label: typeof current === "number" ? heightLabel(current, fmt.language) : String(current),
    });
  }

  const failure =
    lookup.state === "failed"
      ? lookup.reason.includes("needs_ytdlp")
        ? t("video.needsYtdlp")
        : t("video.lookupFailed")
      : null;

  return (
    <div className="video-picker">
      <div className="video-picker__heading">
        {isList ? (
          <ListVideo size={15} aria-hidden="true" />
        ) : (
          <Clapperboard size={15} aria-hidden="true" />
        )}
        {probe?.title ? (
          <strong className="video-picker__title" title={probe.title} dir="auto">
            {probe.title}
          </strong>
        ) : (
          <strong>
            {videos.length > 1
              ? t("video.qualityForAll", { count: fmt.number(videos.length) })
              : t("video.quality")}
          </strong>
        )}
        {probe?.durationSeconds ? (
          <span className="video-picker__meta">{fmt.duration(probe.durationSeconds)}</span>
        ) : null}
      </div>

      {isList && probe ? (
        <div className="video-picker__list">
          <p className="video-picker__note">
            {t("video.playlistHint", { count: fmt.number(probe.entries.length) })}
          </p>
          <button
            type="button"
            className="video-picker__expand"
            onClick={() => onReplaceLinks(probe.entries.map((entry) => entry.url).join("\n"))}
          >
            {t("video.addAll", { count: fmt.number(probe.entries.length) })}
          </button>
        </div>
      ) : null}

      <div className="video-picker__choices" role="radiogroup" aria-label={t("video.quality")}>
        {choices.map((choice) => {
          const selected = current === choice.quality;
          return (
            <button
              key={String(choice.quality)}
              type="button"
              role="radio"
              aria-checked={selected}
              className={`video-picker__choice ${selected ? "video-picker__choice--selected" : ""}`}
              onClick={() => onQualityChange(choice.quality)}
            >
              {choice.quality === "audio" ? <Music size={12} aria-hidden="true" /> : null}
              <span>{choice.label}</span>
              {choice.bytes ? <small>≈ {fmt.bytes(choice.bytes)}</small> : null}
            </button>
          );
        })}
      </div>

      <p className="video-picker__note">
        {lookup.state === "loading"
          ? t("video.loading")
          : failure ?? (current === "audio" ? t("video.audioHint") : t("video.hint"))}
      </p>
    </div>
  );
}
