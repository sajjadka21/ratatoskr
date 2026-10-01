# Changelog

## Unreleased

- Android: direct file downloads use up to 8 connections per file with a per-segment journal (resume after a break), falling back to a single stream when the server has no byte ranges or no file validator.
- Android: Spotify track links are saved as audio (matched on YouTube; Spotify itself is DRM-protected).
- Android: speed, size and time-left on every download, pause/resume all, remove from history, one-tap download for a copied link.
- Android: add many links at once (any pasted text, `file[01-20].jpg` patterns expand), files and videos routed automatically; copied links are offered on open (switchable); finished files are sorted into Video, Music, Archives… folders (switchable). See docs/ANDROID_FEATURES.md for the comparison with ADM.
- Android: one APK per CPU instead of a universal APK (about a third of the size); `Ratatoskr-android.apk` is now the arm64 build.

## 1.0.2

- Android: upgrade yt-dlp/FFmpeg integration to 0.18.1 and rebuild nested WebP libraries from official source for 16 KB memory pages.
- Recursively check all 64-bit native executables/libraries, including ZIP payloads, before publishing.
- Version 1.0.1 was cancelled before publication after the incompatible payloads were detected; its immutable tag is preserved.

## [1.0.1] — 2026-10-01

### Fixed
- Release publication no longer fails on nested Windows artifact folders.
- Android releases require a persistent production signing key and verify the expected certificate and package identity.
- The Windows updater signature, URLs and version numbers are verified before assets are published.
- The full CI checks gate release publication; SHA-256 checksums accompany every asset.

### Changed
- First public Windows installer, MSI, portable ZIP and Android APK release.
- Release limitations, installation instructions and Android debug-build migration are documented in [release notes](docs/releases/1.0.1.md).
