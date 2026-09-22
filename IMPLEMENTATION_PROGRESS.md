# Implementation Progress

Authoritative specification: `DOWNLOAD_MANAGER_FEATURE_LOCK_AND_CODEX_MASTER.md`

## Repository Baseline

- Starting checkpoint: `1728b6b` (`feat: add smart download input settings and history removal`)
- Branch at audit: `master`
- Existing architecture: Rust workspace (`dm-common`, `dm-core`, `dm-storage`, `dm-ipc`, `src-tauri`) with a React/TypeScript/Vite presentation layer.
- SQLite schema version: 1.
- The master specification was supplied as an untracked repository file and is preserved without modification.

## Phase 0 - Repository Audit

Status: Complete

### Verified baseline

- Rust is authoritative for persisted download records and lifecycle state.
- `dm-common` owns the canonical download status enum.
- `dm-storage` owns SQLite schema version 1, settings, and download persistence.
- `dm-core` owns the single-stream HTTP/HTTPS downloader and persistence orchestration.
- `dm-ipc` owns serialized response/event contracts.
- Tauri commands are the frontend boundary; React renders persisted rows and the current transfer progress.
- Existing functionality matches the specification's Current Baseline: persistent history, single-stream downloads, list/search/status filters, details/context actions, safe history removal, Smart Add clipboard/manual input, HTTP/HTTPS extraction, deduplication, and automatic single/batch detection.

### Phase 1 gaps found

- `start_download` accepts a URL, creates a row, and performs the transfer in one blocking command.
- A created task cannot be started later by its existing ID.
- The Add Download modal remains open for the duration of the transfer.
- Rows are refreshed only after a transfer or batch completes.
- State changes are persisted but are not guarded by canonical typed transition validation.
- Logging currently includes the full source URL; Phase 1 will remove that exposure while touching the start path.
- The existing schema already supports Phase 1. No migration is required; adding a no-op schema version would violate the migrations-only intent without changing persisted structure.

### Quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (18 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed after setting `NPM_CONFIG_PREFIX=C:\Program Files\nodejs` to bypass a broken user-level npm prefix

## Phase 1 - Persistent Task Architecture

Status: Complete

### Plan

1. Add canonical transition validation and tests in `dm-common`.
2. Add atomic, status-guarded persistence transitions and lifecycle tests in `dm-storage`.
3. Split task creation from transfer execution in `dm-core`; validate URLs locally, start only an existing task ID, preserve the ID through completion/failure, and test that no duplicate record is created.
4. Add create/start IPC contracts and thin Tauri commands; claim the task before spawning background work and stream task events without logging source URLs.
5. Update React so Start Now and Download Later create rows immediately, close the modal immediately, and let background events update the list.
6. Run the complete quality gate, update this document with results, and create the focused Phase 1 commit.

### Implemented

- Added canonical Phase 1 transition rules for `created -> probing -> downloading -> finalizing -> completed`, with typed rejection of invalid transitions.
- Added atomic SQLite transition guards so concurrent/double start attempts cannot claim the same task twice.
- Split task creation from execution in `dm-core`; creation performs local URL validation only and does not contact the source.
- Starting now accepts an existing task ID, preserves it through completion/failure, and never inserts a second record.
- Added a bounded Rust background executor (three simultaneous transfers) and coalesced progress persistence instead of writing SQLite on every chunk.
- Added background IPC events with UI delivery capped at 10 progress updates per second per task.
- Replaced full-URL transfer logging and raw persisted HTTP errors with task-ID logging and redacted failure messages.
- Added Start Now / Download Later split actions. Both create rows immediately and close the modal after persistence; only Start Now launches background work.
- Added real Start Download actions for created tasks in the context menu and details panel.
- Kept schema version 1 because all Phase 1 fields and states already exist; no data-destructive or empty migration was introduced.

### Tests added/updated

