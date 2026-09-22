# Phase 0 and Phase 1 Tasks

## Task 0: Audit checkpoint

**Acceptance criteria:**

- [x] Current Git state and checkpoint are recorded.
- [x] Baseline features and Phase 1 gaps are verified against source.
- [x] The complete repository quality gate passes.

**Verification:** repository commands from the master specification.

**Dependencies:** None.

## Task 1: Canonical state transitions

**Acceptance criteria:**

- [x] Legal Phase 1 transitions are explicit in `dm-common`.
- [x] Illegal transitions return a typed error.
- [x] Unit tests cover allowed and rejected transitions.

**Verification:** focused `dm-common` tests, then workspace tests.

**Dependencies:** Task 0.

## Task 2: Atomic storage lifecycle

**Acceptance criteria:**

- [x] A persisted created task can be atomically claimed for start.
- [x] A second claim is rejected without changing identity or adding a row.
- [x] Storage lifecycle tests cover creation, transitions, completion, and failure.

**Verification:** focused `dm-storage` tests, then workspace tests.

**Dependencies:** Task 1.

## Task 3: Create and start service operations

**Acceptance criteria:**

- [x] Creating a task validates locally and performs no network request.
- [x] Starting accepts an existing task ID and preserves it through completion/failure.
- [x] Integration tests prove one database row per task lifecycle.

**Verification:** focused `dm-core` tests against a local HTTP server.

**Dependencies:** Task 2.

## Task 4: Background IPC lifecycle

**Acceptance criteria:**

- [x] Create-task IPC returns an immediately renderable persisted record.
- [x] Start-by-ID claims before background spawn and returns immediately.
- [x] Progress and terminal task events contain no sensitive request context.

**Verification:** Rust tests plus workspace check and strict Clippy.

**Dependencies:** Task 3.

## Task 5: Start Now and Download Later UI

**Acceptance criteria:**

- [x] Both actions create persisted task rows and close the modal immediately.
- [x] Download Later performs no transfer.
- [x] Start Now starts the same IDs in the background and updates rows from backend events.

**Verification:** TypeScript build and Rust lifecycle tests.

**Dependencies:** Task 4.

## Task 6: Phase 1 gate and commit

**Acceptance criteria:**

- [x] All five required commands pass.
- [x] `IMPLEMENTATION_PROGRESS.md` contains final Phase 1 evidence.
- [x] The Phase 1 diff is committed without unrelated files.

**Verification:** Git status and commit inspection.

**Dependencies:** Tasks 1-5.

## Task 7: Queue domain and migration

**Acceptance criteria:**

- [x] Canonical queue state and priority models are serializable and validated.
- [x] Schema v2 creates the Default Queue and adds queue/priority assignment without resetting v1 data.
- [x] Fresh and migrated databases both reopen at schema version 2.

**Verification:** focused `dm-common` and `dm-storage` migration tests.

**Dependencies:** Task 6.

## Task 8: Persistent queue operations

**Acceptance criteria:**

- [x] Named queues can be created/listed and running/stopped state persists.
- [x] Tasks can be enqueued, moved, removed, prioritized, and reordered transactionally.
- [x] Invalid queue/task operations return typed errors.

**Verification:** focused `dm-storage` queue integration tests.

**Dependencies:** Task 7.

## Task 9: Queue runner

**Acceptance criteria:**

- [x] Queued tasks start only through a running queue.
- [x] Per-queue and per-host concurrency limits are enforced.
- [x] Stable IDs and persisted terminal states are preserved.

**Verification:** local HTTP integration tests in `dm-core`.

**Dependencies:** Task 8.

## Task 10: Queue IPC and UI

**Acceptance criteria:**

- [x] Queue list/create/start/stop/assignment/reorder commands are thin Rust-backed IPC.
- [x] Queues are manageable from the sidebar and queued task rows show queue/priority hints.
- [x] Add Download supports adding a batch to the Default Queue or a selected named queue.

**Verification:** workspace Rust tests and `npm run build`.

**Dependencies:** Task 9.

## Task 11: Phase 2 gate and commits

**Acceptance criteria:**

- [x] All five required commands pass.
- [x] Code review has no unresolved required findings.
- [x] Progress documentation and focused Phase 2 commits are complete.

**Verification:** Git status/log inspection.

**Dependencies:** Tasks 7-10.

# Phase 2.5 Tasks: Defect Closure

## Task 12: Restart recovery for orphaned tasks

**Acceptance criteria:**

- [x] Statuses only an executor can own are canonical in `dm-common`.
- [x] Startup returns orphaned rows to an actionable state and explains why.
- [x] Storage tests cover queued and non-queued recovery and the no-op case.

**Verification:** `dm-common` and `dm-storage` tests.

**Dependencies:** Phase 2.

## Task 13: Application-level engine events

**Acceptance criteria:**

- [x] Runner and transfer events are published to the window, not to a per-invoke channel.
- [x] A queue resumed at startup reports progress to the UI.
- [x] Starting an already running queue reuses its runner instead of failing silently.

**Verification:** frontend IPC contract test plus manual runtime check.

**Dependencies:** Task 12.

## Task 14: Queue slot refill and enablement

**Acceptance criteria:**

- [x] A running queue starts newly added work while other transfers are active.
- [x] A disabled queue neither starts nor schedules.
- [x] Local HTTP tests cover both, and fail without the fix.

