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
