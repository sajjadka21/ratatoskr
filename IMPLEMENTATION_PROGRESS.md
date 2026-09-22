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
