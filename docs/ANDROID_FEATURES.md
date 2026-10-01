# Ratatoskr Android vs. Advanced Download Manager (ADM)

ADM is the reference for what people expect from an Android download manager. This table is
written from ADM's publicly known feature set (the app itself was not run here) and from this
repository's code. ✅ done · 🟡 partly · ⬜ not yet.

| Feature | ADM | Ratatoskr Android |
|---|---|---|
| Several connections per file (speed-up) | up to 16 | ✅ 1–8, journaled per segment, safe fallback to one stream |
| Resume after a break / reboot | ✅ | ✅ (SQLite journal + per-segment journal) |
| Add many links at once | ✅ | ✅ paste a list, files and videos are routed automatically |
| Batch pattern `file[01-20].jpg` | ✅ | ✅ (up to 200 links) |
| Clipboard link detection | ✅ | ✅ one-tap banner on open, plus Quick Settings tile; can be switched off |
| Share-sheet target | ✅ | ✅ |
| Video sites (YouTube, Instagram, …) | via browser | ✅ built-in yt-dlp with quality choice |
| Spotify | ✗ | ✅ track → matching audio from YouTube (Spotify itself is DRM-protected) |
| Categories / folders per type | ✅ | ✅ Video, Music, Archives, Programs, Documents, Other (switchable) |
| Speed, size and time left per download | ✅ | ✅ |
| Pause / resume all, remove from list | ✅ | ✅ |
| Wi-Fi only, roaming rules | ✅ | ✅ |
| Speed limit | ✅ | ✅ (shared across a file's connections) |
| Themes, Persian and English | partly | ✅ four brand themes, RTL |
| Built-in browser with link sniffer | ✅ | ⬜ |
| Scheduler (start at a time) | ✅ | ⬜ |
| Rename / choose folder before download | ✅ | 🟡 rename only through the file name in the link |
| Checksum verification | ✅ | ⬜ |
| FTP, torrent/magnet | ✅ | ⬜ |
| Per-site login / cookies | ✅ | ⬜ on purpose (project rule: no stored credentials) |

Next candidates, in order of value: scheduler, rename/choose folder dialog, checksum, built-in browser.
