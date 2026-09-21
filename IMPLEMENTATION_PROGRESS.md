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

Status: In progress

### Plan

1. Add canonical transition validation and tests in `dm-common`.
2. Add atomic, status-guarded persistence transitions and lifecycle tests in `dm-storage`.
3. Split task creation from transfer execution in `dm-core`; validate URLs locally, start only an existing task ID, preserve the ID through completion/failure, and test that no duplicate record is created.
4. Add create/start IPC contracts and thin Tauri commands; claim the task before spawning background work and stream task events without logging source URLs.
5. Update React so Start Now and Download Later create rows immediately, close the modal immediately, and let background events update the list.
6. Run the complete quality gate, update this document with results, and create the focused Phase 1 commit.

### Scope boundary

Phase 1 will not implement queues, pause/resume, segmented transfer, adaptive connections, scheduler, browser integration, or media extraction. Those remain assigned to later phases in the master specification.
