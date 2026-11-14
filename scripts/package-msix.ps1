# Builds a signed MSIX for Microsoft Store submission.
#
# Identity values live in packaging/identity.json and can be overridden by
# parameters. They are not secrets: every MSIX ships them and the Store shows
# the publisher name publicly.
#
# Usage:
#   scripts/package-msix.ps1
#   scripts/package-msix.ps1 -Version 0.1.25.0
param(
  [string]$IdentityName = "",
  [string]$Publisher = "",
  [string]$PublisherDisplayName = "",
  [string]$Version = "",
  [ValidateSet("x64", "arm64")]
  [string]$Architecture = "x64",
  [string]$Description = "Transferencia P2P de archivos con códigos",
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..").Path
$identityFile = Join-Path $root "packaging\identity.json"
if (Test-Path $identityFile) {
  $identity = Get-Content $identityFile -Raw -Encoding UTF8 | ConvertFrom-Json
  if (-not $IdentityName) { $IdentityName = $identity.identityName }
  if (-not $Publisher) { $Publisher = $identity.publisher }
  if (-not $PublisherDisplayName) { $PublisherDisplayName = $identity.publisherDisplayName }
}
if (-not $IdentityName) { throw "Falta IdentityName: usa packaging/identity.json o el parámetro -IdentityName." }
if (-not $Publisher) { throw "Falta Publisher: usa packaging/identity.json o el parámetro -Publisher." }
if (-not $PublisherDisplayName) { $PublisherDisplayName = "PeerWay" }
$config = Get-Content (Join-Path $root "crates\desktop\src-tauri\tauri.conf.json") -Raw -Encoding UTF8 | ConvertFrom-Json
if (-not $Version) { $Version = "$($config.version).0" }
if ($Version -notmatch '^\d+\.\d+\.\d+\.\d+$') {
  throw "La versión MSIX debe tener cuatro partes (por ejemplo 0.1.25.0): $Version"
}
if ($Publisher -notmatch '^CN=') {
  throw "Publisher debe ser la identidad de Partner Center, con formato CN=...: $Publisher"
}

$sdk = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Directory -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -match '^10\.' } |
  Sort-Object { [version]$_.Name } -Descending |
  Select-Object -First 1
if (-not $sdk) { throw "No se encontró el Windows SDK (MakeAppx/signtool)." }
$makeappx = Join-Path $sdk.FullName "x64\makeappx.exe"
$signtool = Join-Path $sdk.FullName "x64\signtool.exe"
foreach ($tool in @($makeappx, $signtool)) {
  if (-not (Test-Path $tool)) { throw "Falta la herramienta: $tool" }
}

if (-not $SkipBuild) {
  Write-Host "Compilando la variante Store..." -ForegroundColor Cyan
  Push-Location $root
  try {
    & cargo build --release -p peerway-desktop --features store
    if ($LASTEXITCODE -ne 0) { throw "cargo build falló" }
  }
  finally {
    Pop-Location
  }
}

$exe = Join-Path $root "target\release\peerway-desktop.exe"
if (-not (Test-Path $exe)) { throw "No existe el binario: $exe" }

$outDir = Join-Path $root "dist"
$layout = Join-Path $outDir "msix-layout"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force (Join-Path $layout "Assets") | Out-Null
Copy-Item $exe $layout

$icons = Join-Path $root "crates\desktop\src-tauri\icons"
foreach ($asset in "StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png", "Square310x310Logo.png") {
  $source = Join-Path $icons $asset
  if (-not (Test-Path $source)) { throw "Falta el icono: $source" }
  Copy-Item $source (Join-Path $layout "Assets\$asset")
}

$manifest = Get-Content (Join-Path $root "packaging\AppxManifest.xml") -Raw -Encoding UTF8
$escapedName = [System.Security.SecurityElement]::Escape($PublisherDisplayName)
$escapedDescription = [System.Security.SecurityElement]::Escape($Description)
$manifest = $manifest.Replace("{{IDENTITY}}", $IdentityName)
$manifest = $manifest.Replace("{{PUBLISHER}}", $Publisher)
$manifest = $manifest.Replace("{{PUBLISHER_DISPLAY_NAME}}", $escapedName)
$manifest = $manifest.Replace("{{VERSION}}", $Version)
$manifest = $manifest.Replace("{{ARCH}}", $Architecture)
$manifest = $manifest.Replace("{{DESCRIPTION}}", $escapedDescription)
$manifestPath = Join-Path $layout "AppxManifest.xml"
Set-Content -Path $manifestPath -Value $manifest -Encoding UTF8
[xml](Get-Content $manifestPath -Raw) | Out-Null

$msix = Join-Path $outDir "PeerWay_${Version}_${Architecture}.msix"
Write-Host "Empaquetando $msix..." -ForegroundColor Cyan
& $makeappx pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "MakeAppx falló" }

# Partner Center requires the package signature subject to match Publisher.
$cert = Get-ChildItem Cert:\CurrentUser\My |
  Where-Object { $_.Subject -eq $Publisher -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date).AddDays(30) } |
  Select-Object -First 1
if (-not $cert) {
  Write-Host "Creando certificado de empaquetado para $Publisher..." -ForegroundColor Cyan
  $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Publisher `
    -FriendlyName "PeerWay MSIX" -CertStoreLocation "Cert:\CurrentUser\My" `
    -KeyUsage DigitalSignature -NotAfter (Get-Date).AddYears(3)
}

Write-Host "Firmando con $($cert.Thumbprint)..." -ForegroundColor Cyan
& $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $msix
if ($LASTEXITCODE -ne 0) { throw "signtool falló" }

Write-Host "MSIX listo: $msix" -ForegroundColor Green
Get-Item $msix | Select-Object FullName, Length, LastWriteTime
Write-Host "Certificado de prueba (solo local): $($cert.Thumbprint)"
