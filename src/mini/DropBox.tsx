import { invoke } from "@tauri-apps/api/core";
import { Download, EyeOff, Maximize2 } from "lucide-react";
import { useEffect, useState } from "react";

import { useI18n } from "../i18n/I18n";

import "./DropBox.css";

/**
 * The floating drop box: a small round target that stays on top of other
 * windows. Links dragged onto it from a browser (or text holding links) go
 * where copied links go. Drag it by its edge to move it; right-click for
 * its menu.
 */
export function DropBox() {
  const { t } = useI18n();
  const [over, setOver] = useState(false);
  const [flash, setFlash] = useState<"ok" | "none" | null>(null);
  const [menu, setMenu] = useState(false);

  useEffect(() => {
    // The window is see-through around the circle.
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
  }, []);

  useEffect(() => {
    if (!flash) return;
    const timer = window.setTimeout(() => setFlash(null), 1200);
    return () => window.clearTimeout(timer);
  }, [flash]);

  function textOf(data: DataTransfer): string {
    return [data.getData("text/uri-list"), data.getData("text/plain"), data.getData("text/html")]
      .filter(Boolean)
      .join("\n");
  }

  return (
    <div
      className={`drop-box ${over ? "drop-box--over" : ""} ${flash ? `drop-box--${flash}` : ""}`}
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(event) => {
        event.preventDefault();
        setOver(false);
        const text = textOf(event.dataTransfer);
        void invoke<number>("add_dropped_links", { text })
          .then((count) => setFlash(count > 0 ? "ok" : "none"))
          .catch(() => setFlash("none"));
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu((open) => !open);
      }}
      onDoubleClick={() => void invoke("show_main_window").catch(() => {})}
      title={t("drop.hint")}
    >
      <div className="drop-box__circle" data-tauri-drag-region>
        <Download size={26} aria-hidden="true" />
      </div>
      {menu ? (
        <div className="drop-box__menu" role="menu">
          <button
            type="button"
            role="menuitem"
            title={t("drop.open")}
            aria-label={t("drop.open")}
            onClick={() => {
              setMenu(false);
              void invoke("show_main_window").catch(() => {});
            }}
          >
            <Maximize2 size={14} />
          </button>
          <button
            type="button"
            role="menuitem"
            title={t("drop.hide")}
            aria-label={t("drop.hide")}
            onClick={() => void invoke("set_drop_box", { enabled: false }).catch(() => {})}
          >
            <EyeOff size={14} />
          </button>
        </div>
      ) : null}
    </div>
  );
}
