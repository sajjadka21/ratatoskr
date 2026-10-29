Files shipped beside the Linux application (installed as its `extras`
resources, like `src-tauri/extras` on Windows).

- `yt-dlp`: the official standalone Linux build, fetched by
  `scripts/fetch-linux-tools.sh` and checked against the release's published
  checksum. The app copies it into its data folder on first start and keeps
  that copy updated.
- `deno` (x86_64 only): Deno v2.9.8 (MIT), yt-dlp's JavaScript runtime for
  YouTube, checked against a pinned SHA-256
  (`e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609`).
  Its licence is included as `DENO-LICENSE.txt`.

FFmpeg is not bundled on Linux: the packages recommend the distribution's own
`ffmpeg`, which the app finds on `PATH`.
