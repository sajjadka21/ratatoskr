Ratatoskr for Linux (any distribution)
======================================

This archive runs on any 64-bit Linux desktop with WebKitGTK 4.1, for
example Debian 12+, Ubuntu 22.04+, Linux Mint 21+, Fedora 37+, openSUSE
Tumbleweed/Leap 15.5+, Arch, Manjaro and their derivatives.

1. Unpack the archive wherever you like (for example ~/Applications).
2. Run ./install.sh to add Ratatoskr to your applications menu.
   Or start it directly: ./bin/ratatoskr

Needed system libraries (usually installed already on a desktop):
  Debian/Ubuntu/Mint:  sudo apt install libwebkit2gtk-4.1-0 libayatana-appindicator3-1
  Fedora:              sudo dnf install webkit2gtk4.1 libayatana-appindicator-gtk3
  openSUSE:            sudo zypper install libwebkit2gtk-4_1-0 libayatana-appindicator3-1
  Arch/Manjaro:        sudo pacman -S webkit2gtk-4.1 libayatana-appindicator
Recommended: ffmpeg, to join video and sound downloaded separately.

Your downloads list and settings live in
~/.local/share/com.downloadmanager.desktop, so replacing this folder with a
newer version keeps them. This copy tells you when a new version is out; it
does not replace itself. The .deb and .rpm packages update from inside the app.

To remove: run ./uninstall.sh, then delete this folder.
