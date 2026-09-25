import { useEffect, useState } from "react";
import { FolderOpen, HardDrive, RotateCcw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import { useI18n } from "../../i18n/I18n";
import { categoryName } from "../../utils/categories";
import type { DownloadCategory } from "../../types/download";
import { pickFolder } from "../settings/SettingsPage";
import "./CategoriesPage.css";

type CategoriesPageProps = {
  onError: (message: string) => void;
};


export function CategoriesPage({ onError }: CategoriesPageProps) {
  const { t, fmt } = useI18n();
  const [categories, setCategories] = useState<DownloadCategory[]>([]);
  const [loaded, setLoaded] = useState(false);

  const nameOf = (category: DownloadCategory) => categoryName(category.id, category.name, t);

  useEffect(() => {
    void invoke<DownloadCategory[]>("list_categories")
      .then(setCategories)
      .catch((reason) => onError(t("categories.loadFailed", { reason: String(reason) })))
      .finally(() => setLoaded(true));
  }, [onError, t]);

  async function setDirectory(category: DownloadCategory, directory: string | null) {
    try {
      const saved = await invoke<DownloadCategory>("set_category_directory", {
        id: category.id,
        directory,
      });
      setCategories((current) => current.map((item) => (item.id === saved.id ? saved : item)));
    } catch (reason) {
      onError(t("categories.changeFailed", { reason: String(reason) }));
    }
  }

  async function chooseDirectory(category: DownloadCategory) {
    try {
      const folder = await pickFolder(t("categories.folderFor", { name: nameOf(category) }), category.defaultDirectory);
      if (folder) await setDirectory(category, folder);
    } catch (reason) {
      onError(t("settings.pickerFailed", { reason: String(reason) }));
    }
  }

  const withFolder = categories.filter((category) => category.defaultDirectory).length;

  return (
    <div className="categories-page">
      <header className="categories-page__intro">
        <div>
          <h2>{t("categories.title")}</h2>
          <p>{t("categories.hint")}</p>
        </div>
        <div className="categories-page__summary">
          <strong className="num">{fmt.number(withFolder)}</strong>
          <span>{t("categories.summary", { total: fmt.number(categories.length) })}</span>
        </div>
      </header>

      <div className="categories-page__grid">
        {categories.map((category) => (
          <article className="category-card" key={category.id}>
            <div className="category-card__icon">
              <FolderOpen size={18} />
            </div>
            <div className="category-card__body">
              <h3>{nameOf(category)}</h3>
              <p className="ltr">
                {category.extensions.length
                  ? category.extensions.map((extension) => `.${extension}`).join("  ")
                  : null}
              </p>
              {category.extensions.length === 0 ? <p>{t("categories.anything")}</p> : null}
              <span
                className={category.defaultDirectory ? "category-card__folder category-card__folder--set" : "category-card__folder"}
                title={category.defaultDirectory ?? undefined}
              >
                <HardDrive size={13} />
                <span className={category.defaultDirectory ? "ltr" : undefined}>
                  {category.defaultDirectory ?? t("categories.defaultFolder")}
                </span>
              </span>
              <div className="category-card__actions">
                <button type="button" className="category-card__button" onClick={() => void chooseDirectory(category)}>
                  {category.defaultDirectory ? t("categories.change") : t("categories.choose")}
                </button>
                {category.defaultDirectory ? (
                  <button
                    type="button"
                    className="category-card__button category-card__button--quiet"
                    aria-label={t("categories.reset", { name: nameOf(category) })}
                    title={t("categories.resetShort")}
                    onClick={() => void setDirectory(category, null)}
                  >
                    <RotateCcw size={13} />
                  </button>
                ) : null}
              </div>
            </div>
          </article>
        ))}
        {loaded && categories.length === 0 ? <div className="categories-page__empty">{t("categories.empty")}</div> : null}
      </div>
    </div>
  );
}
