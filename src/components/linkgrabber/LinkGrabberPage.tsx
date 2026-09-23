import { useEffect, useMemo, useState } from "react";
import { Check, Copy, Filter, Radar, ScanLine, Sparkles, Wand2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import type { DownloadQueue } from "../../types/download";
import type { AddDownloadAction } from "../downloads/AddDownloadModal";
import { formatBytes } from "../../utils/format";
import { batchAction, readDroppedText } from "../../utils/linkgrabber";
import "./LinkGrabberPage.css";

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
  const [input, setInput] = useState(() => intake?.urls.join("\n") ?? "");
  const [appliedIntake, setAppliedIntake] = useState(intake?.id ?? 0);

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
      setError(String(reason));
    }
  }

  const defaultQueueId = queues.find((queue) => queue.id === "default")?.id ?? queues[0]?.id ?? null;
  const startAction = batchAction(selected.size, queueId || null, defaultQueueId);

  function submit(action: AddDownloadAction) {
    if (!engineReady || submitting || selected.size === 0) return;
    onSubmit([...selected], action);
  }

  return <div className="linkgrabber-page">
    <section className="linkgrabber-page__hero">
      <div>
        <span className="eyebrow"><ScanLine size={14} /> Capture desk</span>
        <h2>Find every downloadable link</h2>
        <p>Paste a page, a copied list, or raw HTML, or drop a text file. The Rust engine will normalize, deduplicate, and classify candidates before anything is created.</p>
      </div>
      <div className="linkgrabber-page__hero-mark"><Sparkles size={28} /><span>LOCAL<br />ANALYSIS</span></div>
    </section>

    <section className="linkgrabber-page__workbench">
      <textarea
        className={dragging ? "is-dragging" : undefined}
        value={input}
        onChange={(event) => setInput(event.target.value)}
        onDragOver={(event) => { event.preventDefault(); setDragging(true); }}
        onDragLeave={() => setDragging(false)}
        onDrop={(event) => void handleDrop(event)}
        placeholder="Paste URLs or HTML here, or drop a .txt / .html file…"
        aria-label="Links to inspect"
      />
      <div className="linkgrabber-page__generator">
        <Wand2 size={14} />
        <input
          value={pattern}
          onChange={(event) => setPattern(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter" && pattern.trim()) void generate(); }}
          placeholder="Numbered series, e.g. https://site.com/part[01-20].rar"
          aria-label="Link pattern"
        />
        <button type="button" onClick={() => void generate()} disabled={!pattern.trim()}>Generate</button>
      </div>
      <div className="linkgrabber-page__toolbar">
        <span>{candidates.length} candidates · {selected.size} selected</span>
        <button type="button" onClick={() => navigator.clipboard?.writeText([...selected].join("\n"))} disabled={!selected.size}><Copy size={14} /> Copy selected</button>
      </div>
    </section>

    <section className="linkgrabber-page__results">
      <div className="linkgrabber-page__results-head">
        <div><strong>Candidate links</strong><span>Review before sending to the queue</span></div>
        <div className="linkgrabber-page__filters"><label><Filter size={14} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Filter hosts" /></label><select value={kind} onChange={(event) => setKind(event.target.value)}><option value="all">All types</option>{types.map((type) => <option key={type} value={type}>{type}</option>)}</select></div>
      </div>
      <div className="linkgrabber-page__select-row">
        <button type="button" onClick={() => setSelected(new Set(visible.map((item) => item.url)))}><Check size={14} /> Select visible</button>
        <button type="button" onClick={() => setSelected(new Set())}>Clear</button>
        <button type="button" onClick={() => void checkSelected()} disabled={!selected.size || probing}><Radar size={14} /> {probing ? "Checking…" : "Check selected"}</button>
        {checkedSummary.known || checkedSummary.unreachable ? (
          <span className="linkgrabber-page__summary">
            {checkedSummary.known ? `${formatBytes(checkedSummary.bytes)} in ${checkedSummary.known} checked` : null}
            {checkedSummary.unreachable ? <em>{checkedSummary.unreachable} unreachable</em> : null}
          </span>
        ) : null}
      </div>
      {error ? <div className="linkgrabber-page__error">{error}</div> : null}
      <div className="linkgrabber-page__list">{visible.map((item) => {
        const probe = probes.get(item.url);
        return <label key={item.url} className={`linkgrabber-page__item${probe && !probe.reachable ? " is-unreachable" : ""}`}>
          <input type="checkbox" checked={selected.has(item.url)} onChange={() => toggle(item.url)} />
          <span className="linkgrabber-page__item-main">
            <strong>{probe?.filename ?? item.host}</strong>
            <span>{item.url}</span>
          </span>
          <span className="linkgrabber-page__item-facts">
            {probe ? (probe.reachable ? <>
              {probe.totalBytes !== null ? <b>{formatBytes(probe.totalBytes)}</b> : null}
              <i className={probe.rangeSupported ? "is-good" : undefined}>{probe.rangeSupported ? "Resumable" : "No resume"}</i>
            </> : <i className="is-bad" title={probe.error ?? undefined}>Unreachable</i>) : null}
            <code>{item.extension ?? "other"}</code>
          </span>
        </label>;
      })}{!visible.length ? <div className="linkgrabber-page__empty">Paste links above to begin.</div> : null}</div>
      <div className="linkgrabber-page__actions">
        {startAction.note ? <span className="linkgrabber-page__note">{startAction.note}</span> : null}
        <select value={queueId} onChange={(event) => setQueueId(event.target.value)} aria-label="Target queue"><option value="">No queue</option>{queues.map((queue) => <option key={queue.id} value={queue.id}>{queue.name}</option>)}</select>
        <button type="button" className="secondary" disabled={!engineReady || submitting || !selected.size} onClick={() => submit({ kind: "download-later" })}>Download later</button>
        <button type="button" disabled={!engineReady || submitting || !selected.size} onClick={() => submit(startAction.action)}>{startAction.label}</button>
      </div>
    </section>
  </div>;
}

