# Download Manager

A modern local-first download manager for Windows.

## Current Status

Milestone 0 - Application foundation.

Implemented:

- Tauri 2 desktop shell
- React + TypeScript + Vite frontend
- Rust workspace
- SQLite persistence
- Migration system
- Shared download status model
- Tauri IPC
- Backend health checks
- Structured logging

No download engine exists yet.

## Architecture

Rust is the authoritative application layer.

React is responsible only for presentation and user interaction.

Main crates:

- dm-common - shared domain types
- dm-core - application and download logic
- dm-storage - SQLite persistence
- dm-ipc - IPC contracts
- src-tauri - desktop application host

## Development

npm install
npm run tauri dev

Rust quality checks:

cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace

Frontend build:

npm run build

## Principles

- Local-first
- No mandatory backend
- Rust owns critical state
- Reliability before feature count
- Never store credentials or browser secrets in logs
- Recovery and correctness are more important than raw feature count
