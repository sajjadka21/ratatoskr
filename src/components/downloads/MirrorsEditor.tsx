import { Plus, Server, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";

/**
 * Other addresses of the same file. The engine checks each one against the
 * file (size and validator) before reading from it, so a wrong mirror is
 * ignored rather than trusted.
 */
export function MirrorsEditor({
  downloadId,
  onError,
}: {
  downloadId: string;
  onError: (message: string) => void;
}) {
  const { t } = useI18n();
  const [mirrors, setMirrors] = useState<string[]>([]);
  const [draft, setDraft] = useState("");

  useEffect(() => {
    let cancelled = false;
    setDraft("");
    invoke<string[]>("list_download_mirrors", { id: downloadId })
      .then((found) => {
        if (!cancelled) setMirrors(found);
      })
      .catch(() => {
        if (!cancelled) setMirrors([]);
      });
    return () => {
      cancelled = true;
    };
  }, [downloadId]);

  async function save(next: string[]) {
    try {
      setMirrors(await invoke<string[]>("set_download_mirrors", { id: downloadId, urls: next }));
      setDraft("");
    } catch (reason) {
      onError(String(reason));
    }
  }

  function add() {
    const urls = draft
      .split(/\s+/)
      .map((value) => value.trim())
      .filter(Boolean);
    if (urls.length) void save([...mirrors, ...urls]);
  }

  return (
    <section className="details__section">
      <h3>{t("mirrors.title")}</h3>
      <p className="details__note">{t("mirrors.hint")}</p>
      {mirrors.length ? (
        <ul className="details__mirrors">
          {mirrors.map((mirror) => (
            <li key={mirror}>
              <Server size={13} aria-hidden="true" />
              <span className="ltr details__value-clip" title={mirror}>
                {mirror}
              </span>
              <button
                type="button"
                className="details__copy"
                aria-label={t("mirrors.remove")}
                title={t("mirrors.remove")}
                onClick={() => void save(mirrors.filter((item) => item !== mirror))}
              >
                <Trash2 size={13} />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="details__mirror-add">
        <input
          dir="ltr"
          value={draft}
          placeholder="https://mirror.example.com/file.iso"
          aria-label={t("mirrors.add")}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") add();
          }}
        />
        <button type="button" onClick={add} disabled={!draft.trim()} aria-label={t("mirrors.add")} title={t("mirrors.add")}>
          <Plus size={14} />
        </button>
      </div>
    </section>
  );
}
