# Phase 2 Review — Findings and Prioritized Gaps

**Reviewed at:** commit `6654c02` (`feat: add queue management workspace`), branch `master`, working tree clean except the untracked master specification.
**Review date:** 2026-09-22
**Scope:** verification of the Phase 2 completion claim, plus a gap analysis against `DOWNLOAD_MANAGER_FEATURE_LOCK_AND_CODEX_MASTER.md`.
**Status:** report only. No source file was modified by this review.

## 1. Verification of the Phase 2 Claim

The claims in `IMPLEMENTATION_PROGRESS.md` were checked against the source and by execution.

| Claim | Result |
| --- | --- |
| `cargo test --workspace` passes with 42 tests | Confirmed — dm-common 7, dm-core 14, dm-ipc 5, dm-storage 16 |
| Schema version 2 migration, no destructive reset | Confirmed — `MIGRATION_V2` in `crates/dm-storage/src/lib.rs` adds `queues`, seeds a stable Default Queue, and adds columns via `ALTER TABLE` |
| Atomic claim of queued tasks | Confirmed — `claim_queued_download` is a status-guarded conditional `UPDATE` (`crates/dm-storage/src/downloads.rs:261`) |
| Per-queue and per-host concurrency | Confirmed — `fill_available_slots` (`crates/dm-core/src/queue.rs:209`) |
| Priority-then-position ordering | Confirmed — `list_queued_downloads` orders by priority rank, then `queue_position` (`crates/dm-storage/src/downloads.rs:202`) |
| Queue identity/ordering/claims authoritative in Rust | Confirmed — React holds no queue state of its own; `useQueues` is an IPC projection |
| Queues UI with reorder, priority, start/stop | Confirmed — `src/components/queues/QueuePage.tsx` |

**Conclusion: Phase 2 is genuinely complete as specified.** The findings below are defects and gaps found *inside* that completed work, not a dispute of it.

## 2. Defects — should be fixed before Phase 3 work builds on them

### D1. Crash-orphaned tasks are permanently stuck (severity: high)

Nothing resets non-terminal rows at startup. `src-tauri/src/lib.rs` `run()` opens storage and resumes running queues, but never reconciles rows left in `probing`, `downloading`, or `finalizing` by a previous process.

Such a row is then a complete dead end:

- not startable — `claim_task` only accepts `created`;
- not claimable by a queue runner — `list_queued_downloads` selects `status = 'queued'` only;
- not removable — `remove_download` rejects any status outside `completed | failed | cancelled` (`src-tauri/src/lib.rs:316`);
- not retriable or cancellable — those actions do not exist yet.

The master specification treats this as non-negotiable (§11: "interrupted active states must become recoverable states, not phantom Downloading rows"). Full resume belongs to Phase 3, but the startup reconciliation that prevents permanently stuck rows does not depend on resume and should not wait for it.

### D2. A queue runner resumed at startup emits no events (severity: high)

`src-tauri/src/lib.rs:505` resumes persisted running queues with the callback `|_| {}`. The event callback exists only because the `start_queue` command receives a `Channel` from the frontend invoke; a runner started during `setup` has no such channel.

Consequences:

- after a restart, a resumed queue downloads silently — no progress, no status change, no completion in the UI;
- the state is worse than stale, because pressing Start in the UI does not repair it. `start_queue` flips the persisted state to running and returns `Ok`, then the spawned `run_queue` fails with `QueueServiceError::AlreadyRunning` (the runner is already registered in `active_runners`) and only logs a warning. The frontend believes the queue started and keeps waiting on a channel that will never receive anything.

The structural fix is to stop carrying runner output on a per-invoke `Channel` and emit application-level events instead, so any runner — startup-resumed or user-started — reaches whatever UI is listening. `start_queue` should also report the already-running case instead of silently succeeding.

### D3. Free concurrency slots are only refilled when a task finishes (severity: medium)

The runner loop (`crates/dm-core/src/queue.rs:163`) calls `fill_available_slots`, then blocks on `tasks.join_next().await` (`:196`). While at least one transfer is in flight, newly enqueued work is not noticed.

With `max_concurrent = 3` and tasks added one at a time to an already-running queue, the queue effectively runs them one at a time. The configured concurrency only materializes when several tasks are enqueued before the runner fills its slots. The loop needs a second wake source (an enqueue notification, or a bounded timer) alongside task completion.

### D4. `created` and `queued` tasks cannot be removed (severity: medium)

Same guard as D1 (`src-tauri/src/lib.rs:316`), but reachable without any crash: choose "Download Later", change your mind, and the row cannot be deleted. `DownloadContextMenu.tsx` correctly disables the item to match the backend, so the dead end is visible but unexplained.

