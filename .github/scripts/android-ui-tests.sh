#!/usr/bin/env bash
set -uo pipefail
result=0
# Match the approved 400dp-wide reference; device chrome remains native.
adb shell wm size 1200x2562
adb shell wm density 480
gradle --no-daemon :app:connectedDebugAndroidTest || result=$?
mkdir -p build/ui-screenshots
# Gradle uninstalls the test application; the shell-owned screenshots survive.
adb pull /sdcard/Download/ratatoskr-ui-review build/ui-screenshots/ || true
if [ "$result" -eq 0 ] && [ "$(find build/ui-screenshots -name '*.png' | wc -l)" -lt 8 ]; then
  echo "::error::Expected eight native UI screenshots"
  exit 1
fi
exit "$result"
