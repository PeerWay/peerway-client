# Builds the PeerWay Windows desktop installer (Tauri + WebView2) and copies it
# to dist/ for publishing.
param(
  [string]$Version = "0.1.1",
  [string]$ServerUrl = "https://api.peerway.io",
  [ValidateSet("msi", "nsis", "both")]
  [string]$Bundle = "both"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..").Path
$configVersion = (Get-Content (Join-Path $root "crates\desktop\src-tauri\tauri.conf.json") -Raw -Encoding UTF8 | ConvertFrom-Json).version
if ($Version -ne $configVersion) {
  throw "La versión del instalador ($Version) no coincide con tauri.conf.json ($configVersion). Ejecuta scripts/set-version.ps1 primero."
}
$env:PEERWAY_DEFAULT_SERVER_URL = $ServerUrl
$env:PEERWAY_DEFAULT_INSECURE_TLS = "0"

# Updater artifacts must be signed with the release key. The private key is
# intentionally kept outside the repository.
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  $signingKey = Join-Path $env:USERPROFILE ".tauri\peerway-updater.key"
  if (Test-Path $signingKey) {
    $env:TAURI_SIGNING_PRIVATE_KEY = $signingKey
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -and $env:tauri_pass) {
      $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $env:tauri_pass
    }
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
      $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
    }
  } else {
    throw "Falta la clave de actualizaciones: $signingKey. No publiques un instalador sin firma."
  }
}

Push-Location (Join-Path $root "crates\desktop")
try {
  if (-not (Test-Path "node_modules\.bin\tauri.cmd")) {
    $ErrorActionPreference = "Continue"
    npm install
    $ErrorActionPreference = "Stop"
  }
  $bundlesArg = if ($Bundle -eq "both") { "msi,nsis" } else { $Bundle }
  $ErrorActionPreference = "Continue"
  .\node_modules\.bin\tauri.cmd build --bundles $bundlesArg
  $buildExit = $LASTEXITCODE
  $ErrorActionPreference = "Stop"
  if ($buildExit -ne 0) { throw "tauri build falló" }

  $downloads = Join-Path $root "dist"
  New-Item -ItemType Directory -Force $downloads | Out-Null

  if ($Bundle -in @("msi", "both")) {
    $msi = Get-ChildItem (Join-Path $root "target\release\bundle\msi") -Filter "PeerWay_${Version}_x64_*.msi" -File |
      Select-Object -First 1
    if (-not $msi) {
      throw "No se encontró el MSI de $Version."
    }
    $dest = Join-Path $downloads "PeerWay-Setup-$Version.msi"
    Copy-Item -Force $msi.FullName $dest
    Write-Host "Instalador MSI: $dest" -ForegroundColor Green
    Get-Item $dest | Select-Object Name, Length, LastWriteTime
  }

  if ($Bundle -in @("nsis", "both")) {
    $setup = Join-Path $root "target\release\bundle\nsis\PeerWay_$Version`_x64-setup.exe"
    if (-not (Test-Path $setup)) {
      throw "No se encontró el instalador: $setup"
    }
    $dest = Join-Path $downloads "PeerWay-Setup-$Version.exe"
    Copy-Item -Force $setup $dest
    Write-Host "Instalador NSIS: $dest" -ForegroundColor Green
    Get-Item $dest | Select-Object Name, Length, LastWriteTime
  }
}
finally {
  Pop-Location
}