- Canonical legal and illegal state transitions.
- Atomic duplicate-start rejection and required probing order.
- Network-free task creation and non-HTTP rejection without persistence.
- Stable task identity and single-record completion lifecycle.
- Stable task identity on HTTP failure with redacted persisted error text.
- Background IPC progress and authoritative-record event contracts.

### Quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (29 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed with the machine-specific npm prefix override documented in Phase 0

### Scope boundary

Phase 1 will not implement queues, pause/resume, segmented transfer, adaptive connections, scheduler, browser integration, or media extraction. Those remain assigned to later phases in the master specification.

## Phase 2 - Queue Foundation

Status: Complete

### Plan

1. Add canonical queue/priority models and a schema version 2 migration with a stable Default Queue, queue metadata, and download queue assignment fields.
2. Add tested storage operations for named queues, enqueue/remove/move/reorder, priority, and persistent running/stopped state.
3. Add a Rust queue service and runner that is the only path for queued tasks to start, honors per-queue and per-host concurrency, and stops scheduling when the queue is stopped.
4. Add IPC contracts and thin Tauri commands for queue CRUD, assignment, ordering, and runner control.
5. Add Queues navigation/UI plus Add Download queue actions, persistent priority/queue hints, and task move/reorder controls.
6. Run the complete quality gate, update this document, review the full change, and create focused commits for the Phase 2 slices.

### Scope boundary

Phase 2 will not add schedules, speed profiles, task pause/resume, retry policy, segmented transfer, or adaptive connection logic. Queue stop prevents new starts; active transfers continue until Phase 3 introduces task control handles.

### Implemented so far

- Added schema version 2 with a stable Default Queue, persistent named queues, concurrency settings, queue assignment, queue position, and priority without losing schema version 1 history.
- Added transactional storage operations for enqueue, move, remove, priority changes, exact-set reorder, and atomic queued-task claims.
- Added a Rust queue service and runner. Queued records remain queued until claimed by their queue runner, preserve their task IDs, and persist terminal states through the existing download service.
- Enforced per-queue concurrency, optional hostname concurrency, and the existing process-wide three-transfer limit shared with direct starts.
- Queue stop now persists immediately, prevents any further claims, and lets already-active transfers finish as documented by the Phase 2 scope boundary.

### Focused verification so far

- Schema v1-to-v2 migration and reopen tests pass.
- Queue storage operation and invalid transition tests pass.
- Local HTTP integration tests cover per-queue, per-host, and global concurrency, stop behavior, stable IDs, and completed persistence.
- Strict `dm-core` Clippy passes after the runner implementation.

### IPC and UI

- Added typed queue/list/event IPC contracts and thin Tauri commands for queue creation, assignment, moving, removal, priority changes, exact ordering, start, and stop.
- Running queues resume their remaining queued work when the app starts; recovery of tasks that were mid-transfer remains assigned to Phase 3.
- Added a Queues sidebar destination with named-queue creation, runner controls, concurrency summaries, drag/drop ordering, move up/down/top/bottom controls, priority editing, queue changes, and remove-from-queue actions.
- Added Default Queue and named-queue batch actions to Add Download, including Create Queue & Add.
- Added persistent queue and priority hints to download rows.
- Kept React as a projection/control surface: all queue identity, ordering, state, claims, and concurrency enforcement remain authoritative in Rust and SQLite.

### Phase 2 quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (42 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed with the machine-specific npm prefix override documented in Phase 0
- Tauri development runtime - compiled and launched successfully against the real schema version 2 backend
- Code review - approved after extracting queue IPC/event orchestration from `App.tsx` into `useQueues`; no unresolved required findings

### Phase boundary

Phase 2 is complete. Phase 3 has not started. Task pause/resume, in-flight crash recovery, validators, and retry state remain assigned to Phase 3 by the master specification.

## Phase 2.5 - Defect Closure

Status: Complete

Not a phase from the master specification. It closes defects found by the
Phase 2 review in `docs/phase2-review.md` before Phase 3 builds on them.

