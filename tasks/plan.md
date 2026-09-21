# Implementation Plan: Phase 0 Audit and Phase 1 Persistent Tasks

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

- [ ] Define and test canonical legal transitions.
- [ ] Implement and test atomic storage transitions.
- [ ] Split create/start service operations and test stable identity/no duplicates.
- [ ] Add create/start IPC and background task events.
- [ ] Implement Start Now and Download Later UI flow with immediate modal close and row insertion.
- [ ] Run the full quality gate and document the result.

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Duplicate start commands race | Duplicate writes or transfers | Atomically claim `created -> probing` before spawning work. |
| Background task failure is lost | Stale UI or persisted state | Persist failure in `dm-core` and emit a terminal task event. |
| Frontend invents lifecycle state | Rust/React divergence | Return authoritative records and refresh from storage on task events. |
| Source URLs expose signed secrets in logs | Privacy/security issue | Log task IDs and destinations only. |
| Existing user databases break | Data loss | Keep schema version 1 unchanged for Phase 1 and exercise reopen tests. |

## Checkpoint

- [ ] All Phase 1 acceptance scenarios pass.
- [ ] Formatting, tests, check, strict Clippy, and frontend build pass.
- [ ] Phase 1 is committed as one focused, reviewable change.
