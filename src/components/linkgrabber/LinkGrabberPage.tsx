import { useEffect, useMemo, useState } from "react";
import { Check, Copy, Filter, Radar, Wand2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import type { DownloadQueue } from "../../types/download";
import type { AddDownloadAction } from "../downloads/AddDownloadModal";
import { useI18n } from "../../i18n/I18n";
import { LARGE_BATCH, batchAction, readDroppedText } from "../../utils/linkgrabber";

import "./LinkGrabberPage.css";

/**
 * What was pasted survives leaving the page and coming back, for as long as
 * the app is open. It lives only in memory and is never saved.
 */
let draft = "";
let draftIntake = 0;

type LinkCandidate = { url: string; host: string; extension: string | null };

type LinkProbe = {
  url: string;
  reachable: boolean;
  filename: string | null;
  totalBytes: number | null;
  contentType: string | null;
  rangeSupported: boolean;
  error: string | null;
};

type Props = {
  queues: DownloadQueue[];
  engineReady: boolean;
  submitting?: boolean;
  onSubmit: (urls: string[], action: AddDownloadAction) => void;
  /// Links sent from the browser. Each new value is appended to the input.
  intake?: { id: number; urls: string[] } | null;
};

export function LinkGrabberPage({ queues, engineReady, submitting = false, onSubmit, intake = null }: Props) {
  const { t, fmt } = useI18n();
  const [input, setInput] = useState(() => draft);
  const [appliedIntake, setAppliedIntake] = useState(draftIntake);
  useEffect(() => {
    draft = input;
    draftIntake = appliedIntake;
  }, [input, appliedIntake]);

  useEffect(() => {
    if (!intake || intake.id === appliedIntake) return;
    setAppliedIntake(intake.id);
    setInput((current) => [current.trim(), ...intake.urls].filter(Boolean).join("\n"));
  }, [intake, appliedIntake]);
  const [candidates, setCandidates] = useState<LinkCandidate[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [queueId, setQueueId] = useState(queues[0]?.id ?? "");
  const [error, setError] = useState<string | null>(null);
  const [pattern, setPattern] = useState("");
  const [probes, setProbes] = useState<Map<string, LinkProbe>>(new Map());
  const [probing, setProbing] = useState(false);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    if (!queues.some((queue) => queue.id === queueId)) setQueueId(queues[0]?.id ?? "");
  }, [queues, queueId]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      if (!input.trim()) { setCandidates([]); setSelected(new Set()); return; }
      void invoke<LinkCandidate[]>("inspect_links", { input })
        .then((items) => { setCandidates(items); setSelected(new Set(items.map((item) => item.url))); setError(null); })
        .catch((reason) => setError(String(reason)));
    }, 180);
    return () => window.clearTimeout(timer);
  }, [input]);

  const types = useMemo(() => [...new Set(candidates.map((item) => item.extension ?? "other"))].sort(), [candidates]);
  const visible = useMemo(() => candidates.filter((item) => {
    const matchesQuery = !query.trim() || `${item.url} ${item.host}`.toLowerCase().includes(query.trim().toLowerCase());
    const matchesKind = kind === "all" || (item.extension ?? "other") === kind;
    return matchesQuery && matchesKind;
  }), [candidates, query, kind]);

  // Totals of what checking found, for the links that are selected.
  const checkedSummary = useMemo(() => {
    let bytes = 0;
    let known = 0;
    let unreachable = 0;
    for (const url of selected) {
      const probe = probes.get(url);
      if (!probe) continue;
      if (!probe.reachable) unreachable += 1;
      else if (probe.totalBytes !== null) { bytes += probe.totalBytes; known += 1; }
    }
    return { bytes, known, unreachable };
  }, [selected, probes]);

  function toggle(url: string) {
    setSelected((current) => { const next = new Set(current); if (next.has(url)) next.delete(url); else next.add(url); return next; });
  }

  function appendLinks(text: string) {
    setInput((current) => [current.trim(), text.trim()].filter(Boolean).join("\n"));
  }

  async function generate() {
    try {
      const links = await invoke<LinkCandidate[]>("generate_links", { pattern });
      appendLinks(links.map((link) => link.url).join("\n"));
      setPattern("");
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function checkSelected() {
    const urls = [...selected];
    if (!urls.length || probing) return;
    setProbing(true);
    try {
      const results = await invoke<LinkProbe[]>("probe_links", { urls });
      setProbes((current) => {
        const next = new Map(current);
        for (const result of results) next.set(result.url, result);
        return next;
      });
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setProbing(false);
    }
  }

  async function handleDrop(event: React.DragEvent<HTMLTextAreaElement>) {
    event.preventDefault();
    setDragging(false);
    try {
      const text = await readDroppedText(event.dataTransfer);
      if (text) appendLinks(text);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  const defaultQueueId = queues.find((queue) => queue.id === "default")?.id ?? queues[0]?.id ?? null;
  const startAction = batchAction(selected.size, queueId || null, defaultQueueId);
  const routedThroughDefault = !queueId && startAction.action.kind === "queue";
  const startLabel = queueId
    ? t("grabber.sendToQueue")
    : routedThroughDefault
      ? t("grabber.startViaQueue", { count: fmt.number(selected.size) })
      : t("grabber.start");

  function submit(action: AddDownloadAction) {
    if (!engineReady || submitting || selected.size === 0) return;
    onSubmit([...selected], action);
  }

  return <div className="linkgrabber-page">
    <section className="linkgrabber-page__hero">
      <div>
        <h2>{t("grabber.title")}</h2>
        <p>{t("grabber.hint")}</p>
      </div>
    </section>

    <section className="linkgrabber-page__workbench">
      <textarea
        className={dragging ? "is-dragging" : undefined}
        value={input}
        dir="ltr"
        onChange={(event) => setInput(event.target.value)}
        onDragOver={(event) => { event.preventDefault(); setDragging(true); }}
        onDragLeave={() => setDragging(false)}
        onDrop={(event) => void handleDrop(event)}
        placeholder={t("grabber.placeholder")}
        aria-label={t("grabber.inputLabel")}
      />
      <div className="linkgrabber-page__generator">
        <Wand2 size={14} />
        <input
          value={pattern}
          dir="ltr"
          onChange={(event) => setPattern(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter" && pattern.trim()) void generate(); }}
          placeholder={t("grabber.patternPlaceholder")}
          aria-label={t("grabber.patternLabel")}
        />
        <button type="button" onClick={() => void generate()} disabled={!pattern.trim()}>{t("grabber.generate")}</button>
      </div>
      <div className="linkgrabber-page__toolbar">
        <span className="num">{t("grabber.counts", { count: fmt.number(candidates.length), selected: fmt.number(selected.size) })}</span>
        <button type="button" onClick={() => navigator.clipboard?.writeText([...selected].join("\n"))} disabled={!selected.size}><Copy size={14} /> {t("grabber.copySelected")}</button>
      </div>
    </section>

    <section className="linkgrabber-page__results">
      <div className="linkgrabber-page__results-head">
        <div><strong>{t("grabber.candidates")}</strong><span>{t("grabber.review")}</span></div>
        <div className="linkgrabber-page__filters"><label><Filter size={14} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("grabber.filter")} aria-label={t("grabber.filter")} /></label><select value={kind} onChange={(event) => setKind(event.target.value)}><option value="all">{t("grabber.allTypes")}</option>{types.map((type) => <option key={type} value={type}>{type}</option>)}</select></div>
      </div>
      <div className="linkgrabber-page__select-row">
        <button type="button" onClick={() => setSelected(new Set(visible.map((item) => item.url)))}><Check size={14} /> {t("grabber.selectVisible")}</button>
        <button type="button" onClick={() => setSelected(new Set())}>{t("grabber.clear")}</button>
        <button type="button" onClick={() => void checkSelected()} disabled={!selected.size || probing}><Radar size={14} /> {probing ? t("grabber.checking") : t("grabber.check")}</button>
        {checkedSummary.known || checkedSummary.unreachable ? (
          <span className="linkgrabber-page__summary num">
            {checkedSummary.known ? t("grabber.checkedTotal", { size: fmt.bytes(checkedSummary.bytes), count: fmt.number(checkedSummary.known) }) : null}
            {checkedSummary.unreachable ? <em>{t("grabber.unreachableCount", { count: fmt.number(checkedSummary.unreachable) })}</em> : null}
          </span>
        ) : null}
      </div>
      {error ? <div className="linkgrabber-page__error">{error}</div> : null}
      <div className="linkgrabber-page__list">{visible.map((item) => {
        const probe = probes.get(item.url);
        return <label key={item.url} className={`linkgrabber-page__item${probe && !probe.reachable ? " is-unreachable" : ""}`}>
          <input type="checkbox" checked={selected.has(item.url)} onChange={() => toggle(item.url)} />
          <span className="linkgrabber-page__item-main">
            <strong className="ltr">{probe?.filename ?? item.host}</strong>
            <span className="ltr">{item.url}</span>
          </span>
          <span className="linkgrabber-page__item-facts">
            {probe ? (probe.reachable ? <>
              {probe.totalBytes !== null ? <b className="num">{fmt.bytes(probe.totalBytes)}</b> : null}
              <i className={probe.rangeSupported ? "is-good" : undefined}>{probe.rangeSupported ? t("grabber.resumable") : t("grabber.noResume")}</i>
            </> : <i className="is-bad" title={probe.error ?? undefined}>{t("grabber.unreachable")}</i>) : null}
            <code>{item.extension ?? t("grabber.other")}</code>
          </span>
        </label>;
      })}{!visible.length ? <div className="linkgrabber-page__empty">{t("grabber.empty")}</div> : null}</div>
      <div className="linkgrabber-page__actions">
        {routedThroughDefault ? <span className="linkgrabber-page__note">{t("grabber.largeBatchNote", { count: fmt.number(LARGE_BATCH) })}</span> : null}
        <select value={queueId} onChange={(event) => setQueueId(event.target.value)} aria-label={t("grabber.targetQueue")}><option value="">{t("grabber.noQueue")}</option>{queues.map((queue) => <option key={queue.id} value={queue.id}>{queue.name}</option>)}</select>
        <button type="button" className="secondary" disabled={!engineReady || submitting || !selected.size} onClick={() => submit({ kind: "download-later" })}>{t("grabber.later")}</button>
        <button type="button" disabled={!engineReady || submitting || !selected.size} onClick={() => submit(startAction.action)}>{startLabel}</button>
      </div>
    </section>
  </div>;
}
