# Download Manager browser extension

This MV3 extension is intentionally opt-in. It stores only takeover settings,
host/type exclusions, and a size threshold in browser-local storage. It never
stores cookies, credentials, authorization headers, or access tokens.

Build the native host with:

```powershell
cargo build --release -p dm-native-host
```

Copy `target/release/dm-native-host.exe` beside the installed Download Manager
executable, replace `REPLACE_WITH_EXTENSION_ID` in the native-host manifest,
replace the absolute `path` in that manifest, and register it using the
Chrome/Edge Native Messaging registry key.
Firefox requires the same host manifest under its native-messaging-hosts
directory and a Firefox-specific allowed origin.
