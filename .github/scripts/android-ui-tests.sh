#!/usr/bin/env bash
set -uo pipefail
result=0
gradle --no-daemon :app:connectedDebugAndroidTest || result=$?
mkdir -p build/ui-screenshots
# Gradle uninstalls the test application; the shell-owned screenshots survive.
adb pull /sdcard/Download/ratatoskr-ui-review build/ui-screenshots/ || true
exit "$result"
