#!/usr/bin/env bash
# Fetches the helper programs the Linux packages carry into
# src-tauri/extras-linux, and keeps each one only if it matches its
# published checksum:
#   - yt-dlp: the official standalone Linux build (no Python needed),
#     checked against the release's SHA2-256SUMS;
#   - Deno: yt-dlp's JavaScript runtime for YouTube, pinned to the same
#     version as the Windows build, checked against a pinned SHA-256.
set -euo pipefail

DENO_VERSION="v2.9.8"
DENO_SHA256="e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609"
YTDLP_BASE="https://github.com/yt-dlp/yt-dlp/releases/latest/download"

extras="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/extras-linux"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

case "$(uname -m)" in
  x86_64) ytdlp_asset="yt-dlp_linux"; deno_asset="deno-x86_64-unknown-linux-gnu.zip" ;;
  aarch64) ytdlp_asset="yt-dlp_linux_aarch64"; deno_asset="" ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

curl -fsSL "$YTDLP_BASE/SHA2-256SUMS" -o "$work/SHA2-256SUMS"
expected="$(awk -v name="$ytdlp_asset" '$2 == name || $2 == "*" name { print tolower($1) }' "$work/SHA2-256SUMS")"
test -n "$expected" || { echo "The release lists no checksum for $ytdlp_asset." >&2; exit 1; }
curl -fsSL "$YTDLP_BASE/$ytdlp_asset" -o "$work/yt-dlp"
echo "$expected  $work/yt-dlp" | sha256sum --check --quiet
install -D -m 0755 "$work/yt-dlp" "$extras/yt-dlp"

if [ -n "$deno_asset" ]; then
  curl -fsSL "https://github.com/denoland/deno/releases/download/$DENO_VERSION/$deno_asset" -o "$work/deno.zip"
  echo "$DENO_SHA256  $work/deno.zip" | sha256sum --check --quiet
  if command -v unzip > /dev/null; then
    unzip -q -o "$work/deno.zip" deno -d "$work"
  else
    python3 -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extract("deno", sys.argv[2])' "$work/deno.zip" "$work"
  fi
  install -D -m 0755 "$work/deno" "$extras/deno"
  curl -fsSL "https://raw.githubusercontent.com/denoland/deno/$DENO_VERSION/LICENSE.md" -o "$extras/DENO-LICENSE.txt"
fi

echo "Verified Linux helper programs are ready in src-tauri/extras-linux."
