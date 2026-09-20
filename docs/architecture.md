# Architecture

## Overview

React / TypeScript
        |
        | Tauri IPC
        v
Tauri Desktop Host
        |
        +---------+
        |         |
        v         v
    dm-core   dm-storage
                 |
                 v
              SQLite

## Ownership

Rust owns authoritative application state.

Frontend state is only a projection of backend state.

## Crates

### dm-common
Shared domain types. Contains the canonical DownloadStatus.

### dm-core
Application and download-engine logic.

### dm-storage
SQLite persistence, schema migrations and health checks.

### dm-ipc
Serializable IPC contracts.

### src-tauri
Application bootstrap, dependency wiring, commands and runtime state.

## Persistence

Database location is resolved through the Tauri application data directory.
Current schema version: 1.

## Security

Sensitive credentials must never be persisted or logged unless a future
feature explicitly requires secure OS-backed storage.
