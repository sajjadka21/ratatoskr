# Implementation Plan: Download Manager Phases 0-4

## Overview

Preserve the checkpoint architecture while separating persistent task creation from background transfer execution. Rust remains authoritative, task IDs stay stable, and React only projects backend records and events.

## Architecture Decisions

- Reuse schema version 1 because it already represents created and active tasks; do not manufacture an empty migration.
- Put legal lifecycle rules in `dm-common`, atomic persistence transitions in `dm-storage`, orchestration in `dm-core`, serialized contracts in `dm-ipc`, and background spawning in the thin Tauri host.
- Create every task before any network operation. Starting accepts only a persisted task ID.
- Use a task event channel for background progress and terminal refresh signals; React does not infer authoritative lifecycle transitions.

## Task List

### Phase 0: Audit

- [x] Inspect repository, schema, APIs, UI flow, and Git state.
- [x] Verify the implementation against the Current Baseline.
- [x] Run the unmodified checkpoint through the full quality gate.
- [x] Record gaps and the Phase 1 plan.

### Phase 1: Persistent Task Architecture

- [x] Define and test canonical legal transitions.
- [x] Implement and test atomic storage transitions.
- [x] Split create/start service operations and test stable identity/no duplicates.
- [x] Add create/start IPC and background task events.
- [x] Implement Start Now and Download Later UI flow with immediate modal close and row insertion.
- [x] Run the full quality gate and document the result.

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Duplicate start commands race | Duplicate writes or transfers | Atomically claim `created -> probing` before spawning work. |
| Background task failure is lost | Stale UI or persisted state | Persist failure in `dm-core` and emit a terminal task event. |
| Frontend invents lifecycle state | Rust/React divergence | Return authoritative records and refresh from storage on task events. |
| Source URLs expose signed secrets in logs | Privacy/security issue | Log task IDs and destinations only. |
| Existing user databases break | Data loss | Keep schema version 1 unchanged for Phase 1 and exercise reopen tests. |

## Checkpoint

- [x] All Phase 1 acceptance scenarios pass.
- [x] Formatting, tests, check, strict Clippy, and frontend build pass.
- [x] Phase 1 is committed as one focused, reviewable change.

### Phase 2: Queue Foundation

- [ ] Add queue/priority domain models and schema v2 migration.
- [ ] Add persistent queue assignment, ordering, priority, and state operations.
- [ ] Add the queue runner with queue and host concurrency enforcement.
- [ ] Add queue IPC and background event delivery.
- [ ] Add queue management and Add Download queue UI.
- [ ] Run the Phase 2 gate, review, document, and commit.

## Phase 2 Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Two runners start the same task | Duplicate transfer/corruption | Atomically claim `queued -> probing` and keep an in-process queue-runner registry. |
| Queue stop cancels active work without Phase 3 controls | Partial files or invalid state | Stop only new scheduling in Phase 2; let already-active tasks finish. |
| Host limits are bypassed | Server overload | Select candidates by parsed host and active-host counters before claim. |
| Migration damages existing history | Data loss | Use additive schema v2 changes and test v1-to-v2 reopen migration. |
| React becomes authoritative | State divergence | Return and emit Rust-persisted queue/download records for every action. |

## Phase 2 Checkpoint

- [ ] Default and named queues survive reopen.
- [ ] Queued tasks start only through a running queue and retain stable IDs.
- [ ] Queue order, priority, queue concurrency, and host concurrency are tested.
- [ ] Full repository quality gate passes.

### Phase 3: Pause / Resume / Recovery

- [x] Extend canonical lifecycle rules and add task control handles.
- [x] Persist attempts/retry time and recover orphaned tasks at startup.
- [x] Probe validators, preserve partial files, and classify bounded retries.
- [x] Add pause/resume/cancel/retry IPC and UI behavior.
- [x] Run the Phase 3 quality gate and document the boundary.

### Phase 4: Segmented Engine

- [ ] Add a schema v4 persistent segment map without changing existing task identity.
- [ ] Add deterministic, gap-free segment planning with conservative connection limits.
- [ ] Add ranged segment transfer with strict `206` and `Content-Range` validation.
- [ ] Persist segment progress and resume completed/partial segments safely after restart.
- [ ] Assemble verified segments in order, atomically finalize, and remove segment files.
- [ ] Fall back to the existing single-stream engine when range capability is absent or invalid.
- [ ] Run focused local HTTP tests, the full quality gate, review, document, and commit.

## Phase 4 Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| A server advertises ranges but returns `200` | Corrupt or duplicated bytes | Require `206` and matching `Content-Range` for every segment; discard the segment set and use single stream. |
| Crash leaves inconsistent segment rows/files | Unsafe resume | Persist each segment's offset, compare file length, truncate to the persisted offset, and revalidate source identity before reuse. |
| Segment completion order differs from byte order | Corrupt final file | Assemble only by ordered segment index into one synced temporary file, then rename atomically. |
| Too many connections hurt reliability | Server overload or throttling | Use a bounded segment worker pool and leave adaptive scaling to Phase 5. |
| Legacy databases fail to open | Data loss | Additive v4 migration and reopen tests from v3 with existing task history. |

## Phase 4 Checkpoint

- [ ] Existing single-stream and resume behavior remains green.
- [ ] Segmented transfers produce byte-identical files under a range-capable server.
- [ ] Interrupted segment maps resume without duplicate or overlapping bytes.
- [ ] No-range and invalid-range responses safely use single-stream fallback.
- [ ] Full repository quality gate passes and the phase is committed.