### Plan

1. Recover tasks a restart orphaned, and make the canonical rules for that
   recovery live in `dm-common`.
2. Publish engine events at application level so every runner reports to the
   UI, including one resumed during startup.
3. Wake a running queue when work is added so configured concurrency is real.
4. Allow removing tasks no executor owns, guarded against a concurrent claim.
5. Measure transfer rate and remaining time in the engine and surface them.
6. Add a frontend test runner, including an IPC contract test.

### Implemented

- Added canonical `is_orphaned_by_restart`, `restart_recovery_status`,
  `is_removable` and `QueueRecord::is_schedulable` so status groups are derived
  from `dm-common` rather than restated in SQL or in React.
- Startup now returns `probing`/`downloading`/`finalizing` rows to `created`
  or, for queued work, to `queued`, clears their progress bytes, and records
  an `interrupted` explanation on the row. Previously such a row could not be
  started, claimed, retried or removed, so it stayed in the list forever.
- Replaced per-invoke channels with the application events
  `download-task-event` and `queue-runner-event`. A queue resumed at startup
  now reports progress; previously it transferred silently and pressing Start
  left the UI waiting on a channel nothing would ever write to.
- `start_queue` detects an already-active runner and reuses it instead of
  spawning a second one that fails inside a detached task.
- The queue runner now waits on both task completion and a wakeup, so a task
  added, moved or reprioritised while transfers are running starts
  immediately instead of waiting for a slot to free by completion.
- A disabled queue neither starts nor schedules, and `enabled` is now a real
  setting with storage, a command and a UI toggle.
- Created and queued tasks can be removed. The delete is status-guarded in SQL,
  so a runner claiming the task at the same moment wins the race rather than
  losing its row mid-transfer.
- Added `ThroughputMeter` in `dm-core`: a smoothed rate built from the bytes
  the engine actually wrote, plus an ETA that reports nothing rather than
  guessing when a transfer stalls or its size is unknown. Rate and ETA travel
  on progress events and appear in rows, the details panel, and as aggregate
  throughput in the top bar.
- Added queue, priority and remove-from-queue actions to the download row
  context menu, so a task no longer has to be managed from the Queues page.

### Tests added/updated

- Canonical recovery, removability and schedulability rules.
- Storage recovery for queued and non-queued tasks, and the no-op case.
- Guarded removal, including refusal while a transfer is active.
- Throughput measurement: first window, smoothing, stalls, unknown totals.
- A running queue filling a free slot with newly added work. Verified to fail
  when the wakeup is removed.
- A disabled queue refusing to start or schedule.
- Frontend: byte/rate/duration/host formatting, link extraction, and an IPC
  contract test that checks command names, argument casing and event names
  against `src-tauri/src/lib.rs`. Verified to fail on a renamed argument.

### Quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (60 tests, up from 42)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `cargo build -p tauri-app` - passed
- `npm run build` - passed
- `npm test` - passed (21 tests)

### Known limitations carried into Phase 3

- Recovery restarts an interrupted transfer from zero. Partial bytes and their
  validators are not kept yet, which is exactly what Phase 3 adds.
- `temp_path` is still never persisted, so a `.part` file left by a killed
  process is not cleaned up. Phase 3 owns temp-file identity.
- Queue pause/resume is still absent; stop is the only control, because pause
  without task-level pause would be indistinguishable from stop.
- A drained running queue still stops itself. Documented as deliberate: work
  added afterwards waits for an explicit Start.

## Phase 3 - Pause / Resume / Recovery

Status: Complete

### Plan

1. Extend the canonical state machine with pause, resume, cancel, retry and
   restart, and make storage derive every update guard from it.
2. Schema version 3: persistent attempt count and retry time.
3. Replace the single-shot downloader with probe + resumable transfer, driven
   by per-task control handles.
4. Validate remote identity before reusing partial bytes.
5. Classify failures and retry only the retryable ones, with bounded
   exponential backoff and jitter.
