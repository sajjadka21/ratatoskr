@echo off
rem Builds a signed release of Ratatosk and the latest.json that tells
rem installed copies about it. See RELEASING.md.
setlocal
cd /d "%~dp0\.."

if not exist ".signing\ratatosk-updater.key" (
  echo The signing key .signing\ratatosk-updater.key is missing.
  echo Without it, installed copies cannot verify the update. See RELEASING.md.
  exit /b 1
)
if "%~1"=="" (
  echo Usage: scripts\release.cmd https://github.com/OWNER/REPO/releases/download/vX.Y.Z
  exit /b 1
)

set "TAURI_SIGNING_PRIVATE_KEY_PATH=%CD%\.signing\ratatosk-updater.key"
set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD="

call npm install || exit /b 1
call npx tauri build --config src-tauri\tauri.release.json || exit /b 1
call node scripts\make-latest-json.mjs "%~1" || exit /b 1

echo.
echo Upload the installer and latest.json from target\release\bundle\nsis to the release.
endlocal
