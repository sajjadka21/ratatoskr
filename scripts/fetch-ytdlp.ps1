# Downloads the latest official yt-dlp.exe into src-tauri\extras so the
# installer carries it, and keeps it only if it matches the checksum the
# release publishes. Uses Windows' own proxy settings.
$ErrorActionPreference = "Stop"
$base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download"
$extras = Join-Path $PSScriptRoot "..\src-tauri\extras"
$target = Join-Path $extras "yt-dlp.exe"
$partial = "$target.partial"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$sums = (Invoke-WebRequest -UseBasicParsing "$base/SHA2-256SUMS").Content
if ($sums -is [byte[]]) { $sums = [Text.Encoding]::UTF8.GetString($sums) }
$line = $sums -split "`n" | Where-Object { $_ -match "^\s*([0-9a-fA-F]{64})\s+\*?yt-dlp\.exe\s*$" } | Select-Object -First 1
if (-not $line) { throw "The release lists no checksum for yt-dlp.exe." }
$expected = ([regex]::Match($line, "[0-9a-fA-F]{64}")).Value.ToLower()

Invoke-WebRequest -UseBasicParsing "$base/yt-dlp.exe" -OutFile $partial
$actual = (Get-FileHash $partial -Algorithm SHA256).Hash.ToLower()
if ($actual -ne $expected) {
  Remove-Item $partial
  throw "The downloaded yt-dlp.exe does not match its published checksum."
}
Move-Item -Force $partial $target
Write-Host "yt-dlp.exe is ready in src-tauri\extras."
