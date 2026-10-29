#!/usr/bin/env bash
# Builds the distribution-independent Linux archive from the .deb that
# `tauri build` made: the same files, laid out as bin/, lib/Ratatoskr/ and
# share/ so the app finds its resources wherever the folder is unpacked, plus
# install.sh / uninstall.sh for a per-user menu entry.
#
# A packaged binary records which package it came from, so the in-app
# updater knows what to replace. A copy in an archive has no package to
# replace, so it uses the unmarked build output; the app then announces new
# versions instead of installing one over itself.
#
# Usage: scripts/make-linux-archive.sh <version> [output-dir]
set -euo pipefail

version="${1:?version}"
out="${2:-target/linux-archive}"
root="$(cd "$(dirname "$0")/.." && pwd)"
arch="$(uname -m)"

deb_dir="$(find "$root/target/release/bundle/deb" -mindepth 1 -maxdepth 1 -type d -name "*_${version}_*" | head -n 1)"
test -d "$deb_dir/data/usr" || { echo "No unpacked .deb for $version under target/release/bundle/deb" >&2; exit 1; }

name="Ratatoskr-${version}-linux-${arch}"
stage="$out/$name"
rm -rf "$stage"
mkdir -p "$stage"
cp -a "$deb_dir/data/usr/." "$stage/"

test -x "$stage/bin/ratatoskr" || { echo "The package has no bin/ratatoskr" >&2; exit 1; }
# The bundler marks only its own copies; the build output keeps the
# "unknown" marker, which no packaged copy contains.
unpackaged="$root/target/release/ratatoskr"
test "$(grep -aoc '__TAURI_BUNDLE_TYPE_VAR_UNK' "$unpackaged")" = 1 || {
  echo "target/release/ratatoskr is not the unpackaged build" >&2
  exit 1
}
install -m 0755 "$unpackaged" "$stage/bin/ratatoskr"

cp "$root/LICENSE" "$stage/LICENSE"
cp "$root/scripts/linux-archive/install.sh" "$root/scripts/linux-archive/uninstall.sh" "$stage/"
cp "$root/scripts/linux-archive/README.txt" "$stage/README.txt"
chmod 0755 "$stage/install.sh" "$stage/uninstall.sh" "$stage/bin/ratatoskr"

tar -C "$out" --owner=0 --group=0 -czf "$out/$name.tar.gz" "$name"
echo "$out/$name.tar.gz"