**Verification:** `dm-core` queue tests.

**Dependencies:** Task 13.

## Task 15: Removable tasks

**Acceptance criteria:**

- [x] Created and queued tasks can be removed.
- [x] The delete is guarded in SQL against a concurrent claim.
- [x] The dialog explains what happens when no file exists yet.

**Verification:** `dm-storage` removal tests.

**Dependencies:** Task 12.

## Task 16: Measured speed and ETA

**Acceptance criteria:**

- [x] Rate and remaining time are measured in the engine, never in React.
- [x] Rows, the details panel and the top bar show real measurements only.
- [x] Meter unit tests cover the first window, smoothing, stalls and unknown totals.

**Verification:** `dm-core` throughput tests and the frontend format tests.

**Dependencies:** Task 13.

## Task 17: Frontend test runner

**Acceptance criteria:**

- [x] `npm test` runs a real suite.
- [x] Helpers and link extraction are covered.
- [x] Command names and argument names are checked against the Rust surface.

**Verification:** `npm test`.

**Dependencies:** None.

# Phase 4 Tasks: Segmented Engine

## Task 18: Segment domain and migration

**Acceptance criteria:**

- [x] Segment status/range models are canonical and validated.
- [x] Schema v4 persists task segment maps and migrates v3 data without reset.
- [x] Segment storage tests cover creation, updates, completion, and reopen.

**Verification:** focused `dm-common` and `dm-storage` tests.

**Dependencies:** Phase 3.

## Task 19: Range segment transfer

**Acceptance criteria:**

- [x] Every segment request requires a matching `206` and `Content-Range`.
- [x] Segment files are bounded, independently resumable, and control-aware.
- [x] Invalid range responses are typed and never appended to existing bytes.

**Verification:** local HTTP tests for valid ranges, ignored ranges, short bodies, and pause/cancel.

**Dependencies:** Task 18.

## Task 20: Segmented service orchestration

**Acceptance criteria:**

- [x] Planner creates gap-free ranges with bounded worker concurrency.
- [x] Segment progress persists and aggregate task progress remains authoritative.
- [x] Ordered assembly is synced and atomically finalized; fallback uses single stream.

**Verification:** end-to-end service tests for completion, interruption/reopen, source change, and fallback.

**Dependencies:** Task 19.

## Task 21: Phase 4 gate and commit

**Acceptance criteria:**

- [x] All five required commands pass.
- [x] Code review has no unresolved required findings.
- [x] Progress documentation and focused Phase 4 commits are complete.

**Verification:** Git status/log inspection and full quality gate.

**Dependencies:** Tasks 18-20.

# Phase 5 Tasks: Adaptive Connections + Speed

## Task 22: Adaptive controller

**Acceptance criteria:**

- [x] Throughput samples produce bounded, deterministic connection targets.
- [x] The controller explains scale-up, stable/diminishing returns, and
  server backoff decisions.
- [x] Unit tests cover conservative start, useful gains, no-gain plateaus,
  and target reduction.

**Verification:** focused `dm-core` adaptive tests.

**Dependencies:** Phase 4.

## Task 23: Host profile persistence

**Acceptance criteria:**

- [x] Additive schema migration stores normalized host-only observations.
- [x] 429/503 counts, preferred cap, and last-seen time survive reopen.
- [x] Credentials, headers, paths, queries, and fragments never enter the
  profile key or persisted value.

**Verification:** `dm-storage` migration/reopen and redaction tests.

**Dependencies:** Task 22.

## Task 24: Adaptive segmented execution

**Acceptance criteria:**

- [x] Segmented workers start conservatively and scale only after measured
  gains, within global/task/queue/per-host bounds.
- [x] 429/503 responses reduce the target and schedule bounded backoff.
- [x] Aggregate progress and final bytes remain authoritative and unchanged.

**Verification:** local HTTP service tests for scale-up, diminishing returns,
  429/503 backoff, and byte-identical completion.

**Dependencies:** Tasks 22-23.

## Task 25: Explainability contract and Phase 5 gate

**Acceptance criteria:**

- [x] IPC progress exposes active/max connections and the adaptive reason.
- [x] Existing UI renders the measured explanation without inventing state.
- [x] All five required commands pass and Phase 5 is committed.

**Verification:** IPC/frontend tests plus the full repository quality gate.

**Dependencies:** Task 24.

# Phase 6 Tasks: Main UX Completion

## Task 26: Restart and task properties

**Acceptance criteria:**

- [x] Restart from zero keeps the task ID and clears partial/segment state.
- [x] Rust validates the operation against the canonical lifecycle.
- [x] Details/context actions expose restart, refresh source, and properties.

**Dependencies:** Phase 5.

## Task 27: Bulk selection and actions

**Acceptance criteria:**

- [x] Ctrl/Shift selection and select-all are keyboard accessible.
- [x] Bulk start/pause/resume/retry/cancel/remove use Rust commands and refresh.
- [x] Queue and priority bulk actions preserve existing queue constraints.

**Dependencies:** Task 26.

## Task 28: Phase 6 gate

**Acceptance criteria:**

- [x] Details/list rows expose measured speed, ETA, status, and valid actions.
- [x] Keyboard shortcuts and notification feedback are covered by tests.
- [x] All five required commands pass and Phase 6 is committed.

**Dependencies:** Task 27.
