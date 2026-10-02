import { AlertTriangle, Check, Copy, FolderOpen, RotateCw, ShieldAlert, ShieldCheck, ShieldQuestion, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type { DownloadChecks } from "../../types/download";

const DOWNLOAD_CHECKS_EVENT = "download-checks-changed";

const ALGORITHM_LABEL = { md5: "MD5", sha1: "SHA-1", sha256: "SHA-256" } as const;

/**
 * Checksum, virus scan and unpacking for one download. Every result comes
 * from the engine; this panel only asks and shows.
 */
export function ChecksSection({
  downloadId,
  completed,
  onError,
}: {
  downloadId: string;
  completed: boolean;
  onError: (message: string) => void;
}) {
  const { t } = useI18n();
  const [checks, setChecks] = useState<DownloadChecks | null>(null);
  const [draft, setDraft] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setDraft("");
    setCopied(false);
    const load = () =>
      invoke<DownloadChecks>("get_download_checks", { downloadId })
        .then((found) => {
          if (!cancelled) setChecks(found);
        })
        .catch(() => {
          if (!cancelled) setChecks(null);
        });
    void load();
    const unlisten = listen<string>(DOWNLOAD_CHECKS_EVENT, (event) => {
      if (event.payload === downloadId) void load();
    }).catch(() => () => {});
    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
    };
  }, [downloadId, completed]);

  async function setExpected(checksum: string | null) {
    try {
      setChecks(await invoke<DownloadChecks>("set_expected_checksum", { downloadId, checksum }));
      setDraft("");
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function runAgain() {
    try {
      setChecks(await invoke<DownloadChecks>("run_post_process", { downloadId }));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function openExtracted() {
    try {
      await invoke("open_extracted_folder", { id: downloadId });
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function copyActual() {
    if (!checks?.actualChecksum) return;
    try {
      await navigator.clipboard.writeText(checks.actualChecksum);
      setCopied(true);
    } catch {
      /* clipboard unavailable */
    }
  }

  const running = checks?.state === "running";
  const integrity = checks?.integrity ?? null;
  const verdict = running
    ? { tone: "busy", icon: <RotateCw size={14} className="details__spin" />, text: t("checks.running") }
    : integrity === "verified"
      ? { tone: "good", icon: <ShieldCheck size={14} />, text: t("checks.verified") }
      : integrity === "mismatch"
        ? { tone: "bad", icon: <ShieldAlert size={14} />, text: t("checks.mismatch") }
        : integrity === "error"
          ? { tone: "warn", icon: <AlertTriangle size={14} />, text: t("checks.error") }
          : checks?.expectedChecksum && !completed
            ? { tone: "idle", icon: <ShieldQuestion size={14} />, text: t("checks.pending") }
            : null;

  const scan =
    checks?.scan === "clean"
      ? { tone: "good", icon: <ShieldCheck size={14} />, text: t("checks.scanClean") }
      : checks?.scan === "threat"
        ? { tone: "bad", icon: <ShieldAlert size={14} />, text: t("checks.scanThreat") }
        : checks?.scan === "unavailable"
          ? { tone: "warn", icon: <ShieldQuestion size={14} />, text: t("checks.scanUnavailable"), detail: checks.scanDetail }
          : null;

  return (
    <details key={downloadId} className="details__section details__disclosure" open={integrity === "mismatch" || scan?.tone === "bad" || undefined}>
      <summary>{t("checks.title")}</summary>
      {!checks?.expectedChecksum ? <p className="details__note">{t("checks.hint")}</p> : null}

      {checks?.expectedChecksum ? (
        <div className="details__checksum">
          <span className="details__checksum-label">
            {t("checks.expected")}
            {checks.algorithm ? ` · ${ALGORITHM_LABEL[checks.algorithm]}` : ""}
          </span>
          <code className="ltr details__value-clip" title={checks.expectedChecksum}>
            {checks.expectedChecksum}
          </code>
          <button
            type="button"
            className="details__copy"
            aria-label={t("checks.clear")}
            title={t("checks.clear")}
            onClick={() => void setExpected(null)}
          >
            <X size={13} />
          </button>
        </div>
      ) : (
        <div className="details__mirror-add">
          <input
            dir="ltr"
            value={draft}
            spellCheck={false}
            placeholder={t("checks.placeholder")}
            aria-label={t("checks.expected")}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && draft.trim()) void setExpected(draft);
            }}
          />
          <button
            type="button"
            onClick={() => void setExpected(draft)}
            disabled={!draft.trim()}
            aria-label={t("checks.verify")}
            title={t("checks.verify")}
          >
            <ShieldCheck size={14} />
          </button>
        </div>
      )}

      {verdict ? (
        <p className={`details__verdict details__verdict--${verdict.tone}`} role="status">
          {verdict.icon}
          <span>{verdict.text}</span>
        </p>
      ) : null}

      {checks?.actualChecksum && checks.algorithm ? (
        <div className="details__checksum">
          <span className="details__checksum-label">
            {t("checks.actual", { algorithm: ALGORITHM_LABEL[checks.algorithm] })}
          </span>
          <code className="ltr details__value-clip" title={checks.actualChecksum}>
            {checks.actualChecksum}
          </code>
          <button
            type="button"
            className="details__copy"
            aria-label={t("checks.copyActual")}
            title={t("checks.copyActual")}
            onClick={() => void copyActual()}
          >
            {copied ? <Check size={13} /> : <Copy size={13} />}
          </button>
        </div>
      ) : null}

      {scan ? (
        <p className={`details__verdict details__verdict--${scan.tone}`} title={scan.detail ?? undefined}>
          {scan.icon}
          <span>{scan.text}</span>
        </p>
      ) : null}

      {checks?.extractedTo ? (
        <div className="details__checksum">
          <span className="details__checksum-label">{t("checks.extracted")}</span>
          <bdi className="ltr details__value-clip" title={checks.extractedTo}>
            {checks.extractedTo}
          </bdi>
          <button
            type="button"
            className="details__copy"
            aria-label={t("checks.openExtracted")}
            title={t("checks.openExtracted")}
            onClick={() => void openExtracted()}
          >
            <FolderOpen size={13} />
          </button>
        </div>
      ) : null}
      {checks?.extractError ? (
        <p className="details__verdict details__verdict--warn">
          <AlertTriangle size={14} />
          <span>
            {t("checks.extractFailed")}: <bdi>{checks.extractError}</bdi>
          </span>
        </p>
      ) : null}
      {checks?.commandError ? (
        <p className="details__verdict details__verdict--warn">
          <AlertTriangle size={14} />
          <span>
            {t("checks.commandFailed")}: <bdi className="ltr">{checks.commandError}</bdi>
          </span>
        </p>
      ) : null}

      {completed && checks && checks.state !== "idle" && !running ? (
        <button type="button" className="details__link-button" onClick={() => void runAgain()}>
          <RotateCw size={13} /> {t("checks.runAgain")}
        </button>
      ) : null}
    </details>
  );
}
