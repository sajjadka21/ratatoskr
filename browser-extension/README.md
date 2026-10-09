# Ratatoskr browser extension

One extension for Chrome, Edge, Brave (Manifest V3, service worker) and
Firefox 140+ (Firefox uses `background.scripts`). Its development IDs
are fixed: `ocefplbhcgfmihahfkaknodbdidflhle` in Chromium browsers (from the
`key` in the manifest) and `browser@ratatosk.app` in Firefox.

## Connecting to the app

Ratatosk registers the native messaging host (`dm-native-host.exe`) for the
current Windows user every time it starts, for Chrome,
Edge, Brave, Chromium and Firefox. No manual registry or manifest editing is
needed. Settings → Browser extension shows what is connected.

The installed app locates the host in its bundled resources; the portable
package keeps it beside `Ratatoskr.exe`. Until the extension is published in
the browsers' stores it is loaded from the `browser-extension` folder installed
with the app (Settings has buttons for the folder and each browser's extensions
page). Once published, put the store
addresses in `STORE_PAGES` in `src/components/settings/BrowserSection.tsx`.
The private key that fixes the Chromium ID is kept outside the repository
(`.signing/ratatosk-extension.pem`); keep it with the update signing key.

## What it does

- Right-click a link: **Download with Ratatosk**.
- In Chrome, Edge and Brave, right-click an image and choose **Download with
  Ratatoskr**. In the page that opens, send the original to Ratatoskr or choose
  PNG, JPEG or WebP. The browser asks for site access only when conversion is
  selected. Converted copies go to the browser's Downloads folder; very large
  images are rejected before transfer to keep the browser responsive.
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

## On video sites

Instagram and other yt-dlp media paths currently support publicly accessible
media. The optional direct-file cookie handover does not sign yt-dlp into an
Instagram account. Private/account-only posts and DRM are not advertised as supported.

## Store packages and connection checks

Run `node scripts/package-extension.mjs` from the repository root to create
separate Chrome, Edge and Firefox ZIPs under `target/extension`. These packages
are ready for submission; they are not approved or signed store downloads.
Store-assigned Chromium IDs must be allowed by the native host before publication.

For local testing, run `node scripts/package-extension.mjs --local-test`. Those
packages go under `target/extension` with a `-local-test` suffix and keep the
fixed development key so the installed app's native messaging host recognizes
the extension. Add `--out-dir <folder>` to choose another output folder.

The extension sends a passive one-minute connection check and never launches the
desktop app for that check. Settings marks a browser as recently contacted only
for 90 seconds after an identified native message. Missing or failed responses
remove the previous green state.

Firefox asks separately for optional technical data consent when the user presses
“Check desktop connection”. This sends only the browser family to the local app;
denial keeps anonymous native status checks available. Link/website-content
handover is declared in the Firefox manifest. Cookies remain optional and off by default.

On the sites Ratatosk downloads with yt-dlp (YouTube, Aparat, Vimeo and
others listed in `manifest.json`), a "Download with Ratatosk" button appears
over a video while the pointer is on it. It sends the page address (or, for a
plain video file, the file's address); the app then asks for the quality.

## Alt+click

With "Alt+click leaves a download to the browser" turned on in the options,
holding Alt while clicking a link lets the browser download it itself. This
registers a tiny script on every site (`alt.js`), so it asks for access to
all sites only when turned on and gives it back when turned off.
