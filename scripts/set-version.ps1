# Sets the product version across the Rust workspace, Tauri and the web UI.
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..").Path

function Write-Utf8([string]$path, [string]$content) {
  [IO.File]::WriteAllText($path, $content, (New-Object Text.UTF8Encoding($false)))
}

$cargoPath = Join-Path $root "Cargo.toml"
$cargo = Get-Content -Raw -Encoding UTF8 $cargoPath
$start = $cargo.IndexOf("[workspace.package]")
if ($start -lt 0) { throw "No se encontró [workspace.package] en Cargo.toml" }
$next = $cargo.IndexOf("`n[", $start + 1)
$end = if ($next -ge 0) { $next } else { $cargo.Length }
$block = $cargo.Substring($start, $end - $start)
$block = [regex]::Replace($block, '(?m)^\s*version\s*=\s*"[^"]+"', "version = `"$Version`"")
$cargo = $cargo.Substring(0, $start) + $block + $cargo.Substring($end)
Write-Utf8 $cargoPath $cargo

$tauriPath = Join-Path $root "crates\desktop\src-tauri\tauri.conf.json"
$tauri = Get-Content -Raw -Encoding UTF8 $tauriPath
$tauri = [regex]::Replace($tauri, '("version"\s*:\s*")[^"]+(")', '${1}' + $Version + '${2}')
Write-Utf8 $tauriPath $tauri

Push-Location (Join-Path $root "crates\desktop")
try {
  npm version $Version --no-git-tag-version --allow-same-version | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "npm version falló" }
}
finally {
  Pop-Location
}

$webPath = Join-Path $root "web\index.html"
$web = Get-Content -Raw -Encoding UTF8 $webPath
$web = [regex]::Replace($web, 'id="versionText">[^<]+<', "id=`"versionText`">$Version<")
$web = [regex]::Replace($web, 'id="windowVersion">[^<]+<', "id=`"windowVersion`">$Version<")
$web = [regex]::Replace($web, 'id="aboutVersion">[^<]+<', "id=`"aboutVersion`">$Version<")
Write-Utf8 $webPath $web

$appPath = Join-Path $root "web\app.js"
$app = Get-Content -Raw -Encoding UTF8 $appPath
$app = [regex]::Replace($app, "\|\| '[0-9]+\.[0-9]+\.[0-9]+'", "|| '$Version'")
Write-Utf8 $appPath $app

Write-Host "Versión $Version aplicada. Compila con scripts/package-desktop.ps1 -Version $Version." -ForegroundColor Green
