# Ratatosk

A modern local-first download manager for Windows, Persian first and bilingual.

Named after Ratatosk, the squirrel of Norse myth who runs up and down the
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
