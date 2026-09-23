# Download Manager browser extension

This MV3 extension is intentionally opt-in. It stores only takeover settings,
host/type exclusions, and a size threshold in browser-local storage. It never
stores cookies, credentials, authorization headers, or access tokens.

## Browser login handover (optional, off by default)

Some downloads only work while signed in. With "Hand over my browser login"
turned on in the extension options, the extension reads the cookie header the
browser would send to that one download URL and passes it to Download Manager.

- Turning it on asks the browser for the `cookies` permission and access to
  sites; turning it off gives both back. Neither is held by default.
- The cookie header travels from the native host to the running application
  over a named pipe that only the current Windows user can open. The native
  host checks that the program owning the pipe is the installed Download
  Manager before sending anything.
- It is never written to the database, a command line, a log, or the
  interface. The application holds it in memory, sends it only to the exact
  scheme, host and port it was captured for, and forgets it when the download
  finishes, is cancelled or removed, or the application closes.
- If the application cannot receive it, the handoff is refused and the
  browser keeps its own download.

Consequence of keeping it in memory only: a download that needed a login and
is resumed after Download Manager restarts no longer has the session, and the
site may refuse it. Start it again from the browser.

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
