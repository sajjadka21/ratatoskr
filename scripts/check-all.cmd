@echo off
rem Runs every check the project uses and writes the output to
rem target\check-all.log. Double-click it or run scripts\check-all.cmd.
setlocal
cd /d "%~dp0.."
if not exist target mkdir target
set "LOG=%CD%\target\check-all.log"
echo Ratatosk checks started %DATE% %TIME% > "%LOG%"
echo.
echo  Ratatosk: running every check. This takes 5 to 15 minutes the first time.
echo  Please leave this window open until it says DONE.
echo  Full output: %LOG%
echo.
echo  [1/6] npm install...
echo. >> "%LOG%"
echo ===== npm install ===== >> "%LOG%"
call npm install >> "%LOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo EXIT npm-install %RC% >> "%LOG%"
if "%RC%"=="0" (echo         passed) else (echo         FAILED - see the log)
echo  [2/6] interface tests...
echo. >> "%LOG%"
echo ===== interface tests ===== >> "%LOG%"
call npx vitest run >> "%LOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo EXIT npm-test %RC% >> "%LOG%"
if "%RC%"=="0" (echo         passed) else (echo         FAILED - see the log)
echo  [3/6] interface build...
echo. >> "%LOG%"
echo ===== interface build ===== >> "%LOG%"
call npm run build >> "%LOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo EXIT npm-build %RC% >> "%LOG%"
if "%RC%"=="0" (echo         passed) else (echo         FAILED - see the log)
echo  [4/6] Rust formatting...
echo. >> "%LOG%"
echo ===== Rust formatting ===== >> "%LOG%"
cargo fmt --all --check >> "%LOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo EXIT cargo-fmt %RC% >> "%LOG%"
if "%RC%"=="0" (echo         passed) else (echo         FAILED - see the log)
echo  [5/6] clippy (the first run takes a few minutes)...
echo. >> "%LOG%"
echo ===== clippy (the first run takes a few minutes) ===== >> "%LOG%"
cargo clippy --workspace --all-targets -- -D warnings >> "%LOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo EXIT cargo-clippy %RC% >> "%LOG%"
if "%RC%"=="0" (echo         passed) else (echo         FAILED - see the log)
echo  [6/6] Rust tests (a few minutes)...
echo. >> "%LOG%"
echo ===== Rust tests (a few minutes) ===== >> "%LOG%"
cargo test --workspace >> "%LOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo EXIT cargo-test %RC% >> "%LOG%"
if "%RC%"=="0" (echo         passed) else (echo         FAILED - see the log)
echo. >> "%LOG%"
echo ALL DONE %DATE% %TIME% >> "%LOG%"
echo.
echo  DONE. You can close this window.
pause
