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

## Phase 7 - Categories / Rules / Scheduler

Status: Complete

### Plan

1. Add additive schema v6 for built-in/custom categories and ordered rules.
2. Add typed common models and storage CRUD with validated JSON list fields.
3. Add rule evaluation and apply category/queue/priority actions during task
   intake without moving business logic into React.
4. Add queue schedules and a persistent scheduler runner with manual override;
   test restart persistence and time-window boundaries.
5. Add Settings/Categories/Rules/Scheduler UI, run the full gate, review, and
   commit the phase.

### Implemented so far

- Schema v6 creates built-in Applications, Archives, Documents, Video, Audio,
  Images, and Other categories plus an ordered `download_rules` table.
- `dm-common` now owns serializable category and rule models; `dm-storage`
  persists custom category lists and rule match/action fields with validation.
- Migration, built-in category, custom category, and custom rule persistence
  tests pass.
- Added backend rule evaluation with ordered explicit matches, MIME/extension/
  host/Other category precedence, human-readable match explanations, and
  intake application for queue and priority actions.
- Added schema v7 queue schedules, typed schedule windows, persistence and
  validation, and a restart-safe scheduler loop that only stops runners it
  started itself. Manual queue starts remain independent.
- Added typed schedule/category/rule IPC responses and Settings UI controls;
  queue schedule toggles remain backend-owned and default to no completion
  action. Download details now display the backend rule explanation.

### Phase 7 quality gate

