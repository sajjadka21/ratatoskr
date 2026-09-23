import { useEffect, useState } from "react";
import { FolderOpen, HardDrive, RotateCcw, Tag } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import type { DownloadCategory } from "../../types/download";
import { pickFolder } from "../settings/SettingsPage";
import "./CategoriesPage.css";

type CategoriesPageProps = {
  onError: (message: string) => void;
};

export function CategoriesPage({ onError }: CategoriesPageProps) {
  const [categories, setCategories] = useState<DownloadCategory[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    void invoke<DownloadCategory[]>("list_categories")
      .then(setCategories)
      .catch((reason) => onError(`Could not load categories: ${String(reason)}`))
      .finally(() => setLoaded(true));
  }, [onError]);

  async function setDirectory(category: DownloadCategory, directory: string | null) {
    try {
      const saved = await invoke<DownloadCategory>("set_category_directory", {
        id: category.id,
        directory,
      });
      setCategories((current) =>
        current.map((item) => (item.id === saved.id ? saved : item)),
      );
    } catch (reason) {
      onError(`Could not change the folder: ${String(reason)}`);
    }
  }

  async function chooseDirectory(category: DownloadCategory) {
    try {
      const folder = await pickFolder(
        `Folder for ${category.name}`,
        category.defaultDirectory,
      );
      if (folder) await setDirectory(category, folder);
    } catch (reason) {
      onError(`Could not open the folder picker: ${String(reason)}`);
    }
  }

  const withFolder = categories.filter((category) => category.defaultDirectory).length;

  return (
    <div className="categories-page">
      <header className="categories-page__intro">
        <div>
          <span className="eyebrow">
            <Tag size={14} /> Filing system
          </span>
          <h2>Every kind of file in its own folder</h2>
          <p>
            A download is sorted by its type once the server says what it is.
            Give a category a folder and its files go there; without one they
            go to the default folder from Settings. Rules can override both.
          </p>
        </div>
        <div className="categories-page__summary">
          <strong>{withFolder}</strong>
          <span>of {categories.length} categories have their own folder</span>
        </div>
      </header>

      <div className="categories-page__grid">
        {categories.map((category) => (
          <article className="category-card" key={category.id}>
            <div className="category-card__icon">
              <FolderOpen size={18} />
            </div>
            <div className="category-card__body">
              <h3>{category.name}</h3>
              <p>
                {category.extensions.length
                  ? category.extensions.map((extension) => `.${extension}`).join("  ")
                  : "Anything no other category matches"}
              </p>
              <span
                className={
                  category.defaultDirectory
                    ? "category-card__folder category-card__folder--set"
                    : "category-card__folder"
                }
                title={category.defaultDirectory ?? undefined}
              >
                <HardDrive size={13} />
                {category.defaultDirectory ?? "Default folder"}
              </span>
              <div className="category-card__actions">
                <button
                  type="button"
                  className="category-card__button"
                  onClick={() => void chooseDirectory(category)}
                >
                  {category.defaultDirectory ? "Change folder…" : "Choose folder…"}
                </button>
                {category.defaultDirectory ? (
                  <button
                    type="button"
                    className="category-card__button category-card__button--quiet"
                    aria-label={`Use the default folder for ${category.name}`}
                    title="Use the default folder"
                    onClick={() => void setDirectory(category, null)}
                  >
                    <RotateCcw size={13} />
                  </button>
                ) : null}
              </div>
            </div>
          </article>
        ))}
        {loaded && categories.length === 0 ? (
          <div className="categories-page__empty">No categories configured yet.</div>
        ) : null}
      </div>
    </div>
  );
}