6. Tauri commands, a retry scheduler, and UI controls.

### Implemented

- Canonical transitions now include pause, resume, cancel, retry and restart.
  `DownloadStatus::sources_of` feeds every storage update guard, so
  `can_transition_to` is the only place the rules exist; before this it was
  only exercised by tests while each SQL statement carried its own status list.
- Direct start, resume and retry share one claim path that refuses `queued`
  tasks, so queued work still starts only through its queue runner.
- Schema version 3 adds `attempts` and `retry_at` without touching existing
  rows. Migration tests are version-agnostic.
- Probing: HEAD is read when it answers, but range support is only accepted
  after a `bytes=0-0` request returns `206` with a `Content-Range`. The total
  size comes from `Content-Range` when ranged, never from the one-byte body.
- The transfer plan (resolved URL, filename, destination, temp path, size,
  ETag, Last-Modified, range support) is persisted before any byte is written.
- Destination and partial file are reserved together by creating the partial
  file exclusively. This fixed a real race found by the concurrency tests:
  two transfers with the same filename could previously plan the same path.
- Resume rules (`resume::plan_resume`): trust the file over the record; refuse
  to append when ETag or Last-Modified changed, or when the size changed;
  accept an unchanged size when the server offers no validator; restart when
  the server cannot serve ranges; finalize directly when every byte is
  already on disk. A server that answers a ranged request with `200` is
  detected at transfer time and the file is rewritten from zero.
- Restarts that throw away bytes record a `restarted` notice explaining why.
- Pause and cancel reach a running transfer through `TaskControl` and take
  effect between chunks. Pause keeps the partial file; cancel deletes it.
- Startup recovery now keeps partial transfers: a queued task returns to its
  queue and continues from its partial file, a non-queued one becomes paused.
- A body that ends early never becomes a finished file.
- Retry classification: 408/425/429/503/5xx, timeouts, connection errors and
  truncated bodies retry; 401/403/404 and other 4xx, invalid URLs, permission
  and disk-full errors, and user stops do not. Default budget is five attempts
  with 2s exponential backoff, capped at five minutes, plus up to 25% jitter.
- A retry scheduler in the app polls every five seconds; queued retries go
  back to their queue so its concurrency still applies.
- Removing a paused or cancelled task also deletes its partial file.
- UI: one primary control per row (Pause, Resume, Start or Retry), full
  action lists in the context menu and details panel derived from the same
  rules, a retry countdown, and attempt count in details.

### Tests added/updated

- Canonical rules: pause/resume, retry from terminal states, completed is
  final, `sources_of` matches `can_transition_to`.
- Storage: partial-transfer recovery, pause/resume byte preservation,
  cancel clears the partial, completed cannot reopen, failed retries keep
  identity, due retries respect backoff.
- Engine, against a real local HTTP server (`dm-core/src/testing.rs`):
  verified range probing, no-range probing, whole transfer, resume from disk,
  range ignored by the server, interrupted body, pause mid-transfer, pause then
  finish, already-complete finalize, destination collision, strict ranged
  segments, ignored-range rejection, and ordered segment assembly.
- Resume planner: validators, size change, missing partial, record ahead of
  file, already complete.
- Retry policy: classification of statuses, IO errors and stops; backoff
  growth, cap, jitter bounds, bounded budget.
- Service: pause then resume produces the exact original file, cancel deletes
  the partial, changed source is re-downloaded rather than appended to, crash
  recovery resumes from the partial file, 404 fails permanently, 503 schedules
  a retry, the retry budget ends in failure, segmented completion, segmented
  pause/resume, and no-range single-stream fallback.
- Queue tests moved onto the shared test server and now measure real overlap
  of body transfers.

### Quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (126 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `cargo build -p tauri-app` - passed
- `npm run build` - passed
- `npm test` - passed (21 tests)

### Known limitations carried forward

- Adaptive connection scaling, host learning, speed profiles, and
  explainability UI remain Phase 5; Phase 4 uses a conservative bounded worker
  count.
