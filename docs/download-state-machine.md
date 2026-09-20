# Download State Machine

The Rust backend defines the canonical download states.

Created
  |
  v
Probing
  |
  v
Queued
  |
  v
Downloading
  |
  +----> Paused
  |
  +----> Retrying
  |
  v
Finalizing
  |
  v
Completed

Canonical states:

- created
- probing
- queued
- downloading
- paused
- retrying
- finalizing
- completed
- failed
- cancelled

The frontend must not define an incompatible state model.

Actual transition rules will be implemented during later download-engine milestones.
