#!/usr/bin/env bash
# Removes the menu entry, icon and command that install.sh added. This folder,
# your downloads and your settings are left alone.
set -euo pipefail

data="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$data/applications/ratatoskr.desktop" "$HOME/.local/bin/ratatoskr"
rm -f "$data"/icons/hicolor/*/apps/ratatoskr.png
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/ratatoskr.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
echo "Ratatoskr's menu entry was removed. Delete this folder to remove the program itself."
