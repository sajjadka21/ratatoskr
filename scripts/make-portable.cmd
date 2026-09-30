@echo off
rem Builds the portable version of Ratatosk: a folder (and zip) that runs
rem from anywhere, keeps its data beside itself and leaves nothing in the
rem user's profile. See RELEASING.md.
setlocal
cd /d "%~dp0\.."

call npm install || exit /b 1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-ytdlp.ps1 || exit /b 1
call npx tauri build --no-bundle --config src-tauri\tauri.release.json || exit /b 1

set "OUT=target\portable\Ratatoskr-portable"
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%" || exit /b 1

copy /y target\release\tauri-app.exe "%OUT%\Ratatoskr.exe" >nul || exit /b 1
copy /y target\release\dm-native-host.exe "%OUT%\" >nul || exit /b 1
copy /y target\release\tosk.exe "%OUT%\" >nul || exit /b 1
xcopy /e /i /y browser-extension "%OUT%\browser-extension" >nul || exit /b 1
xcopy /e /i /y src-tauri\extras "%OUT%\extras" >nul || exit /b 1

rem The marker file is what makes it portable.
echo This file makes Ratatosk keep its data in the "data" folder beside it.> "%OUT%\portable.txt"

powershell -NoProfile -Command "Compress-Archive -Force -Path '%OUT%' -DestinationPath 'target\portable\Ratatoskr-portable.zip'" || exit /b 1
echo.
echo Portable version: target\portable\Ratatoskr-portable.zip
endlocal
