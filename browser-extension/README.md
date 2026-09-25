# Ratatosk browser extension

One extension for Chrome, Edge, Brave (Manifest V3, service worker) and
Firefox 121+ (the same manifest; Firefox uses `background.scripts`). Its IDs
are fixed: `ocefplbhcgfmihahfkaknodbdidflhle` in Chromium browsers (from the
`key` in the manifest) and `browser@ratatosk.app` in Firefox.

## Connecting to the app

Ratatosk registers the native messaging host (`dm-native-host.exe`, installed
next to it) for the current Windows user every time it starts, for Chrome,
Edge, Brave, Chromium and Firefox. No manual registry or manifest editing is
needed. Settings → Browser extension shows what is connected.

Until the extension is published in the browsers' stores it is loaded from the
`browser-extension` folder installed with the app (Settings has buttons for
the folder and each browser's extensions page). Once published, put the store
addresses in `STORE_PAGES` in `src/components/settings/BrowserSection.tsx`.
The private key that fixes the Chromium ID is kept outside the repository
(`.signing/ratatosk-extension.pem`); keep it with the update signing key.

## What it does

- Right-click a link: **Download with Ratatosk**.
- Right-click selected text: its links go to the link collector.
- On YouTube, Aparat, Instagram and similar sites: **Download this video with
  Ratatosk** (downloaded with yt-dlp).
- Optionally, take over downloads the browser starts (off by default), with a
  size threshold and site/type exceptions.

It stores only these settings, in the browser's local storage. It never stores
cookies, passwords, authorization headers or access tokens.

## Browser login handover (optional, off by default)

Some downloads only work while signed in. With "Hand over my browser login"
turned on, the extension reads the cookie header the browser would send to
that one download URL and passes it to Ratatosk.

- Turning it on asks the browser for the `cookies` permission and access to
  sites; turning it off gives both back. Neither is held by default.
- The cookie header travels from the native host to the running application
  over a named pipe that only the current Windows user can open. The native
  host checks that the program owning the pipe is the installed Ratatosk
  before sending anything.
- It is never written to the database, a command line, a log, or the
  interface. The application holds it in memory, sends it only to the exact
  scheme, host and port it was captured for, and forgets it when the download
  finishes, is cancelled or removed, or the application closes.
- If the application cannot receive it, the handoff is refused and the
  browser keeps its own download.
