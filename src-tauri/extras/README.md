Files shipped beside the application.

- `ratatosk-firefox.xpi` (optional): the browser extension signed by Mozilla.
  When present, Settings → Browser extension installs it in Firefox with one
  confirmation. See RELEASING.md for getting it signed (free).
- `yt-dlp.exe`: fetched by `scripts/fetch-ytdlp.ps1` (part of
  `scripts/release.cmd`) and checked against its published checksum. The app
  copies it into its data folder on first start and keeps that copy updated.

# YouTube JavaScript runtime

Windows releases also include Deno v2.9.8 (MIT), fetched and verified by
`scripts/fetch-deno.ps1`. Its licence is included as `DENO-LICENSE.txt`.
The pinned upstream archive SHA-256 is
`cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf`.
