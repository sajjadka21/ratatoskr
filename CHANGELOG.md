# Changelog

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