Removing a task that has never transferred bytes is safe and has no Phase 3 dependency. Cancel for in-flight tasks is legitimately Phase 3.

## 3. Smaller gaps inside Phase 2 scope

| # | Gap | Location |
| --- | --- | --- |
| S1 | `queues.enabled` is persisted and exposed over IPC but never read by the runner — a disabled queue still runs | `crates/dm-core/src/queue.rs` |
| S2 | A drained running queue persists itself as `stopped`, so a task enqueued afterwards waits for a manual Start. Plausible as a deliberate choice, but it is undocumented and makes the persisted `running` state almost unobservable | `crates/dm-core/src/queue.rs:178` |
| S3 | The download row context menu has no queue actions. §6 lists Move to Queue, Change Queue, Remove from Queue, and Change Priority as per-task actions; they exist only inside the Queues page, so a `created` or `failed` task in the main list cannot be sent to a queue | `src/components/downloads/DownloadContextMenu.tsx` |
| S4 | Queue `pause`/`resume` from §7 are not implemented; only start/stop exist | `crates/dm-core/src/queue.rs` |
| S5 | No frontend test runner is installed, so the frontend half of §35 (helpers, IPC names and argument casing, critical UI behavior) is unverified. IPC casing mismatches are exactly the class of bug that silently breaks a Tauri command | `package.json` |

## 4. Gaps that are correctly deferred, with a note on sequencing

These are assigned to later phases by the specification and are **not** defects today. They are listed because two of them affect how early the product becomes usable.

- **Phase 3** — pause/resume, partial-byte persistence, validators (ETag/Last-Modified), retry state, crash recovery. D1 above is the part that should not wait.
- **Phase 4** — segmented engine. The current engine is single-stream with no range probing (`crates/dm-core/src/lib.rs` issues a plain GET).
- **Phase 5** — adaptive connections. This is the product's actual differentiator and currently has no groundwork.
- **Phase 6** — bulk selection, sort, list virtualization, keyboard shortcuts, notifications.
- **Phase 7+** — categories, rules, scheduler, LinkGrabber, browser integration, media. No schema or service exists for any of these yet, as expected.

Two deferred items are worth pulling earlier than their phase number suggests:

1. **Per-row speed and ETA, and aggregate speed in the top bar.** `DownloadRow.tsx` currently shows percent and byte counts only; `TopBar.tsx` shows title, search, and Add. Beyond being the main thing that makes a download manager feel fast, speed and ETA are the cheapest way to *see* whether a resumed transfer is behaving correctly — which makes them a practical prerequisite for testing Phase 3 by hand, not just a Phase 6 polish item.
2. **A configurable default destination.** The settings page (`src/components/settings/SettingsPage.tsx`) contains only the Add Download input mode; every command derives its destination from `app.path().download_dir()`. §27 requires a default destination, and its absence is felt on first use.

## 5. UI direction assessment

Against §22 ("dark charcoal/navy modern Windows utility, compact, restrained blue accent") the current implementation is compliant and clean: `src/styles/tokens.css` defines a coherent dark palette with a single blue accent, the sidebar and rows are compact, and there is no excessive glass or oversized card styling.

It is, however, compliant rather than distinctive. The palette is a conventional dark theme with a blue accent, and the row is a name, a bar, and a percentage. The differentiators listed in §37 — explainable adaptive connections, host-aware behavior, failure recovery that explains itself — are all *informational*, which means the visual identity of this product should come from how densely and legibly it presents live engine state, not from decoration. That state does not exist yet (no speed, no ETA, no connection count, no adaptive explanation), so the distinctive look is currently blocked on the engine rather than on design work.

The practical implication: a visual redesign done now would be redesigning a row that is about to gain four or five new live fields. The higher-value sequencing is to let Phases 3–5 produce real engine telemetry, surface each field as it becomes real, and treat the visual identity pass as the step that makes that dense information legible.

## 6. Suggested priority order

1. D1, D2 — correctness and observability holes that Phase 3 will otherwise inherit and obscure.
2. D4, D3 — reachable in normal use without a crash.
3. S5 — a frontend test runner, before the IPC surface grows further in Phase 3.
4. Phase 3 proper, with per-row speed/ETA surfaced alongside it.
5. S1–S4 and the default destination setting, foldable into Phase 6.

## 7. Method

Static review of `crates/`, `src/`, and `src-tauri/src/`; `cargo test --workspace` executed (42 passed, exit 0). The application was not launched and no end-to-end run was performed, so the defects above are derived from source and are stated as such: D1–D4 each follow from an explicit guard or callback in the code cited, not from an observed failure.
