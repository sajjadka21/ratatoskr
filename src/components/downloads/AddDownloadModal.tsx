import {
  ArrowLeft,
  Check,
  CheckCheck,
  ChevronDown,
  Clock3,
  Download,
  Layers3,
  ListChecks,
  Link2,
  Plus,
  X,
} from "lucide-react";

import {
  useLayoutEffect,
  useEffect,
  useRef,
  useState,
} from "react";

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { extractHttpUrls } from "../../utils/downloadLinks";

import { useI18n } from "../../i18n/I18n";

import { StreamQualityPicker } from "./StreamQualityPicker";
import { VideoQualityPicker } from "./VideoQualityPicker";
import { withVideoQualityForAll, type VideoQuality } from "../../utils/videoPages";

import "./AddDownloadModal.css";
import type { DownloadQueue } from "../../types/download";

export type AddDownloadAction =
  | { kind: "start-now" }
  | { kind: "download-later" }
  | { kind: "queue"; queueId: string }
  | { kind: "create-queue"; queueName: string };
export type DuplicateChoice = "all" | "new-only" | "single-copy";

type AddDownloadModalProps = {
  open: boolean;
  url: string;
  submitting: boolean;
  engineReady: boolean;
  error: string | null;
  linkCount: number;
  /** How many of the links are already in the downloads list. */
  duplicateCount?: number;
  duplicateUrls?: string[];
  completedDuplicateCount?: number;
  queues: DownloadQueue[];
  /// Folder used when no category or rule names one; null is the system
  /// Downloads folder.
  defaultDirectory?: string | null;
  onUrlChange: (value: string) => void;
  onClose: () => void;
  /**
   * `links` is the text to add, with any quality chosen here attached;
   * `folder` is a folder chosen here, or null for the usual one.
   */
  onSubmit: (action: AddDownloadAction, links: string, folder: string | null, duplicateChoice: DuplicateChoice) => void;
};

