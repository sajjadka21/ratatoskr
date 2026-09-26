Files shipped beside the application.

- `ratatosk-firefox.xpi` (optional): the browser extension signed by Mozilla.
  When present, Settings → Browser extension installs it in Firefox with one
  confirmation. See RELEASING.md for getting it signed (free).
- `yt-dlp.exe`: fetched by `scripts/fetch-ytdlp.ps1` (part of
  `scripts/release.cmd`) and checked against its published checksum. The app
  copies it into its data folder on first start and keeps that copy updated.