- `cargo fmt --all` - passed
- `cargo test --workspace` - passed (150 tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed (with `npm_config_prefix` pointed at the installed
  Node.js npm prefix because the default user npm shim targets a missing path)

### Phase boundary

Phase 7 is complete and committed. LinkGrabber, browser integration, media
extraction, post-processing, and release hardening remain for later phases.

## Phase 8 - LinkGrabber / Batch

Status: Complete (2026-09-23)

### Completed in this pass

- **Link checking before download.** "Check selected" probes each link the
  way a download would start and stops there: reachable, file name, size and
  whether it can be resumed, with a running total. At most four links are
  checked at once, 500 per request, and each gets 15 seconds, so a dead link
  is reported instead of holding up the batch. Errors never contain the URL.
  The concurrency test first proves the server can observe overlap (more
  than two at once without a limit), then that the limit holds.
- **Numbered-series generator.** `https://site/part[01-20].rar` expands in
  order, keeps zero padding, supports letter ranges and several ranges at
  once, leaves non-range brackets such as IPv6 hosts alone, refuses backwards
  ranges and anything over 1 000 links (checked on the product of all ranges,
  with overflow-safe arithmetic).
- **Order is kept.** Extracted links now appear in the order they occur on
  the page; they used to be sorted as text, which put part10 before part2.
- **Dropped files.** Text and HTML files dropped onto the input are read;
  other files and anything over 2 MB are refused with a message.
- **No uncontrolled batches.** The specification forbids launching hundreds
  of links in parallel. With no queue chosen, a selection of more than 20
  links starts through the Default Queue, and the page says so.

### Earlier in this phase

### Implemented so far

- Added a backend-owned link inspector that extracts HTTP(S) URLs from plain
  text and pasted HTML, normalizes and de-duplicates them, and returns safe
  host/extension metadata through typed IPC.
- Added focused parser tests for HTML/plain-text input and non-HTTP rejection.
- Existing Add Download batch flow remains the user-facing intake surface;
  task creation still validates each URL in Rust before persistence.

## Phase 9 - Browser Integration

Status: In progress

### Implemented so far

- Added a validated browser handoff model and Tauri command for native-message
  style intake. HTTP(S) only, URL credentials rejected, and context metadata is
  bounded and never persisted or logged.

## Phase 10 - Resource / Media

Status: In progress

### Implemented so far

- Added backend media-source classification for direct resources, HLS, DASH,
  and explicitly unsupported protected entry points. No DRM bypass is
  attempted; classification is exposed through typed IPC.
- Added metadata-only HLS master-playlist variant parsing with absolute URL
  resolution; segment/key fetching remains in the normal guarded engine.

### Scope boundary

Browser integration, LinkGrabber, media extraction, post-processing, and
release hardening remain out of scope until their dedicated phases.

### Scope boundary

Phase 5 will not add categories, rules, schedulers, browser integration, media
extraction, or post-processing. Speed controls remain conservative and local;
no cookies, credentials, or authorization data are persisted in host profiles.

## Phase 11 - Post-processing / System / CLI

Status: In progress

### Implemented so far

- Added a reusable archive-member path guard that rejects absolute paths and
  traversal components before any future extractor writes to disk.

## Phase 12 - Hardening / Benchmark / Release

Status: In progress

### Implemented so far

- Added focused security tests for browser credentials, media protection,
  link intake, and archive traversal.
- Full Rust and frontend quality gates continue to pass after each slice.

## Release packaging update

- Product metadata is now `Download Manager` version `1.0.0`.
- `npm run tauri -- build` produced the x64 NSIS installer, MSI installer, and
  portable release executable under `target/release/bundle/`.
- Added the MV3 browser extension source, options/popup UI, context-menu and
  takeover flow, Native Messaging manifest, and `dm-native-host` release
  binary. Takeover is opt-in and exclusions remain local to the browser.

## Current verification

- `cargo test --workspace` - passed (160 Rust tests)
- `cargo check --workspace` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `npm run build` - passed
- `npm test` - passed (23 tests)
- Working tree contains only the user-provided untracked master specification;
  no generated or temporary project changes remain.

## Product polish slice - LinkGrabber and visual refresh

Status: Implemented in working tree

- Added a dedicated LinkGrabber workspace backed by the Rust `inspect_links`
  command. It supports pasted text/HTML, de-duplicated candidate review,
  host/type filtering, selection, clipboard export, and direct queue/start
  actions through the existing task creation path.
- Added a Categories workspace that reads the persisted Rust category model and
  exposes destination, extension, and priority metadata without duplicating
  task state in React.
- Refreshed the shell visual system: tighter radius hierarchy, warmer charcoal
  surfaces, a restrained teal accent, editorial sidebar grouping, grid texture,
  denser download rows, clearer focus states, and responsive LinkGrabber and
  Categories layouts.
- Verification for this slice: `npm.cmd run build` and `npm.cmd test -- --run`
  pass. The PowerShell `npm` shim on this host points at a missing global npm
  installation; `npm.cmd` uses the repository-local package manager correctly.

## Review follow-up - Step 1: Critical defects

Status: Implemented in working tree (not committed)

Source: full product review of 2026-09-23 (browser handoff, error visibility,
list performance, window setup).

### Fixed

- **Single instance.** `tauri-plugin-single-instance` is registered first. A
  second launch (every browser handoff used to start one) now forwards its
  arguments to the running process and exits, so there is never a second
  window or a second engine on the same database.
- **Browser handoff is accepted only after the task is persisted.** The
  native host now writes the task into the application's database itself
  (`dm_common::APP_IDENTIFIER` locates the Tauri data directory; override with
  `DOWNLOAD_MANAGER_DATA_DIR`) and answers `accepted: true` only after that
  write. The extension pauses the browser's download while it waits, cancels
  and erases it only on acceptance, and resumes it otherwise. The application
  receives `--handoff-task <id>` and starts or queues the task with the same
  intake rules as Add Download.
- **Selected links open in LinkGrabber** in one launch (`--grab-links`),
  bounded to 500 links / 24 000 characters, instead of starting one process
  and one download per link. Links that arrive while the window is loading
  wait in `take_pending_link_intake`.
- **Referer and User-Agent reach the server.** Schema v8 adds
  `download_request_context`; the engine replays a task's browser Referer and
  User-Agent on every probe, ranged and single-stream request. Values with
  control characters are dropped (no header injection), cookies are never
  stored. The default User-Agent is now browser-compatible instead of
  `DownloadManager/0.1`, which many CDNs reject.
- **SQLite WAL mode**, so the native host and the application can use the
  database at the same time.
- **Errors are always visible.** A toast system reports failures of task,
  bulk, queue and LinkGrabber actions; previously the message was only
  rendered inside the Add Download dialog and was lost when it was closed.
- **List performance.** Progress events replace only the row that moved;
  `DownloadRow` is memoised with stable handlers; the retry clock re-renders
  only retrying rows. A late progress event can no longer flip a paused,
  cancelled or completed row back to "downloading".
- **Window and security setup.** Default window 1280x820 with a 960x600
  minimum (was 800x600, where the header overlapped), a real CSP (was
  `null`), and a real page title (was the Vite template's).
- **Small UI defects.** Row action button no longer overlaps the progress
  bar; sidebar labels align consistently; the adaptive explanation in the
  details panel is styled; the bulk action bar appears only for two or more
  selected rows.
- **Archive path guard** now treats `\` separators and drive prefixes the same
  on every platform (its test failed on non-Windows hosts).

### Tests added

- Storage: request context round trip, cascade on task removal, header
  injection rejection.
- Engine: a hotlink-protected server refuses a task without the browser
  Referer and serves one with it.
- Browser handoff: only HTTP(S) referrers without credentials are kept.
- Native host: acceptance only after persistence, refusal when the database
  cannot be opened, selected text in one LinkGrabber launch, identifier kept
  in sync with `tauri.conf.json`.
- Application: launch argument parsing.

### Verification

- `cargo fmt --all` - clean
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `cargo test --workspace` - passed (170 Rust tests)
- `npx tsc --noEmit`, `npm test` (23 tests), `vite build` - passed
- Verified on Linux; a Windows `npm run tauri -- build` is still required.

### Known limitations carried forward

- Cookies are still not handed over, so downloads that need a browser login
  can still fail with 401/403 in the application (the browser copy is then
  already cancelled). Needs an in-memory channel between the native host and
  the running application; planned with the refresh-link flow.
- `opener` is still scoped to `$DOWNLOAD`; widen it when category
  destinations are applied (Step 2).

## Review follow-up - Step 2: Stored settings now take effect

Status: Implemented in working tree (not committed)

Several features were persisted and exposed over IPC but never read by the
engine. They now change behaviour, and each has a UI to configure it.

### Implemented

- **Destination folders.** After probing, the engine evaluates the full rule
  set with the real MIME type and size and picks the folder in this order:
  matching rule folder, the category's folder, the default folder from
  Settings, the system Downloads folder. Only absolute paths are used.
  Settings has a folder picker (`tauri-plugin-dialog`) and a reset; every
  category card has Choose/Change folder and reset.
- **Bandwidth limits.** New `dm_core::ratelimit::RateLimiter`: a shared token
  bucket that may go into debt, so many connections together respect one
  rate. One application-wide limiter (setting `global_speed_limit`, applied
  live, even to running transfers) plus a per-task limiter when a rule sets
  `speed_cap`. A pause or cancel interrupts a throttling wait immediately.
  A rule's `max_connections` now also caps segmented connections.
- **Schedules use local wall-clock windows.** Schema v9 adds
  `window_start_minute` / `window_end_minute`. Daily and weekday schedules
  are evaluated in local time (`chrono::Local`, re-read every check), windows
  may cross midnight, and a weekday window belongs to the day it opens.
  Previously times were compared in UTC (3.5 hours off in Tehran), a daily
  window with a stop time only ever ran on its first day, and weekday
  schedules had no time window at all. Old rows keep working.
- **Completion actions run.** When a queue runner that processed work drains
  its queue, the schedule's action runs: Notify shows a Windows notification;
  Close app / Sleep / Hibernate / Shut down show a 60-second countdown banner
  with Cancel and a notification, and are skipped if any other download is
  still running. Stopping a queue by hand never triggers the action.
- **Keep awake.** New `dm-system` crate owns a dedicated thread that holds
  `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` while
  transfers run (global setting, default on) or while a running queue's
  schedule asks for it. Power actions use `SetSuspendState` and
  `shutdown /s`. Non-Windows builds compile to no-ops.
- **Rules editor.** Settings lists rules with enable switches, a readable
  summary ("from example.com → save to D:\Lectures, limit to 2048 KB/s"),
  delete, and an editor for domain / extension / URL pattern / minimum size
  conditions and folder / queue / priority / connections / speed actions.
  A rule without any condition is refused by storage.
- **Opening files goes through the backend.** `open_download_file` and
  `reveal_download_file` only open files of completed downloads, so the
  frontend no longer needs an opener scope; files in any destination folder
  now open (the old scope only allowed `$DOWNLOAD`).
- Add Download shows the real default folder.

### Tests added

- Rate limiter: unlimited, debt repayment, shared budget across callers,
  live limit changes, real elapsed time.
- Engine: rule folder beats category folder beats default folder; relative
  default folders refused; global limit slows a transfer and persists; a
  rule speed cap limits only matching tasks.
- Storage: category folder set/clear, rule update/delete, empty rules
  refused, schedule window validation, weekday schedules need a day.
- Schedules: local-time daily windows (Tehran offset), every day not only
  the first, overnight weekday windows.
- `dm-system`: power action names, keep-awake thread lifecycle.
- Frontend: schedule time and speed unit conversions; the IPC contract test
  now reads every Rust module of the desktop host.

### Verification

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (186 Rust tests) - passed
- `tsc --noEmit`, `npm test` (27 tests), `vite build` - passed
- The Windows-only code in `dm-system` was type-checked against
  `windows-sys` 0.61 on Linux; it still needs a Windows build and a manual
  sleep / shutdown test.

## Review follow-up - Windows verification of Steps 1 and 2

Status: Verified on Windows 11 (2026-09-23)

Steps 1 and 2 were written and verified on Linux, where the `cfg(windows)`
code in `dm-system` is compiled out and therefore never built or linted. The
full gate was rerun on Windows so that code is now actually compiled.

- `cargo fmt --all --check` - clean
- `cargo test --workspace` - passed (186 Rust tests, 0 failed)
- `cargo clippy --workspace --all-targets -- -D warnings` - passed, which now
  includes the Windows keep-awake and power-action paths
- `cargo check --workspace` and `cargo build -p tauri-app` - passed
- `tsc --noEmit`, `npm test` (27 tests), `npm run build` - passed

One environment fix was needed: `@tauri-apps/plugin-dialog` was declared in
`package.json` and the lockfile but not installed on this host, so the
frontend type check failed until `npm install` ran. No source change.

### Still not verified

- The Windows `npm run tauri -- build` release bundle was not produced here.
- Sleep, hibernate and shutdown were deliberately not triggered: running
  them would suspend or power off this machine. They need a manual test on a
  machine where that is acceptable.
- The single-instance forwarding and browser handoff were not exercised
  end to end against a real browser in this pass.

## Browser session handover (cookies)

Status: Implemented and verified on Windows 11 (2026-09-23)

Closes the limitation recorded in Step 1: downloads that need the browser
login failed with 401/403 in the application, after the browser had already
cancelled its own copy.

### Design

A cookie header is the most sensitive value the application handles, and
AGENTS.md forbids storing or logging it. The native host is a separate,
short-lived process, so the session needs a way into the running
application that avoids the two existing channels: the database (forbidden)
and the command line (readable by every process of the user, and recorded by
process auditing).

- **Extension:** off by default. Turning on "Hand over my browser login"
  requests the optional `cookies` permission and site access from a user
  gesture; turning it off removes both. The cookie header is built with
  `chrome.cookies.getAll({ url })`, so the browser's own domain, path and
  Secure matching decide what is included.
- **Transport (`dm-system::session_channel`):** a Windows named pipe owned
  by the application. Its DACL grants access to the current user's SID only
  (protected, no inherited ACEs); remote clients are rejected; it is created
  as the first instance of its name and a new instance always exists before
  a connected one is handed off, so no other process can take the name while
  the application runs. The name includes the user SID because pipes are
  machine-wide. The client connects at identification level, and before
  sending anything checks that the process owning the pipe is the installed
  application executable, so a program that took the name first receives
  nothing.
- **Native host:** persists the task without the session, delivers the
  session, and accepts the handoff only when the application confirms it. If
  delivery fails, the task is withdrawn and the handoff refused, so the
  browser resumes its own download. The session is attached before the task
  starts, so the first request already carries it.
- **Engine (`dm-core::session`):** the session lives in memory in
  `DownloadService` and is attached per request only when the request's
  scheme, host and port equal those it was captured for. It is marked
  sensitive in the HTTP client, printed as `<redacted>`, forgotten on
  completion, cancellation and removal, and lost on restart by design.

### Defect found and fixed on the way

- **The native host passed the browser's stdio to the application it
  launched.** A child inherits stdin and stdout by default, and the
  application logs to stdout, so its output could land inside the native
  messaging stream and corrupt the reply the browser was waiting for. This
  predates the session work and affected every handoff that had to start the
  application. The native host now launches it with null stdio. Found by
  running the real binary; covered by a regression test that fails without
  the fix.

### Tests added

- Session model: exact-origin matching (scheme, host, port; no sibling or
  parent domains), header-injection refusal, size and emptiness limits,
  non-HTTP sources, redacted `Debug`, errors that never carry the cookie.
- Engine, against local HTTP servers: a login-protected server refuses a
  task without the session and serves one with it; a redirect to another
  origin never carries it; **segmented ranged requests sent straight to a
  resolved CDN never carry it** (verified to fail when the origin check is
  removed: the CDN then received the cookie 8 times); the database and its
  write-ahead log never contain it; attachment only before start; forgotten
  on completion and cancellation.
- Transport, over real Windows named pipes: delivery to the expected
  program, nothing sent to any other program, refusals returned without the
  session, a second server cannot take a name in use, nothing listening is
  reported, per-user pipe name. The pipe tests were run five times in a row
  after a fix to the server, which had disconnected before the client read
  its reply.
- Native host: delivered and never stored, withdrawn when undelivered or
  sent to an untrusted program, requires a known application, requests never
  print their session. Integration tests run the real binary and require the
  stdout stream to hold exactly one framed reply.

### Verification

- `cargo fmt --all --check` - clean
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `cargo test --workspace` - passed (216 Rust tests)
- `cargo build -p tauri-app -p dm-native-host` - passed
- `tsc --noEmit`, `npm test` (27 tests), `npm run build` - passed
- Extension scripts pass `node --check`; the manifest parses.

### Not verified

- The full path with a real browser, the real extension and the real
  application was not exercised: running the application here would use this
  user's real database and Downloads folder. Each piece is covered above,
  and the transport was tested over real Windows pipes.
- Firefox's cookie API and partitioned (CHIPS) cookies were not tested;
  `getAll({ url })` returns unpartitioned cookies only.

### Known limitations

- A session is lost when the application restarts, on purpose. A download
  that needed a login and is resumed afterwards may be refused by the site;
  start it again from the browser.
- Handover needs Windows. Elsewhere the channel reports it is unsupported and
  the browser keeps the download.

## Review follow-up - Step 3: "Rud" interface, Persian first

The interface was rebuilt around a new visual identity (working name "Rud",
river) with Persian as the default language and English as the second.
The Rust service remains the single owner of download state; every new
control calls an existing or new backend command.

### Language, direction and formatting

- `src/i18n`: typed fa/en dictionaries (`messages.ts`, `pageMessages.ts`),
  `I18nProvider` / `useI18n()` returning `t`, `dir` and a formatter.
- Numbers use Persian digits in Persian; dates use the Persian (Jalali)
  calendar; byte units and rates are translated. English keeps Latin digits.
- The whole layout is written with logical CSS properties, so it mirrors in
  RTL. URLs, paths and file names stay left-to-right inside RTL text.
- Language and theme are stored by the backend (`ui_language`, `ui_theme`)
  through `get_ui_preferences` / `set_ui_preferences` and applied before the
  first paint. Vazirmatn is bundled (`@fontsource-variable/vazirmatn`), so no
  font is fetched from the network.

### Visual system

- New tokens (`styles/tokens.css`): dark default, light, and follow-system;
  saffron accent; status colours shared by pills, sparklines and the sidebar.
- Sidebar with the Rud mark, per-status counts and a collapsed 72px form
  below 1100px. Top bar with search, add, pause all / resume all and a
  global speed-limit control.
- Throughput band: live total speed with a 60-second sparkline.
- Download table: virtualized, sortable (added, name, progress, speed, size),
  file-type badges, per-row sparkline, status pills; columns drop with
  container queries on narrow windows.
- Details drawer: progress ring, one lane per connection, the adaptive
  engine's reason, speed history, properties and a session timeline.
- Refresh-link dialog replaces `window.prompt`.
- LinkGrabber keeps the Phase 8 additions (checks, numbered series, dropped
  files, large batches through the Default Queue), now translated.

### Desktop behaviour

- System tray (`src-tauri/src/tray.rs`): show, pause all, quit; the tooltip
  shows active downloads and total speed in the chosen language.
- Optional close-to-tray (`ui_close_to_tray`), off by default.
- `pause_all_downloads` pauses every running transfer (service `pause_all`).
- Keyboard: Ctrl+N add, Ctrl+F search, Ctrl+A select all, arrows to move,
  Space to pause/resume, Delete to remove, Esc to close.

### Verification

- `cargo fmt --all` - clean
- `cargo clippy --workspace --all-targets -- -D warnings` - passed
- `cargo test --workspace` - passed (220 Rust tests)
- `tsc --noEmit`, `npm test` (42 tests), `npm run build` - passed
- Screens checked with a mocked backend in Chromium: Persian dark, Persian
  light, English dark, settings, and narrow windows.

### Not verified

- Tray, close-to-tray, native dialogs and notifications need a Windows build
  of the application; they were only type-checked and unit-tested here.
- `npm install` is required once, for the bundled Vazirmatn font.

## Review follow-up - Step 4: Engine v2

### One preallocated partial file

- A segmented download writes every range straight into one `.part` file,
  preallocated at full size, at its own offset. The old per-range files and
  the final copy that joined them are gone: finishing a 10 GB file no longer
  reads and writes 10 GB again, and it no longer needs twice the disk space.
- On Windows the file is marked sparse before it is sized
  (`dm-system::sparse`), so NTFS does not zero-fill the gap in front of a
  late range. FAT32/exFAT refuse sparse files; downloads still work there.
- Each connection flushes the file (`sync_data`) before it records how far
  it got (at most every 8 MB or every second, and whenever it stops), so the
  stored count never runs ahead of what survives a power cut.
- Schema v10 rebuilds `download_segments` without the one-file-per-range
  constraint (rows are kept) and adds `traffic_usage`.
- A task paused by an older version, with ranges in separate files, is
  detected and downloaded again from the start (with a notice); the old
  files are removed.

### Work stealing

- Ranges are planned one per connection (never shorter than 1 MB). When a
  connection is free and no planned range is left, the largest range still
  running is split and its untouched half handed over (both halves at least
  512 KB). The split is recorded in one transaction
  (`Storage::split_download_segment`), which refuses any split that would cut
  into written bytes or leave a gap.
- A connection claims bytes from its range before writing them
  (`slot::RangeSlot`), so two connections can never write the same byte.
- The coordinator no longer reads every segment row from SQLite for each
  received chunk; it counts written bytes in memory and reports progress at
  most every 150 ms.

### Adaptive connections

- Starts with one connection and doubles while each step brings at least a
  10% gain (1, 2, 4, 8 within a few seconds). A step without gain is undone
  and remembered; after a while one extra connection is tried again. 429/503
  still remove a connection and back off.
- Throughput is judged over one-second windows during which the connection
  count did not change, so a connection that is still opening does not
  count as "no gain".
- The limit is a setting now (`max_connections_per_download`, default 8,
  1-64), shown in Settings.

### Proxy and routing

- `network_proxy_mode`: `off`, `system` (default, unchanged behaviour) or
  `manual` with `network_proxy_url` (http, https, socks5, socks5h). Proxy
  addresses with a user name or password are refused, so no credential is
  ever stored.
- Hosts in `network_direct_hosts` always go direct; with
  `network_domestic_direct` (default on) so do domestic hosts, so Iranian
  sites skip a foreign proxy.
- Changing the settings rebuilds the HTTP client; running transfers keep
  their connections.

### Domestic and international traffic

- Every download's bytes are counted per local day as domestic (`.ir` and
  the domains in `traffic_domestic_hosts`) or international.
- Optional international quota (`traffic_international_quota`) over a period
  starting on a chosen day (`traffic_period_start`) or the last 30 days.
  When the quota is used up, new international downloads are paused with
  the notice code `quota` instead of starting; domestic downloads and
  transfers already running are not affected.
- Settings shows the split, the quota bar, today and this month; the
  download page shows a small usage chip.

### Interface

- Settings: connections per download, a traffic and quota section, a network
  and proxy section.
- The adaptive engine's reasons and the `quota`, `interrupted` and
  `restarted` notices are shown in the interface language.
- Dates of the counting period use the Solar Hijri calendar in Persian.

### Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check --workspace` - passed
- `cargo test --workspace` - passed (251 Rust tests). New tests cover the
  shared file, splitting a running range, a split that stops a connection at
  its new end, refusing splits that cut into written bytes, migration v10
  keeping rows, replacing an old per-range layout, the doubling ramp and its
  ceiling, proxy validation, routing through a manual proxy and around it
  for direct and domestic hosts, traffic classification and local days,
  quota enforcement, and settings round trips.
- `tsc --noEmit`, `npm test` (50 tests), `npm run build` - passed
- Screens checked with a mocked backend (Persian dark, English light).

### Not verified

- Sparse files, the Windows proxy behaviour and real-world throughput need
  a Windows build and real servers. The sparse call was type-checked against
  `windows-sys` 0.61 only.
- The quota is checked when a download starts, not while it runs.
- Domestic detection is by domain name, not by IP address.

## Review follow-up - Step 5: Links, streams, mirrors, control

### Expired links

- A 401/403/404/410 on a download that had already transferred bytes, or on
  a link carrying signature or expiry parameters (S3, Google Cloud,
  CloudFront, nginx `secure_link`...), fails with the code `link_expired`
  and a message saying how to go on. It is not retried.
- Refreshing a link by hand now keeps the downloaded bytes
  (`Storage::adopt_source_url`); on the next start the engine probes the new
  link and continues only if the size and validators still match, otherwise
  it starts over with a notice. The old behaviour discarded everything.
- A fresh link added in the window or sent by the browser for a file whose
  download stopped part-way continues that download instead of creating a
  second copy (`adopt_fresh_link`): the new link is probed and must report
  the same file name and size, and the same ETag when both have one; exactly
  one stopped download must match. The request context and any browser
  session move with it; the new row is removed. Setting `auto_adopt_links`
  (default on).

### Polite hosts and Retry-After

- Hosts in `polite_hosts` get at most two connections and no splitting of
  running ranges.
- A `Retry-After` (seconds, capped at an hour) on 429/503 sets the next
  automatic attempt at least that far away.

### Mirrors

- A download can have up to 16 other addresses (`download_mirrors`, schema
  v11). Before a segmented transfer each mirror is probed and used only if it
  serves ranges of a file with the same size and validator. Ranges take turns
  across the trusted sources; a source that fails a range is dropped and the
  range, with its durable progress, goes back to be finished elsewhere.
- Details panel: add and remove mirrors.

### HLS streams (unprotected)

- Master and media playlists (quoted attributes, `EXT-X-MAP`,
  `EXT-X-BYTERANGE`, media sequence). The best quality that carries its own
  sound, at or below `stream_max_height`, is chosen; the Add dialog lists the
  qualities of a `.m3u8` link to pick one by hand.
- Parts are fetched four at a time and appended in order. `AES-128`
  (clear-key) parts are decrypted with the explicit IV or the sequence
  number. Transport streams are saved as `.ts`, fragmented MP4 as `.mp4`,
  named after the show rather than `index.m3u8`.
- Pause and restart continue from the parts already written (a small
  `.stream` record next to the partial file).
- Refused with a reason and not retried: DRM (SAMPLE-AES, any key format
  other than `identity`, protected session keys) `protected_stream`; live
  playlists `live_stream`; qualities with a separate sound track
  `needs_muxing`; DASH `unsupported_stream`.

### Command line and command palette

- `rud` (`crates/dm-cli`): add, list, status, pause, resume, retry, cancel,
  pause-all, queue start/stop. Reading commands open the database directly
  (WAL); acting commands are passed to the application through its
  single-instance launch arguments (`--control`, `--refresh`), which the
  application carries out with the same rules as its buttons. Short ids are
  accepted when unambiguous.
- A local HTTP API was not added: it would be a new way in for any program or
  web page on the computer, and needs an authentication design first.
- Ctrl+K opens a command palette: every action, every page and every download
  by name or host; English keywords also find Persian commands. Ctrl+, opens
  Settings.

### Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check --workspace` - passed
- `cargo test --workspace` - passed (285 Rust tests). New tests cover
  expired-link detection, signed-link recognition, adoption and its refusal
  for a different file, refresh keeping bytes, Retry-After, polite hosts,
  mirrors sharing ranges, a broken mirror dropped mid-transfer, a mirror of
  another file never used, the HLS parser (attributes, ranges, keys, DRM,
  live), AES-128 decryption, a full HLS download with a master playlist and
  an encrypted part, pausing and resuming a stream, refused stream kinds,
  migration v11, the CLI commands, and the new launch switches.
- `tsc --noEmit`, `npm test` (58 tests), `npm run build` - passed
- Screens checked with a mocked backend (Persian dark).

### Not verified

- Real HLS sites, real mirrors, and the CLI talking to a running Windows
  build.
- DASH and streams whose sound is a separate track need FFmpeg; not done.

## Review follow-up - Step 6: FFmpeg, DASH, separate sound

### FFmpeg

- `dm-core::ffmpeg`: FFmpeg is used only where the user has it: a path set
  in Settings (`ffmpeg_path`, must be an existing absolute file), otherwise
  next to the application, otherwise on `PATH`. It is never downloaded or
  bundled. It runs with an argument list, never through a shell, on local
  files only, with `-c copy` (no re-encoding), no console window on
  Windows, and is killed when the download is paused or cancelled. Its
  error output (trimmed) becomes the task's message, code `ffmpeg_failed`.
- Settings shows whether FFmpeg was found, where and which version, lets
  the user choose the program or go back to finding it automatically.

### Streams with separate picture and sound

- HLS: `EXT-X-MEDIA` audio renditions are read; the default rendition with
  an address is preferred. With FFmpeg every quality can be chosen, and the
  sound track is downloaded next to the picture and joined into an MP4.
  Without FFmpeg only qualities that carry their own sound are chosen, and
  a stream that has none fails with `needs_muxing`, which says to choose
  FFmpeg in Settings.
- DASH (`dm-core::dash`, `roxmltree`): static single-period manifests,
  `BaseURL` chains, `SegmentTemplate` with `$Number$`/`$Time$`/widths and
  `SegmentTimeline` (including `r="-1"`), `SegmentList` with byte ranges,
  and single-file representations. `ContentProtection` is refused
  (`protected_stream`), `dynamic` manifests are live (`live_stream`),
  several periods are not supported. The best picture at or below the
  quality limit and the best sound are downloaded and joined.
- Each track downloads into its own file with its own resume record; after
  joining, the track files and records are removed. A pause during the join
  keeps the finished tracks, and the next start only joins them.
- Transport streams (`.ts`) are rewrapped as `.mp4` when FFmpeg is there
  (`stream_prefer_mp4`, default on).
- A quality chosen in the Add dialog travels in the link as
  `#rud-quality=<height>`; the fragment never reaches the server. The
  picker lists DASH qualities too, and disables qualities with separate
  sound only when FFmpeg is missing.

### Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check --workspace` - passed
- `cargo test --workspace` - passed (300 Rust tests). With a
  real FFmpeg 6.1 in the test environment, end-to-end tests generate real
  media and download it through the engine: an HLS stream with separate
  sound joined into an MP4 (both streams checked with ffprobe), a DASH
  stream made by FFmpeg's own DASH muxer (SegmentTemplate + timeline)
  downloaded and joined, and a transport stream rewrapped as MP4 and, with
  the setting off, kept as `.ts`. Where FFmpeg is missing these tests say
  so and skip; the parser and refusal tests always run.
- `tsc --noEmit`, `npm test` (59 tests), `npm run build` - passed

### Not verified

- FFmpeg on Windows (the creation flag that hides its console was
  type-checked only), and real DASH/HLS sites.

## Review follow-up - Step 7: After the download

### Integrity

- `dm-core::postprocess`: MD5, SHA-1 and SHA-256 (`md-5`, `sha1`, `sha2`).
  An expected checksum is accepted as the bare value, `sha256:<hex>` or a
  `<hex>  <filename>` line from a checksum file; the algorithm follows
  from the length, and case does not matter.
- Schema v12 adds `download_checks` (one row per download, removed with
  it): expected and actual checksum, result (`verified`, `mismatch`,
  `error`), scan result, unpacked folder and the errors of each step.
- The expected checksum can be set before or after the download. Set
  before, it is checked when the download finishes; set after, it is
  checked straight away. A mismatch keeps the file and leaves the notice
  `integrity_failed` on the completed row, which stays visible in the list.

### Other steps (Settings > After the download, all off by default)

- Always compute SHA-256, to compare by hand.
- Windows Defender scan (`MpCmdRun.exe -Scan -ScanType 3 -DisableRemediation`):
  the scan only reports; nothing is deleted or quarantined by the
  application. A threat leaves the notice `threat_found`, and the file is
  then neither unpacked nor handed to the user's command. The switch is
  disabled when Defender is not found.
- Unpack ZIP archives (`zip`, deflate only) into a new folder beside the
  archive (`name`, `name (2)`, …); the archive is kept. Members that would
  escape the folder and links are skipped; encrypted archives, more than
  100,000 entries, more than 64 GB or a compression ratio over 200× are
  refused.
- A user command with `{file}`, `{folder}` and `{name}`, split with double
  quotes and run directly (never through a shell) with a 10-minute limit
  and no console window.
- The steps run in the background after the download is marked completed,
  so finishing is never delayed; the window hears
  `download-checks-changed` and refreshes the row and the details panel.
  They can be run again from the details panel. The unpacked folder is
  opened from the engine's own record, never from a path the window sends.

### Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check --workspace` - passed
- `cargo test --workspace` - passed (315 Rust tests): checksum parsing and
  hashing against known values, zip-slip, links, zip bombs and encrypted
  archives, command splitting, storage round trip, and service tests that
  download a file and verify it, flag a wrong checksum set afterwards,
  unpack a downloaded archive and run a command on the finished file.
- `tsc --noEmit`, `npm test` (62 tests), `npm run build` - passed

### Not verified

- The Windows Defender scan and the command's hidden console on Windows
  (type-checked only; the test environment is Linux).
