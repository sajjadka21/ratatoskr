# Ratatoskr

A modern local-first download manager for Windows, Persian first and bilingual.

Named after Ratatoskr, the squirrel of Norse myth who runs up and down the
world tree carrying messages between its top and its roots: this one brings
files down. The command-line tool is `tosk`.

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
- dm-system - Windows power, sleep, sparse files, locating the app
- dm-cli - `tosk`, the command-line tool
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

- `Ratatoskr_1.0.1_x64-setup.exe` (NSIS installer)
- `Ratatoskr_1.0.1_x64_en-US.msi` (MSI installer)

The installer carries the browser extension folder; release builds made with
`scripts\release.cmd` also carry `dm-native-host.exe` and `tosk.exe`.

## Browser extension

The extension in `browser-extension/` works in Chrome, Edge, Brave and
Firefox. Ratatoskr connects it by itself: every start registers the
`dm-native-host.exe` next to it for the current Windows user. Settings →
Browser extension shows the connection and opens the folder and each
browser's extensions page for loading it. Takeover is off by default, and
cookies, credentials, authorization headers and access tokens are never
stored. See `browser-extension/README.md`.

## Releases and updates

`scripts\release.cmd` builds a signed installer (with the browser connector
and `tosk.exe`) and the `latest.json` installed copies read to update. See
`RELEASING.md`.

## Command line (`tosk`)

Build with `cargo build --release -p dm-cli` and place `tosk.exe` next to the
application. It uses the application's own database and hands every action to
the running application (starting it when needed), so there is one engine and
one record of every download:

```text
tosk add https://example.com/file.iso          # add and start
tosk add --later https://example.com/big.zip   # add, start later
tosk list [--status downloading] [--json]
tosk status 3f2a9c1e
tosk pause|resume|cancel <id>...               # ids may be shortened
tosk pause-all
tosk queue start "Default Queue"
```

## Principles

- Local-first
- No mandatory backend
- Rust owns critical state
- Reliability before feature count
- Never store credentials or browser secrets in logs
- Recovery and correctness are more important than raw feature count

## Android licence

Android is separately licensed under GPL-3.0-only; desktop retains PolyForm Noncommercial. See LICENSES.md and android/NOTICE.
