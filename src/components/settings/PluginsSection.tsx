import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../../i18n/I18n";
import { Switch } from "./Switch";

type InstalledPlugin = { id: string; name: string; version: string; rules: number; enabled: boolean };

/**
 * Plugins are small JSON files of rules (rewrite a link, rename a file, send a
 * Referer or User-Agent for one site). They hold data only, never code, and the
 * backend checks every file before it keeps it.
 */
export function PluginsSection({ onError }: { onError: (message: string) => void }) {
  const { t } = useI18n();
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);

  useEffect(() => {
    invoke<InstalledPlugin[]>("list_plugins").then(setPlugins).catch((reason) => onError(String(reason)));
  }, [onError]);

  const apply = (command: string, args: Record<string, unknown>) =>
    invoke<InstalledPlugin[]>(command, args).then(setPlugins).catch((reason) => onError(String(reason)));

  async function add() {
    const picked = await openDialog({ multiple: false, filters: [{ name: "Plugin", extensions: ["json"] }] }).catch(() => null);
    if (typeof picked === "string") void apply("import_plugin", { path: picked });
  }

  return (
    <div className="settings-page__section">
      <div className="settings-page__section-heading">
        <h2>{t("plugins.title")}</h2>
        <p>{t("plugins.hint")}</p>
      </div>
      <div className="settings-page__group settings-page__rows">
        {plugins.length === 0 ? (
          <div className="settings-page__row">
            <div className="settings-page__row-label">
              <span>{t("plugins.none")}</span>
            </div>
          </div>
        ) : (
          plugins.map((plugin) => (
            <div className="settings-page__row" key={plugin.id}>
              <div className="settings-page__row-label">
                <strong>{plugin.name}</strong>
                <span className="ltr num">
                  {plugin.id} · {plugin.version || "—"} · {t("plugins.rules", { count: String(plugin.rules) })}
                </span>
              </div>
              <div className="settings-page__row-control">
                <Switch
                  id={`plugin-${plugin.id}`}
                  checked={plugin.enabled}
                  label={plugin.name}
                  onChange={(enabled) => void apply("set_plugin_enabled", { id: plugin.id, enabled })}
                />
                <button
                  type="button"
                  className="settings-page__secondary-button"
                  aria-label={t("plugins.remove")}
                  onClick={() => void apply("remove_plugin", { id: plugin.id })}
                >
                  <Trash2 size={15} />
                </button>
              </div>
            </div>
          ))
        )}
        <div className="settings-page__row">
          <div className="settings-page__row-label">
            <span>{t("plugins.format")}</span>
          </div>
          <div className="settings-page__row-control">
            <button type="button" className="settings-page__primary-button" onClick={() => void add()}>
              <Plus size={15} /> {t("plugins.add")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