- Queue pause/resume at the queue level is still absent; tasks can now be
  paused individually.
- Retry after connectivity returns is time-based only; there is no network
  change detection.
- The app was built but not launched against a live server in this phase;
  runtime behaviour is covered by the engine's local HTTP integration tests.

### Phase boundary

Phase 3 is complete. Phase 4 is complete and committed; Phase 5 is the next
active milestone.

## Phase 4 - Segmented Engine

Status: Complete

Phase 3 was verified from the current repository state before this phase began:
schema version 3, task controls, partial-file resume, validator checks,
orphan recovery, bounded retry policy, and the documented quality gate were
present in the current history and source.

### Plan

1. Add canonical segment/range models and an additive schema v4 persistent
   segment map.
2. Add strict ranged HTTP transfer primitives that require `206` and a
   matching `Content-Range` for every segment.
3. Add a deterministic gap-free planner and bounded worker pool with dynamic
   pending-segment assignment.
4. Persist segment offsets/status, reuse only source-compatible segment files,
   assemble in order into the existing safe temporary-file finalization path,
   and clean segment files after success.
5. Fall back to the existing single-stream engine when the source cannot
   provide validated ranges; keep pause/cancel and retry behavior intact.
6. Run focused local HTTP/storage tests, the full quality gate, review the
   complete diff, and create focused Phase 4 commits.

### Implemented slices

- Schema v4 adds an additive `download_segments` map with canonical
  `Pending`/`Downloading`/`Completed` statuses and atomic storage operations.
- Ranged transfers require an exact `206` plus matching `Content-Range`, write
  only within an inclusive segment, resume from the segment offset, and reject
  ignored/malformed ranges without appending bytes.
- A deterministic gap-free planner and bounded dynamic worker pool persist
  real segment progress, reuse source-compatible files after interruption,
  assemble in index order through the existing synced finalization path, and
  clean up segment files on success/cancel.
- Range capability remains verified at probe time; no-range sources continue
  through the existing single-stream engine. Segment maps are discarded when
  validators or size change.

### Focused verification so far

- `dm-core`: planner, strict ranged transfer, no-range fallback, segmented
  completion, pause/resume from segment files, and ordered assembly tests pass.
- The focused Phase 4 tests pass; the full gate is recorded below.

### Phase 4 gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (126 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed (with `npm_config_prefix` pointed at the installed
  Node.js npm prefix because the default user npm shim targets a missing path)

### Phase boundary

Phase 4 is complete. Phase 5 (adaptive connections and explainable speed)
remains intentionally out of scope.

### Scope boundary

Phase 4 will not add adaptive connection scaling, host learning, speed
profiles, or explainability UI; those remain Phase 5. The segmented engine
will use a conservative bounded worker count and real byte progress only.

## Phase 5 - Adaptive Connections + Speed

Status: Complete

### Plan

1. Add a deterministic adaptive controller with throughput samples, bounded
   connection targets, diminishing-return detection, and explainable reasons.
2. Add additive host-profile persistence and source-safe host keying for
   reusable connection limits and observed 429/503 behavior.
3. Connect the controller to the segmented worker pool: start conservatively,
   scale up when measured throughput improves, scale down on diminishing gains
   or server instability, and honor bounded global/task/queue limits.
4. Add typed 429/503 handling with backoff and expose active/max connections,
   measured throughput history, and the adaptive explanation through IPC/UI.
5. Add focused policy/storage/HTTP/service/UI tests, run the full quality gate,
    review the diff, and create focused commits.

### Implemented

- Added a deterministic adaptive controller that starts at one connection,
  probes bounded additional streams, detects diminishing returns, and applies
  capped exponential backoff after 429/503 responses.
- Added schema v5 host profiles keyed only by normalized hostname. Profiles
  persist preferred connection caps, rate-limited/busy observations, and the
  last numeric status without URL paths, queries, fragments, credentials, or
  headers.
