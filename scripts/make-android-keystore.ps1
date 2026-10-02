<#
  Makes the Android release key once, outside the repository, and prints exactly what to put in GitHub.
  Run in PowerShell:   powershell -ExecutionPolicy Bypass -File scripts\make-android-keystore.ps1
  Nothing is sent anywhere. The passwords you type are used only to run keytool on this computer.
#>
param([string]$Folder = (Join-Path $HOME "RatatoskrSecrets"))

function Find-Keytool {
  $candidates = @()
  if ($env:JAVA_HOME) { $candidates += Join-Path $env:JAVA_HOME "bin\keytool.exe" }
  $candidates += "$env:ProgramFiles\Android\Android Studio\jbr\bin\keytool.exe"
  $candidates += Get-ChildItem "$env:ProgramFiles\Java","$env:ProgramFiles\Eclipse Adoptium","$env:ProgramFiles\Microsoft" -Recurse -Filter keytool.exe -ErrorAction SilentlyContinue | ForEach-Object FullName
  $found = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
  if ($found) { return $found }
  $onPath = Get-Command keytool -ErrorAction SilentlyContinue
  if ($onPath) { return $onPath.Source }
  throw "keytool was not found. Install Android Studio (it includes Java) or a JDK 17, then run this again."
}

function Read-Secret($prompt) {
  $secure = Read-Host $prompt -AsSecureString
  [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
}

$keytool = Find-Keytool
New-Item -ItemType Directory -Force $Folder | Out-Null
$store = Join-Path $Folder "ratatoskr-release.jks"
if (Test-Path $store) { throw "$store already exists. Do NOT replace it: a different key means installed apps can never be updated." }

$alias = "ratatoskr"
$password = Read-Secret "Choose a password for the key file (at least 8 characters; save it in your password manager)"
if ($password.Length -lt 8) { throw "The password is too short." }
if ($password -ne (Read-Secret "Type it again")) { throw "The two passwords differ." }

& $keytool -genkeypair -v -keystore $store -alias $alias -keyalg RSA -keysize 4096 -validity 36500 `
  -storepass $password -keypass $password -dname "CN=Ratatoskr, O=Ratatoskr, C=IR"
if ($LASTEXITCODE -ne 0) { throw "keytool failed." }

$b64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($store))
$b64File = Join-Path $Folder "ANDROID_KEYSTORE_BASE64.txt"
Set-Content -Path $b64File -Value $b64 -NoNewline

$listing = & $keytool -list -v -keystore $store -alias $alias -storepass $password
# The release check compares with apksigner's form: lower-case hex, no colons.
$sha = (($listing | Select-String "SHA256:").ToString().Split(":",2)[1].Trim() -replace ":","").ToLower()

Write-Host ""
Write-Host "Done. Your key file: $store" -ForegroundColor Green
Write-Host ""
Write-Host "1) BACK UP $store in two safe places now (losing it means the app can never be updated)."
Write-Host "2) In GitHub: repository > Settings > Secrets and variables > Actions > New repository secret:"
Write-Host "     ANDROID_KEYSTORE_BASE64   = the whole content of $b64File"
Write-Host "     ANDROID_KEYSTORE_PASSWORD = the password you chose"
Write-Host "     ANDROID_KEY_PASSWORD      = the same password"
Write-Host "     ANDROID_KEY_ALIAS         = $alias"
Write-Host "3) Same page, tab 'Variables' > New repository variable:"
Write-Host "     ANDROID_SIGNING_CERT_SHA256 = $sha"
Write-Host "4) Delete $b64File after pasting it into GitHub."
