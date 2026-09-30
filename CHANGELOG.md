# Changelog

## [1.0.1] — 2026-10-01

### Fixed
- Release publication no longer fails on nested Windows artifact folders.
- Android releases require a persistent production signing key and verify the expected certificate and package identity.
- The Windows updater signature, URLs and version numbers are verified before assets are published.
- The full CI checks gate release publication; SHA-256 checksums accompany every asset.

### Changed
- First public Windows installer, MSI, portable ZIP and Android APK release.
- Release limitations, installation instructions and Android debug-build migration are documented in [release notes](docs/releases/1.0.1.md).