- Integrated adaptive targets with the segmented worker pool while retaining
  the existing process-wide, queue, per-host, and per-task bounds. Throughput
  and connection counts are measured by Rust and remain authoritative.
- Added typed HTTP status errors and retry classification for rate limiting and
  server-busy responses. Host observations are persisted for both probe and
  segment failures.
- Extended IPC, Tauri mapping, and the existing React projection to render
  measured active/max connections and the backend-provided adaptive reason.

### Tests added/updated

- Adaptive policy tests cover conservative start, gain-based probing, target
  caps, diminishing returns, and bounded 429/503 backoff.
- Storage tests cover v5 migration, host-only key validation, redaction, and
  reopen/accumulation of observations.
- Core tests cover typed rate-limit errors, retry classification, adaptive
  segmented progress, byte-identical completion, and probe profile updates.
- IPC and frontend tests/build cover the new measured progress fields and
  explainability rendering.

### Phase 5 quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (138 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed (with `npm_config_prefix` pointed at the installed
  Node.js npm prefix because the default user npm shim targets a missing path)
- `npm test` - passed (21 tests)
- Code review - approved across correctness, readability, architecture,
  security, and performance; no unresolved required findings.

### Phase boundary

Phase 5 is complete and committed. The next master-spec milestone is not
started; no categories, rules, schedulers, browser integration, media
extraction, or post-processing work was added.

## Phase 6 - Main UX Completion

Status: Complete

### Plan

1. Close the remaining task-management gaps with a backend-authoritative
   restart-from-zero operation and typed IPC contract.
2. Add keyboard-friendly multi-selection and a contextual bulk action bar for
   start, pause, resume, retry, cancel, queue, priority, and removal actions.
3. Complete details/context actions for restart, refresh-source, properties,
   copy/open/reveal, and measured speed/ETA without duplicating state in React.
4. Add focused lifecycle, IPC, frontend interaction, and accessibility tests;
   run the full quality gate and commit the phase.

### Implemented

- Added backend-authoritative restart-from-zero for reusable tasks, preserving
  stable IDs while clearing partial transfer state and segment maps.
- Added validated source URL refresh for failed/paused/queued tasks without
  exposing URLs in logs or bypassing lifecycle guards.
- Added IPC/UI metadata for range support, ETag, and Last-Modified alongside
  existing measured speed, ETA, connection counts, and adaptive explanations.
- Added Ctrl/Shift multi-selection, Ctrl+A/Escape selection shortcuts, and a
  contextual bulk action bar for start, pause, resume, retry, cancel, remove,
  queue assignment, and priority changes. Every mutation delegates to Rust.
- Added Ctrl+N/Ctrl+F shortcuts and accessible in-app completion/failure
  notifications driven by authoritative task events.

### Tests added/updated

- Canonical lifecycle, storage, and service tests cover restart reset,
  stable identity, source refresh validation, and metadata clearing.
- IPC/frontend tests cover expanded task metadata, restart action availability,
  and primary row action behavior. The frontend build validates bulk UI
  contracts and keyboard-safe rendering.

### Phase 6 quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (143 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed (with `npm_config_prefix` pointed at the installed
  Node.js npm prefix because the default user npm shim targets a missing path)
- `npm test` - passed (23 tests)
- Code review - approved across correctness, readability, architecture,
  security, and performance; no unresolved required findings.

### Phase boundary

Phase 6 is complete and committed. Categories, rules, scheduler, LinkGrabber,
browser integration, media extraction, post-processing, and release hardening
remain for later master-spec phases.

### Scope boundary

Phase 6 does not introduce categories, rules, schedules, LinkGrabber, browser
integration, media extraction, post-processing, or system power actions.

### Scope boundary

Phase 5 will not add categories, rules, schedulers, browser integration, media
extraction, or post-processing. Speed controls remain conservative and local;
no cookies, credentials, or authorization data are persisted in host profiles.
