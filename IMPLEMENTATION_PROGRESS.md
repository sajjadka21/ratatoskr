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

Status: In progress

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
