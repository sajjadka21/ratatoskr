# Download Manager

A modern local-first download manager for Windows.

## Current Status

Windows release `1.0.0` builds from the Rust download engine and Tauri UI.
Persistent tasks, queues, recovery, segmented/resumable transfers, adaptive
connections, categories/rules, schedules, LinkGrabber inspection, browser
handoff validation, media classification, and safety guards are implemented.

## Architecture

Rust is the authoritative application layer.

React is responsible only for presentation and user interaction.

Main crates:

- dm-common - shared domain types
- dm-core - application and download logic
- dm-storage - SQLite persistence
- dm-ipc - IPC contracts
- dm-native-host - Chrome/Edge/Firefox Native Messaging stdio host
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

## Release build

The release command builds the Windows executable plus NSIS and MSI installers:

```powershell
$env:npm_config_prefix = 'C:\Program Files\nodejs'
npm run tauri -- build
```

Outputs are written under `target/release/bundle/`:

- `Download Manager_1.0.0_x64-setup.exe` (NSIS installer)
- `Download Manager_1.0.0_x64_en-US.msi` (MSI installer)
- `Download-Manager-1.0.0.exe` (portable executable copy)
- `Download-Manager-Browser-Extension-1.0.0.zip` (MV3 extension + native host)

## Browser extension

Load `browser-extension/` as an unpacked extension in Chrome/Edge, or use the
release ZIP. Build the host with `cargo build --release -p dm-native-host`,
place it next to the application, replace the extension ID in
`native-messaging-host.json`, and register the host manifest in the browser's
Native Messaging registry/directory. Takeover is disabled by default and all
cookies, credentials, authorization headers, and access tokens are excluded.

## Principles

- Local-first
- No mandatory backend
- Rust owns critical state
- Reliability before feature count
- Never store credentials or browser secrets in logs
- Recovery and correctness are more important than raw feature count
