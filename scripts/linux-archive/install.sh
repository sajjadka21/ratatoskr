#!/usr/bin/env bash
# Adds Ratatoskr from this folder to your applications menu, for you only
# (no administrator rights). The folder stays where it is; run uninstall.sh
# to remove the menu entry again. Your downloads and settings are kept in
# ~/.local/share/com.downloadmanager.desktop either way.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin"

missing=()
for library in libwebkit2gtk-4.1.so.0 libgtk-3.so.0 libayatana-appindicator3.so.1; do
  if ! ldconfig -p 2>/dev/null | grep -q "$library"; then
    missing+=("$library")
  fi
done
if [ "${#missing[@]}" -gt 0 ]; then
  echo "These system libraries were not found: ${missing[*]}"
  echo "Install them with your package manager, for example:"
  echo "  Debian/Ubuntu/Mint:  sudo apt install libwebkit2gtk-4.1-0 libayatana-appindicator3-1"
  echo "  Fedora:              sudo dnf install webkit2gtk4.1 libayatana-appindicator-gtk3"
  echo "  openSUSE:            sudo zypper install libwebkit2gtk-4_1-0 libayatana-appindicator3-1"
  echo "  Arch/Manjaro:        sudo pacman -S webkit2gtk-4.1 libayatana-appindicator"
  echo
fi

mkdir -p "$data/applications" "$bin"
for size in 32x32 128x128 256x256@2; do
  source="$here/share/icons/hicolor/$size/apps/ratatoskr.png"
  if [ -f "$source" ]; then
    install -D -m 0644 "$source" "$data/icons/hicolor/$size/apps/ratatoskr.png"
  fi
done

desktop="$data/applications/ratatoskr.desktop"
sed -e "s|^Exec=.*|Exec=\"$here/bin/ratatoskr\" %U|" \
    -e "s|^Icon=.*|Icon=ratatoskr|" \
    "$here/share/applications/Ratatoskr.desktop" > "$desktop"
chmod 0644 "$desktop"
ln -sfn "$here/bin/ratatoskr" "$bin/ratatoskr"

command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true

echo "Ratatoskr is in your applications menu. You can also start it with: ratatoskr"
command -v ffmpeg >/dev/null || echo "Tip: install ffmpeg as well, so videos with separate sound can be joined."
