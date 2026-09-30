# Security policy

## Reporting a vulnerability

Please do **not** open a public issue. Use GitHub's private reporting:
**Security → Report a vulnerability** on this repository. Include the version,
what you did and what happened. You will get an answer as soon as possible;
please allow time for a fix before telling anyone else.

## What counts as official

- Only files attached to the **Releases** of `github.com/sajjadka21/ratatoskr`
  are official builds. Copies from anywhere else may be tampered with.
- Each release carries `SHA256SUMS.txt`; compare with
  `Get-FileHash <file> -Algorithm SHA256` (Windows) before installing.
- The Windows app installs an update only if its signature matches the public
  key built into the app (`plugins.updater.pubkey`). The private key is never
  in this repository.
- The Android APK is signed with the project's release key.

## Design choices that protect you

- Everything is local: no account, no telemetry, no server of ours.
- Cookies, credentials and tokens are never written to logs or the database.
- Links handed over by the browser extension are validated, and the native
  messaging host is registered only for the project's own extension.
- The bundled yt-dlp is checked against the checksum its release publishes
  when the installer is built.

## Supported versions

Only the latest release receives security fixes.
