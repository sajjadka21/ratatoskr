import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArchiveRestore,
  Copy,
  Database,
  FileDown,
  FileSpreadsheet,
  Link2,
  RotateCw,
  Save,
  ShieldCheck,
  Stethoscope,
  Wifi,
  WifiOff,
} from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import type { MessageKey } from "../../i18n/messages";
import type { BackupInfo, ConnectionCheck } from "../../types/download";

type SectionProps = {
  onError: (message: string) => void;
  onSaved: (message: string) => void;
};

const BACKUP_EXTENSION = "tosk";
/** Backups made before the rename still open. */
const OLD_BACKUP_EXTENSION = "rudbackup";

function today(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** Backing up, restoring and exporting the download list. */
export function BackupSection({ onError, onSaved }: SectionProps) {
  const { t, fmt } = useI18n();
  const [pending, setPending] = useState<BackupInfo | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<BackupInfo | null>("get_pending_restore")
      .then(setPending)
      .catch(() => {});
  }, []);

  async function backup() {
    try {
      const path = await saveDialog({
        title: t("backup.createButton"),
        defaultPath: `ratatosk-backup-${today()}.${BACKUP_EXTENSION}`,
        filters: [{ name: t("backup.fileKind"), extensions: [BACKUP_EXTENSION] }],
      });
      if (!path) return;
      setBusy(true);
      const info = await invoke<BackupInfo>("backup_database", { path });
      onSaved(t("backup.created", { downloads: fmt.number(info.downloads) }));
    } catch (reason) {
      onError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function chooseRestore() {
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        title: t("backup.restoreButton"),
        filters: [{ name: t("backup.fileKind"), extensions: [BACKUP_EXTENSION, OLD_BACKUP_EXTENSION, "db"] }],
      });
      if (typeof path !== "string") return;
      setPending(await invoke<BackupInfo>("stage_restore", { path }));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function cancelRestore() {
    try {
      await invoke("cancel_restore");
      setPending(null);
      onSaved(t("backup.cancelled"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function exportList(format: "csv" | "links") {
    try {
      const extension = format === "csv" ? "csv" : "txt";
      const path = await saveDialog({
        title: t(format === "csv" ? "export.csv" : "export.links"),
        defaultPath: `ratatosk-downloads-${today()}.${extension}`,
        filters: [{ name: t(format === "csv" ? "export.csv" : "export.links"), extensions: [extension] }],
      });
      if (!path) return;
      await invoke("export_downloads", { path, format, ids: null });
      onSaved(t("export.done"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("backup.title")}</h2>
        <p>{t("backup.hint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("backup.create")}</strong>
            <span>{t("backup.createHint")}</span>
          </div>
          <div className="settings-page__row-control">
            <button type="button" className="settings-page__secondary-button" disabled={busy} onClick={() => void backup()}>
              {busy ? <RotateCw size={14} className="settings-page__spin" /> : <Save size={14} />} {t("backup.createButton")}
            </button>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("backup.restore")}</strong>
            <span>{t("backup.restoreHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            {pending ? (
              <div className="maintenance__notice" role="status">
                <ArchiveRestore size={16} aria-hidden="true" />
                <span>
                  {t("backup.pending", {
                    downloads: fmt.number(pending.downloads),
                    queues: fmt.number(pending.queues),
                  })}
                </span>
              </div>
            ) : null}
            <span className="settings-page__button-row">
              {pending ? (
                <>
                  <button type="button" className="settings-page__primary-button" onClick={() => void invoke("restart_app")}>
                    <RotateCw size={14} /> {t("backup.restartNow")}
                  </button>
                  <button type="button" className="settings-page__secondary-button" onClick={() => void cancelRestore()}>
                    {t("backup.cancel")}
                  </button>
                </>
              ) : (
                <button type="button" className="settings-page__secondary-button" onClick={() => void chooseRestore()}>
                  <ArchiveRestore size={14} /> {t("backup.restoreButton")}
                </button>
              )}
            </span>
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("export.title")}</strong>
            <span>{t("export.hint")}</span>
          </div>
          <div className="settings-page__row-control">
            <span className="settings-page__button-row">
              <button type="button" className="settings-page__secondary-button" onClick={() => void exportList("csv")}>
                <FileSpreadsheet size={14} /> {t("export.csv")}
              </button>
              <button type="button" className="settings-page__secondary-button" onClick={() => void exportList("links")}>
                <Link2 size={14} /> {t("export.links")}
              </button>
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Database check, a connection test and the diagnostics report. */
export function DiagnosticsSection({ onError, onSaved }: SectionProps) {
  const { t, fmt } = useI18n();
  const [database, setDatabase] = useState<string | null>(null);
  const [checkingDatabase, setCheckingDatabase] = useState(false);
  const [url, setUrl] = useState("");
  const [testing, setTesting] = useState(false);
  const [connection, setConnection] = useState<ConnectionCheck | null>(null);
  const [report, setReport] = useState<string | null>(null);

  async function checkDatabase() {
    setCheckingDatabase(true);
    try {
      setDatabase(await invoke<string>("check_database"));
    } catch (reason) {
      onError(String(reason));
    } finally {
      setCheckingDatabase(false);
    }
  }

  async function testConnection() {
    if (!url.trim()) return;
    setTesting(true);
    setConnection(null);
    try {
      setConnection(await invoke<ConnectionCheck>("check_connection", { url: url.trim() }));
    } catch (reason) {
      onError(String(reason));
    } finally {
      setTesting(false);
    }
  }

  async function toggleReport() {
    if (report !== null) {
      setReport(null);
      return;
    }
    try {
      setReport(await invoke<string>("get_diagnostics_report"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function copyReport() {
    try {
      const text = report ?? (await invoke<string>("get_diagnostics_report"));
      await navigator.clipboard.writeText(text);
      onSaved(t("diag.copied"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function saveReport() {
    try {
      const path = await saveDialog({
        title: t("diag.report"),
        defaultPath: `ratatosk-diagnostics-${today()}.txt`,
        filters: [{ name: "Text", extensions: ["txt"] }],
      });
      if (!path) return;
      await invoke("save_diagnostics_report", { path });
      onSaved(t("diag.saved"));
    } catch (reason) {
      onError(String(reason));
    }
  }

  const elapsed = connection
    ? `${fmt.number(connection.elapsedMs / 1000, connection.elapsedMs < 10_000 ? 1 : 0)} ${fmt.language === "fa" ? "ثانیه" : "s"}`
    : "";

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("diag.title")}</h2>
        <p>{t("diag.hint")}</p>
      </div>

      <div className="settings-page__group settings-page__rows">
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("diag.database")}</strong>
            <span>{t("diag.databaseHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <button
              type="button"
              className="settings-page__secondary-button"
              disabled={checkingDatabase}
              onClick={() => void checkDatabase()}
            >
              {checkingDatabase ? <RotateCw size={14} className="settings-page__spin" /> : <Database size={14} />}{" "}
              {t("diag.databaseButton")}
            </button>
            {database !== null ? (
              <p className={`maintenance__verdict maintenance__verdict--${database === "ok" ? "good" : "warn"}`} role="status">
                {database === "ok" ? <ShieldCheck size={15} /> : <AlertTriangle size={15} />}
                <span>{database === "ok" ? t("diag.databaseOk") : t("diag.databaseBad", { detail: database })}</span>
              </p>
            ) : null}
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <label htmlFor="diag-url">
              <strong>{t("diag.connection")}</strong>
            </label>
            <span>{t("diag.connectionHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <span className="maintenance__inline">
              <input
                id="diag-url"
                className="settings-page__text-input"
                dir="ltr"
                spellCheck={false}
                placeholder="https://example.com/file.zip"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void testConnection();
                }}
              />
              <button
                type="button"
                className="settings-page__secondary-button"
                disabled={testing || !url.trim()}
                onClick={() => void testConnection()}
              >
                {testing ? <RotateCw size={14} className="settings-page__spin" /> : <Stethoscope size={14} />}{" "}
                {testing ? t("diag.testing") : t("diag.test")}
              </button>
            </span>
            {connection ? (
              <div
                className={`maintenance__result maintenance__result--${connection.reachable ? "good" : "bad"}`}
                role="status"
              >
                <p className="maintenance__result-title">
                  {connection.reachable ? <Wifi size={15} /> : <WifiOff size={15} />}
                  <strong>{connection.reachable ? t("diag.reachable", { time: elapsed }) : t("diag.unreachable")}</strong>
                  <span className="maintenance__chip">{t(`diag.route.${connection.route}` as MessageKey)}</span>
                </p>
                {connection.reachable ? (
                  <dl className="maintenance__facts">
                    <dt>{t("diag.size")}</dt>
                    <dd className="num">
                      {connection.totalBytes !== null ? fmt.bytes(connection.totalBytes) : t("diag.sizeUnknown")}
                    </dd>
                    <dt>{t("diag.resume")}</dt>
                    <dd>{connection.rangeSupported ? t("diag.resumeYes") : t("diag.resumeNo")}</dd>
                    {connection.finalHost ? (
                      <>
                        <dt>{t("diag.redirect")}</dt>
                        <dd>
                          <bdi className="ltr">{connection.finalHost}</bdi>
                        </dd>
                      </>
                    ) : null}
                  </dl>
                ) : (
                  <p className="maintenance__error">
                    <bdi>{connection.error}</bdi>
                  </p>
                )}
              </div>
            ) : null}
          </div>
        </div>

        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <strong>{t("diag.report")}</strong>
            <span>{t("diag.reportHint")}</span>
          </div>
          <div className="settings-page__row-control settings-page__row-control--stack">
            <span className="settings-page__button-row">
              <button type="button" className="settings-page__secondary-button" onClick={() => void toggleReport()}>
                <FileDown size={14} /> {report !== null ? t("diag.hide") : t("diag.show")}
              </button>
              <button type="button" className="settings-page__secondary-button" onClick={() => void copyReport()}>
                <Copy size={14} /> {t("diag.copy")}
              </button>
              <button type="button" className="settings-page__secondary-button" onClick={() => void saveReport()}>
                <Save size={14} /> {t("diag.save")}
              </button>
            </span>
          </div>
        </div>
        {report !== null ? (
          <textarea className="maintenance__report" dir="ltr" readOnly value={report} rows={16} aria-label={t("diag.report")} />
        ) : null}
      </div>
    </div>
  );
}
