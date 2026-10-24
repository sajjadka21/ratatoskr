# yt-dlp's official EJS solver requires an external JavaScript runtime.
# Ship a pinned, checksum-verified Deno, so users need no global installation.
$ErrorActionPreference = 'Stop'
$version = 'v2.9.8'
$expected = 'cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf'
$extras = Join-Path $PSScriptRoot '..\src-tauri\extras'
$archive = Join-Path $extras 'deno.zip.partial'
Invoke-WebRequest "https://github.com/denoland/deno/releases/download/$version/deno-x86_64-pc-windows-msvc.zip" -OutFile $archive
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw 'The Deno archive failed SHA-256 verification.'
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    $entry = $zip.GetEntry('deno.exe')
    if (-not $entry) { throw 'The verified archive contains no deno.exe.' }
    [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $extras 'deno.exe.partial'), $true)
} finally { $zip.Dispose() }
Move-Item -LiteralPath (Join-Path $extras 'deno.exe.partial') -Destination (Join-Path $extras 'deno.exe') -Force
Invoke-WebRequest "https://raw.githubusercontent.com/denoland/deno/$version/LICENSE.md" -OutFile (Join-Path $extras 'DENO-LICENSE.txt')
Remove-Item -LiteralPath $archive
Write-Host "Verified Deno $version is ready in src-tauri/extras."