export function AddDownloadModal({
  open,
  url,
  submitting,
  engineReady,
  error,
  linkCount,
  duplicateCount = 0, duplicateUrls = [], completedDuplicateCount = 0,
  queues,
  defaultDirectory = null,
  onUrlChange,
  onClose,
  onSubmit,
}: AddDownloadModalProps) {
  const { t, fmt } = useI18n();
  const inputRef =
    useRef<HTMLTextAreaElement>(null);
  const actionRef =
    useRef<HTMLDivElement>(null);
  const actionMenuRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const [actionMenuOpen, setActionMenuOpen] =
    useState(false);
  const [actionMenuPosition, setActionMenuPosition] = useState<{ top: number; left: number; width: number; maxHeight: number } | null>(null);
  const [newQueueName, setNewQueueName] = useState("");
  const [selectionOpen, setSelectionOpen] = useState(false);
  const [linkSelection, setLinkSelection] = useState<{
    source: string;
    links: string[];
    confirmed: boolean;
  } | null>(null);
  const selectionHeadingRef = useRef<HTMLHeadingElement>(null);
  const [videoQuality, setVideoQuality] = useState<VideoQuality | null>(null);
  const [folder, setFolder] = useState<string | null>(null);
  const [folderInspection, setFolderInspection] = useState<{ matches: { name: string; folder: string; exact: boolean }[]; partial: boolean } | null>(null);
  const [folderChecking, setFolderChecking] = useState(false);
  useEffect(() => {
    setFolderInspection(null);
    const urls = extractHttpUrls(url).slice(0, 200);
    if (!open || !urls.length) { setFolderChecking(false); return; }
    let current = true;
    setFolderChecking(true);
    const timer = window.setTimeout(() => {
      void invoke<{ matches: { name: string; folder: string; exact: boolean }[]; partial: boolean }>("inspect_existing_files", { urls, directory: folder })
        .then((result) => { if (current) setFolderInspection(result); })
        .catch(() => { if (current) setFolderInspection({ matches: [], partial: true }); })
        .finally(() => { if (current) setFolderChecking(false); });
    }, 400);
    return () => { current = false; window.clearTimeout(timer); };
  }, [open, url, folder]);
  const [playlist, setPlaylist] = useState<string[] | null>(null);
  const [lookingUp, setLookingUp] = useState(false);
  const [duplicateAcknowledged, setDuplicateAcknowledged] = useState(false);
  const parsedLinks = extractHttpUrls(url);
  const isBatch = linkCount > 1;
  const duplicateKeys = new Set(duplicateUrls.map((link) => link.split("#")[0]!));
  const historyDuplicates = parsedLinks.filter((link) => duplicateKeys.has(link.split("#")[0]!));
  const defaultSelectedLinks = isBatch && historyDuplicates.length > 0
    ? parsedLinks.filter((link) => !duplicateKeys.has(link.split("#")[0]!))
    : parsedLinks;
  const selectedBatchLinks = parsedLinks.filter((link) =>
    (linkSelection?.source === url ? linkSelection.links : defaultSelectedLinks).includes(link),
  );
  const selectedDuplicateCount = selectedBatchLinks.filter((link) => duplicateKeys.has(link.split("#")[0]!)).length;
  const selectionConfirmed = linkSelection?.source === url && linkSelection.confirmed;
  const batchNeedsReview = isBatch && historyDuplicates.length > 0 && !selectionConfirmed;
  useEffect(() => {
    setDuplicateAcknowledged(false);
    setLinkSelection(null);
    setSelectionOpen(false);
  }, [url, open, duplicateCount, completedDuplicateCount]);

  useEffect(() => {
    if (selectionOpen) selectionHeadingRef.current?.focus();
  }, [selectionOpen]);

  useEffect(() => {
    if (!open) return;
    setVideoQuality(null);
    setFolder(null);
    setPlaylist(null);
    setLookingUp(false);
  }, [open]);

  function submit(action: AddDownloadAction) {
    if (batchNeedsReview || (selectedDuplicateCount > 0 && !isBatch && !duplicateAcknowledged)) return;
    // A playlist link adds each of its videos, not just the first.
    const text = playlist && linkCount === 1
      ? playlist.join("\n")
      : isBatch
        ? selectedBatchLinks.join("\n")
        : url;
    onSubmit(
      action,
      videoQuality === null ? text : withVideoQualityForAll(text, videoQuality),
      folder,
      isBatch ? (selectedDuplicateCount > 0 ? "all" : "new-only") : selectedDuplicateCount > 0 ? "single-copy" : "new-only",
    );
  }

  function selectBatchLinks(links: string[]) {
    setLinkSelection({ source: url, links, confirmed: false });
  }

  function toggleBatchLink(link: string, checked: boolean) {
    const selected = new Set(selectedBatchLinks);
    if (checked) selected.add(link);
    else selected.delete(link);
    selectBatchLinks(parsedLinks.filter((item) => selected.has(item)));
  }

  function describeLink(link: string) {
    try {
      const parsed = new URL(link);
      const pathParts = parsed.pathname.split("/").filter(Boolean);
      const path = pathParts[pathParts.length - 1];
      const name = path ? decodeURIComponent(path) : parsed.hostname;
      return { name, address: parsed.hostname + parsed.pathname };
    } catch {
      return { name: link, address: link };
    }
  }

  async function chooseFolder() {
    try {
      const chosen = await openDialog({
        directory: true,
        multiple: false,
        title: t("add.chooseFolderTitle"),
        defaultPath: folder ?? defaultDirectory ?? undefined,
      });
      if (typeof chosen === "string" && chosen) setFolder(chosen);
    } catch {
      // The picker closing without a choice is not an error worth showing.
    }
  }

  // Until a playlist or channel link is looked up, adding it would take
  // only its first video.
  const waitingForList = lookingUp && linkCount === 1 && looksLikeList(url);
  const actionsDisabled =
    submitting || !engineReady || linkCount === 0 || (isBatch && selectedBatchLinks.length === 0) || waitingForList || batchNeedsReview || (selectedDuplicateCount > 0 && !isBatch && !duplicateAcknowledged);

  useEffect(() => {
    if (!open) return;

    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = dialogRef.current;
    if (!dialog) return;
    const inertSiblings: Array<[HTMLElement, boolean]> = [];
    for (let node: HTMLElement | null = dialog.parentElement; node?.parentElement; node = node.parentElement) {
      for (const sibling of Array.from(node.parentElement.children)) {
        if (sibling instanceof HTMLElement && sibling !== node && !["SCRIPT", "STYLE"].includes(sibling.tagName)) {
          inertSiblings.push([sibling, sibling.inert]);
          sibling.inert = true;
        }
      }
    }
    function controls() {
      return Array.from(dialog!.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])',
      )).filter((element) => {
        const style = getComputedStyle(element);
        return element.tabIndex >= 0 && !element.closest('[hidden], [inert]') && style.display !== "none" && style.visibility !== "hidden";
      });
    }
    function containFocus(event: FocusEvent) {
      if (event.target instanceof Node && !dialog!.contains(event.target)) (controls()[0] ?? dialog!).focus();
    }
    function cycleFocus(event: KeyboardEvent) {
      if (event.key !== "Tab") return;
      const items = controls();
      const current = document.activeElement;
      if (!items.length) {
        event.preventDefault();
        dialog!.focus();
      } else if (event.shiftKey && (current === items[0] || !items.includes(current as HTMLElement))) {
        event.preventDefault();
        items[items.length - 1].focus();
      } else if (!event.shiftKey && (current === items[items.length - 1] || !items.includes(current as HTMLElement))) {
        event.preventDefault();
        items[0].focus();
      }
    }
    document.addEventListener("focusin", containFocus);
    document.addEventListener("keydown", cycleFocus, true);
    (inputRef.current && !inputRef.current.disabled ? inputRef.current : controls()[0] ?? dialog).focus();

    const timer = window.setTimeout(() => {
      inputRef.current?.focus();

      const length =
        inputRef.current?.value.length ?? 0;

      inputRef.current?.setSelectionRange(
        length,
        length,
      );
    }, 60);

    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("focusin", containFocus);
      document.removeEventListener("keydown", cycleFocus, true);
      for (const [element, wasInert] of inertSiblings) element.inert = wasInert;
      if (opener?.isConnected) opener.focus();
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape" || submitting) return;

      if (selectionOpen) {
        setSelectionOpen(false);
        return;
      }

      if (actionMenuOpen) {
        setActionMenuOpen(false);
        return;
      }

      onClose();
    }

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [actionMenuOpen, onClose, open, selectionOpen, submitting]);

  useEffect(() => {
    if (!actionMenuOpen) return;

    function handlePointerDown(event: PointerEvent) {
      if (
        event.target instanceof Node &&
        !actionRef.current?.contains(event.target)
      ) {
        setActionMenuOpen(false);
      }
    }

    window.addEventListener("pointerdown", handlePointerDown);

    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [actionMenuOpen]);

  useLayoutEffect(() => {
    if (!actionMenuOpen) { setActionMenuPosition(null); return; }
    const positionMenu = () => {
      const trigger = actionRef.current?.getBoundingClientRect();
      const menu = actionMenuRef.current;
      if (!trigger || !menu) return;
      const margin = 12;
      const width = Math.min(292, window.innerWidth - 32);
      const maxHeight = Math.max(160, window.innerHeight - margin * 2);
      const height = Math.min(menu.scrollHeight, maxHeight);
      const left = Math.max(margin, Math.min(trigger.right - width, window.innerWidth - width - margin));
      const above = trigger.top - height - 8;
      const top = above >= margin ? above : Math.min(trigger.bottom + 8, window.innerHeight - height - margin);
      setActionMenuPosition({ top, left, width, maxHeight });
    };
    const frame = window.requestAnimationFrame(positionMenu);
    window.addEventListener("resize", positionMenu);
    window.addEventListener("scroll", positionMenu, true);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", positionMenu);
      window.removeEventListener("scroll", positionMenu, true);
    };
  }, [actionMenuOpen, queues.length]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="add-download-modal__backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (
          event.target === event.currentTarget &&
          !submitting
        ) {
          onClose();
        }
      }}
    >
      <section
        ref={dialogRef}
        tabIndex={-1}
        className="add-download-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-download-title"
      >
        <header className="add-download-modal__header">
          <div className="add-download-modal__heading">
            <div className="add-download-modal__icon">
              {isBatch ? (
                <Layers3 size={20} />
              ) : (
                <Link2 size={20} />
              )}
            </div>

            <div>
              <div className="add-download-modal__title-row">
                <h2 id="add-download-title">
                  {isBatch ? t("add.titleBatch", { count: fmt.number(linkCount) }) : t("add.title")}
                </h2>


              </div>

              <p>
                {isBatch
                  ? t("add.subtitleBatch", { count: fmt.number(linkCount) })
                  : t("add.subtitle")}
              </p>
            </div>
          </div>

          <button
            type="button"
            className="add-download-modal__close"
            onClick={onClose}
            disabled={submitting}
            aria-label={t("add.close")}
          >
            <X size={18} />
          </button>
        </header>

        <div className="add-download-modal__body">
          {selectionOpen ? (
            <section className="add-download-modal__selection" aria-labelledby="add-download-selection-title">
              <div className="add-download-modal__selection-heading">
                <h3 id="add-download-selection-title" ref={selectionHeadingRef} tabIndex={-1}>{t("add.selectLinks")}</h3>
                <p>{t("add.selectedCount", { selected: fmt.number(selectedBatchLinks.length), total: fmt.number(parsedLinks.length) })}</p>
              </div>
              <div className="add-download-modal__selection-tools">
                <button type="button" onClick={() => selectBatchLinks(parsedLinks)}><CheckCheck size={15} />{t("add.selectAll")}</button>
                <button type="button" onClick={() => selectBatchLinks(defaultSelectedLinks)}><ListChecks size={15} />{t("add.selectNewOnly")}</button>
                <button type="button" onClick={() => selectBatchLinks([])}>{t("add.selectNone")}</button>
              </div>
              {historyDuplicates.length > 0 ? <p className="add-download-modal__selection-hint">{t("add.duplicateHistoryOnly")}</p> : null}
              <div className="add-download-modal__selection-list" role="group" aria-label={t("add.selectLinks")}>
                {parsedLinks.map((link, index) => {
                  const duplicateIndex = historyDuplicates.findIndex((duplicate) => duplicate.split("#")[0] === link.split("#")[0]);
                  const duplicate = duplicateIndex >= 0;
                  const description = describeLink(link);
                  const selected = selectedBatchLinks.includes(link);
                  return (
                    <label key={link} className="add-download-modal__selection-row">
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={(event) => toggleBatchLink(link, event.target.checked)}
                        aria-label={t("add.selectOne", { number: fmt.number(index + 1), name: description.name })}
                      />
                      <span className="add-download-modal__selection-number">{fmt.number(index + 1)}</span>
                      <span className="add-download-modal__selection-copy">
                        <strong title={description.name}>{description.name}</strong>
                        <small dir="ltr" title={description.address}>{description.address}</small>
                      </span>
                      {duplicate ? <span className="add-download-modal__duplicate-tag">{t("add.duplicateTagNumber", { number: fmt.number(duplicateIndex + 1) })}</span> : <span className="add-download-modal__new-tag">{t("add.newLinkTag")}</span>}
                      {selected ? <Check size={14} className="add-download-modal__selection-check" aria-hidden="true" /> : null}
                    </label>
                  );
                })}
              </div>
            </section>
          ) : (
            <>
          <label className="add-download-modal__label" htmlFor="add-download-links">
            {isBatch ? t("add.links") : t("add.url")}
          </label>

          <div
            className={`add-download-modal__input-wrap ${
              isBatch
                ? "add-download-modal__input-wrap--batch"
                : ""
            }`}
          >
            <Link2 size={16} />

            <textarea
              ref={inputRef}
              id="add-download-links"
              dir="ltr"
              value={url}
              rows={isBatch ? Math.min(Math.max(linkCount, 3), 7) : 1}
              onChange={(event) => onUrlChange(event.target.value)}
              onKeyDown={(event) => {
                if (
                  event.key === "Enter" &&
                  !event.shiftKey &&
                  !isBatch &&
                  linkCount === 1 &&
                  !actionsDisabled
                ) {
                  event.preventDefault();
                  submit({ kind: "start-now" });
                }
              }}
              disabled={submitting}
              placeholder={
                isBatch ? t("add.placeholderBatch") : t("add.placeholder")
              }
            />
          </div>

          {linkCount > 0 ? (
            <div className="add-download-modal__detection">
              {isBatch ? (
                <>
                  <Layers3 size={13} />
                  <span>{t("add.detectedBatch", { count: fmt.number(linkCount) })}</span>
                </>
              ) : (
                <>
                  <Link2 size={13} />
                  <span>{t("add.detectedOne")}</span>
                </>
              )}
            </div>
          ) : url.trim() ? (
            <div className="add-download-modal__detection add-download-modal__detection--warning">
              {t("add.noneValid")}
            </div>
          ) : null}

          {isBatch ? (
            <button
              type="button"
              className="add-download-modal__manage-links"
              onClick={() => setSelectionOpen(true)}
              disabled={submitting}
            >
              <ListChecks size={15} />
              <span>{t("add.chooseLinkCount", { selected: fmt.number(selectedBatchLinks.length), total: fmt.number(parsedLinks.length) })}</span>
            </button>
          ) : null}

          {historyDuplicates.length > 0 ? (
            <div className="add-download-modal__duplicate" role="status">
              <strong>{isBatch ? t("add.duplicateCount", { count: fmt.number(historyDuplicates.length) }) : completedDuplicateCount > 0 ? t("add.duplicateCompleted", { count: fmt.number(completedDuplicateCount) }) : t("add.duplicateOne")}</strong>
              {isBatch ? (
                <span>{t("add.selectedCount", { selected: fmt.number(selectedBatchLinks.length), total: fmt.number(parsedLinks.length) })}</span>
              ) : (
                <label><input type="checkbox" checked={duplicateAcknowledged} onChange={(event) => setDuplicateAcknowledged(event.target.checked)} />{t("add.duplicateConfirm")}</label>
            )}

            </div>
          ) : null}

            <div className="add-download-modal__folder-inspection" role="status" aria-live="polite">
              {folderChecking ? <span>{t("add.folderChecking")}</span> : folderInspection ? <>
                {folderInspection.matches.length > 0 ? <>
                  <strong>{t("add.folderFound")}</strong>
                  {[...new Map(folderInspection.matches.map((match) => [match.folder + match.name, match])).values()].slice(0, 3).map((match) => <div key={match.folder + match.name}>
                    <span dir="ltr">{match.name}</span><small dir="ltr">{match.folder}</small>
                  </div>)}
                  {new Set(folderInspection.matches.map((match) => match.folder + match.name)).size > 3 ? <span>{t("add.folderFoundMore", { count: fmt.number(new Set(folderInspection.matches.map((match) => match.folder + match.name)).size - 3) })}</span> : null}
                  <span>{t("add.folderHint")}</span>
                </> : null}
                {folderInspection.partial ? <span>{t("add.folderPartial")}</span> : null}
              </> : null}
            </div>
          {linkCount === 1 ? (
            <StreamQualityPicker url={url} onChoose={onUrlChange} />
          ) : null}

          {linkCount > 0 ? (
            <VideoQualityPicker
              text={url}
              quality={videoQuality}
              onQualityChange={setVideoQuality}
              onReplaceLinks={onUrlChange}
              onPlaylist={setPlaylist}
              onBusy={setLookingUp}
            />
          ) : null}

          <div className="add-download-modal__destination">
            <span>{t("add.destination")}</span>
            <strong
              title={folder ?? defaultDirectory ?? undefined}
              className={folder || defaultDirectory ? "ltr" : undefined}
            >
              {folder ?? defaultDirectory ?? t("add.systemDownloads")}
            </strong>
            <small>{folder ? t("add.chosenFolderHint") : t("add.destinationHint")}</small>
            <span className="add-download-modal__folder-actions">
              {folder ? (
                <button type="button" onClick={() => setFolder(null)} disabled={submitting}>
                  {t("add.resetFolder")}
                </button>
              ) : null}
              <button type="button" onClick={() => void chooseFolder()} disabled={submitting}>
                {t("add.changeFolder")}
              </button>
            </span>
          </div>

          {error ? (
            <div className="add-download-modal__error">
              {error}
            </div>
          ) : null}
            </>
          )}
        </div>

        <footer className="add-download-modal__footer">
          {selectionOpen ? (
            <>
              <button type="button" className="add-download-modal__cancel" onClick={() => setSelectionOpen(false)}>
                <ArrowLeft size={15} />{t("add.selectionBack")}
              </button>
              <button
                type="button"
                className="add-download-modal__submit"
                disabled={selectedBatchLinks.length === 0}
                onClick={() => {
                  setLinkSelection({ source: url, links: selectedBatchLinks, confirmed: true });
                  setSelectionOpen(false);
                }}
              >
                {t("add.selectionApply", { count: fmt.number(selectedBatchLinks.length) })}
              </button>
            </>
          ) : (
            <>
          <button
            type="button"
            className="add-download-modal__cancel"
            onClick={onClose}
            disabled={submitting}
          >
            {t("add.cancel")}
          </button>

          <button type="button" className="add-download-modal__cancel" disabled={actionsDisabled} onClick={() => submit({ kind: "download-later" })}><Clock3 size={16} />{t("add.later")}</button>

          <div
            ref={actionRef}
            className="add-download-modal__split"
          >
            <button
              type="button"
              className="add-download-modal__submit"
              onClick={() => submit({ kind: "start-now" })}
              disabled={actionsDisabled}
            >
              {isBatch ? (
                <Layers3 size={16} />
              ) : (
                <Download size={16} />
              )}

              {submitting
                ? t("add.creating")
                : isBatch
                  ? t("add.startBatch", { count: fmt.number(selectedBatchLinks.length) })
                  : t("add.start")}
            </button>

            <button
              type="button"
              className="add-download-modal__dropdown-toggle"
              aria-label={t("add.chooseQueue")}
              aria-haspopup="menu"
              aria-expanded={actionMenuOpen}
              onClick={() => setActionMenuOpen((current) => !current)}
              disabled={actionsDisabled}
            >
              <Layers3 size={16} /><span>{t("add.chooseQueue")}</span><ChevronDown size={14} />
            </button>

            {actionMenuOpen ? (
              <div
                className="add-download-modal__action-menu"
                role="menu"
                ref={actionMenuRef}
                style={actionMenuPosition ? { top: actionMenuPosition.top, left: actionMenuPosition.left, width: actionMenuPosition.width, maxHeight: actionMenuPosition.maxHeight } : { visibility: "hidden" }}
              >
                {queues.map((queue) => (
                  <button
                    key={queue.id}
                    type="button"
                    role="menuitem"
                    className="add-download-modal__queue-option"
                    onClick={() => {
                      setActionMenuOpen(false);
                      submit({ kind: "queue", queueId: queue.id });
                    }}
                  >
                    <Layers3 size={16} />
                    <strong>{queue.name}</strong>
                    <span>{t("add.queueAddAction")}</span>
                  </button>
                ))}

                <div className="add-download-modal__create-queue">
                  <input
                    value={newQueueName}
                    onChange={(event) => setNewQueueName(event.target.value)}
                    onKeyDown={(event) => event.stopPropagation()}
                    placeholder={t("add.newQueue")}
                    aria-label={t("add.newQueue")}
                  />
                  <button
                    type="button"
                    disabled={!newQueueName.trim()}
                    onClick={() => {
                      const queueName = newQueueName.trim();
                      if (!queueName) return;
                      setActionMenuOpen(false);
                      setNewQueueName("");
                      submit({ kind: "create-queue", queueName });
                    }}
                  >
                    <Plus size={14} />
                    {t("add.createQueue")}
                  </button>
                </div>
              </div>
            ) : null}
          </div>
            </>
          )}
        </footer>
      </section>
    </div>
  );
}

/** A playlist or channel link, which holds many videos. */
function looksLikeList(text: string): boolean {
  try {
    const url = new URL(text.trim());
    const path = url.pathname.toLowerCase();
    return (
      path.startsWith("/playlist") ||
      path.startsWith("/@") ||
      path.startsWith("/channel/") ||
      path.startsWith("/c/") ||
      path.startsWith("/user/") ||
      (url.searchParams.has("list") && !url.searchParams.has("v"))
    );
  } catch {
    return false;
  }
}
