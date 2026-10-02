# AB Download Manager and Ratatoskr

[AB Download Manager](https://github.com/amir1376/ab-download-manager) is open source (Apache-2.0). This note
records what was read there (the repository was cloned and its README, scheduler and downloader code
looked at) and what was done with it.

## What was reused

No code was copied. AB's Android app is about 160 Kotlin files built on Compose and Decompose over
shared modules; moving it in would add a large dependency graph and several MB to the APK, and it could not
share this project's Rust engine or its security rules. Two ideas were taken and written fresh:

| Idea in AB | Where it is here |
|---|---|
| Schedules are a set of weekdays plus a start and an end time of day (`ScheduleTimes`) | Desktop already had it for queues (days + daily window). Android now has a daily "only download between X and Y" window that may pass midnight (`Schedule.kt`), tested |
| Parts are split while downloading, only inside a "safe zone" already agreed with the connection (`PartSplitSupport`) | Desktop splits the tail segment; Android now lets a finished connection take half of the slowest remaining part (`SegmentedDownload.steal`), tested byte-for-byte |

If code from AB is ever copied, its Apache-2.0 notice must be kept and listed in `LICENSES.md`.

## Feature table

| | AB Download Manager | Ratatoskr |
|---|---|---|
| Multi-connection downloads, resume | ✅ | ✅ desktop and Android |
| Queues and schedulers | ✅ | ✅ queues, daily windows, weekdays, one-time date and time, per-download schedule (desktop, Android) |
| Browser extension | ✅ | ✅ Chrome, Edge, Brave, Firefox |
| HLS | ✅ | ✅ (HLS and DASH) |
| Video sites | via extension | ✅ yt-dlp built in, Instagram, YouTube, Spotify tracks via YouTube |
| Plugins | ✗ | ✅ JSON rule files, desktop and Android |
| Persian, right-to-left | partly (Crowdin) | ✅ |
| Platforms | Windows, Linux, macOS, Android 8+ | Windows 10+, Android 8+ |
| Telegram bot | ✗ | ✅ |

## Older systems

- **Android:** minimum is now Android 8 (API 26), the same as AB. On Android 8 and 9 files are written into
  the public Downloads folder (the app asks for storage permission once) instead of through Android 10's
  scoped storage; the exact network constraint of background jobs falls back to "any / unmetered".
- **Windows:** Windows 10 and later. Windows 7 and 8.1 are not possible: the current Rust standard library
  needs Windows 10, and Tauri 2's web view (WebView2) no longer supports them. Supporting them would mean a
  separate legacy build on an old, unmaintained toolchain, which is not planned.

## Size

- Desktop: release build uses LTO, `panic = "abort"`, symbol stripping and now `opt-level = "s"` (smaller
  code; downloads are limited by the network, not the CPU).
- Android: one APK per CPU instead of one universal APK (about a third of the size), only English and Persian
  strings from libraries. Most of the remaining weight is Python and FFmpeg inside yt-dlp.
