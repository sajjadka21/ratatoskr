import { useEffect, useState } from "react";
import { FolderOpen, HardDrive, Tag } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type { DownloadCategory } from "../../types/download";
import "./CategoriesPage.css";

export function CategoriesPage() {
  const [categories, setCategories] = useState<DownloadCategory[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void invoke<DownloadCategory[]>("list_categories")
      .then(setCategories)
      .catch((reason) => setError(String(reason)));
  }, []);

  return <div className="categories-page">
    <header className="categories-page__intro"><div><span className="eyebrow"><Tag size={14} /> Filing system</span><h2>Categories that stay out of your way</h2><p>Rules are evaluated by the Rust engine. Use these defaults to keep documents, media, and archives in predictable places.</p></div><div className="categories-page__summary"><strong>{categories.length}</strong><span>configured categories</span></div></header>
    {error ? <div className="categories-page__error">{error}</div> : null}
    <div className="categories-page__grid">{categories.map((category) => <article className="category-card" key={category.id}><div className="category-card__icon"><FolderOpen size={18} /></div><div className="category-card__body"><h3>{category.name}</h3><p>{category.extensions.length ? category.extensions.join(" · ") : "Any matching file type"}</p><span><HardDrive size={13} /> {category.defaultDirectory ?? "Default download directory"}</span></div><div className="category-card__priority">{category.priority.replace("_", " ")}</div></article>)}{!categories.length && !error ? <div className="categories-page__empty">No categories configured yet.</div> : null}</div>
  </div>;
}
