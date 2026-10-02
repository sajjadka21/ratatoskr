# Product checklist: Windows and Android

✅ done · 🟡 partly · ⬜ not done · 🔒 needs something only the owner can supply · ⛔ deliberately not planned

Last reviewed for version 1.2.0. Nothing here has been run on a real phone or a real Windows machine by the
people writing it; see "Verification" at the end.

## Downloading (both)

| | Windows | Android |
|---|---|---|
| Several connections per file, resume, crash recovery | ✅ | ✅ |
| Fast end of a download (the last part is shared) | ✅ | ✅ |
| Mirrors, refresh an expired link | ✅ | ⬜ |
| Speed limit | ✅ global, per download, per rule | ✅ global (presets) |
| Retry after a failure | ✅ backoff, 5 attempts | ✅ 3 attempts for network errors |
| Disk-space checks, safe file names, no cookies or passwords stored | ✅ | ✅ |
| Video sites (YouTube, Instagram, many more), quality choice | ✅ | ✅ |
| Spotify tracks (audio matched on YouTube) | ✅ | ✅ |
| HLS / DASH | ✅ | ✅ through yt-dlp |
| Site grabber, link collector | ✅ | ⬜ |
| Checksum check | ✅ | ✅ on request |
| Torrent, magnet, FTP | ⛔ | ⛔ |
| Saved site passwords, cookie import | ⛔ (project rule) | ⛔ |

## Organising and scheduling

| | Windows | Android |
|---|---|---|
| Queues, priorities | ✅ | ⛔ one list |
| Start at a date and time | ✅ queue or single download | ✅ per download |
| Daily window ("only between X and Y"), weekdays | ✅ | ✅ window |
| Action when finished (sleep, shut down…) | ✅ | ⬜ |
| Categories and folders | ✅ rules and categories | ✅ sub-folders and filter chips |
| Choose the save folder | ✅ | ✅ |
| Search, sort, multi-select, bulk actions | ✅ | ✅ |

## Getting links in

| | Windows | Android |
|---|---|---|
| Add many links, patterns like `file[01-20].jpg` | ✅ | ✅ |
| Clipboard detection | ✅ | ✅ |
| Share target / "Open with" | ✅ context menu | ✅ share sheet, file links |
| Browser extension | ✅ Chrome, Edge, Brave, Firefox | ⛔ |
| Floating drop box, command line | ✅ | ⬜ |
| Built-in browser with link sniffer | ⬜ | ⬜ |

## Look and feel

| | Windows | Android |
|---|---|---|
| Persian (right-to-left) and English | ✅ | ✅ |
| Four brand themes, light/dark | ✅ | ✅ |
| First-run guide | ✅ | ✅ |
| Details view (parts, resume support) | ✅ | ✅ sheet |
| Tray / background notification | ✅ closes to tray | ✅ foreground service |
| Finish sound, notifications | ✅ | ✅ |
| Keyboard shortcuts, command palette | ✅ | n/a |
| Screen-reader labels on icons | 🟡 | 🟡 |

## Customising

| | Windows | Android |
|---|---|---|
| Plugins (JSON rules: rewrite link, rename file, Referer/User-Agent) | ✅ | ✅ (no headers) |
| Portable edition | ✅ | n/a |
| Backup and restore | ✅ | ⬜ |

## Updates, size, compatibility

| | Windows | Android |
|---|---|---|
| Update that asks first, checked before install | ✅ signed | ✅ SHA-256 + Android confirmation |
| Size | ✅ LTO, strip, `opt-level="s"` | ✅ one APK per CPU, two languages |
| Oldest system | Windows 10 (Windows 7/8.1 impossible with current Rust and WebView2) | Android 8 |
| Installer language | ✅ English and Persian picker | n/a |
| Works without internet at install | ✅ web view runtime embedded | n/a |

## Quality, safety, support

| | Windows | Android |
|---|---|---|
| Automated tests in CI | ✅ Rust, frontend | ✅ unit tests (JVM and Robolectric) |
| UI tests on a device or emulator | ⬜ | ⬜ |
| Local-network (SSRF) protection, no telemetry, no accounts | ✅ | ✅ |
| Diagnostics for bug reports | ✅ report file | ✅ info share without links or names |
| Dependency audit, CodeQL | ✅ | ✅ |

## Owner tasks (cannot be done by code)

- 🔒 Authenticode certificate, to remove the Windows SmartScreen warning.
- 🔒 Android release keystore and the CI secrets (already required by the release workflow).
- 🔒 Browser-extension store submissions.
- 🔒 Real-device acceptance in `docs/mobile-acceptance.md`; until it passes, releases stay prereleases.
- 🔒 Screenshots of the Android app for the README (none can be taken in CI).

## Backlog, most useful first

1. Emulator UI smoke test in CI (Android) so a broken screen cannot reach a release.
2. Android: mirrors and "refresh link", completion action, backup of the list.
3. Android: built-in browser with a link sniffer.
4. Windows: Windows-on-ARM build.
5. Accessibility pass (focus order, contrast, screen-reader names) on both.

## Verification

CI proves the code compiles and the automated tests pass. It does not prove that downloads work end to end on a
phone, that the installer shows the Persian language, or that an update installs. Those are the first things to
check on a real device before calling a build final.
