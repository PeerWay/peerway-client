param(
  [Parameter(Mandatory = $true)][string]$File
)

# Authenticode signing hook for tauri build (bundle.windows.signCommand).
# Without a certificate it is a no-op so local builds still work.
# Configure one of:
#   PEERWAY_SIGN_CERT_THUMBPRINT  SHA-1 thumbprint of a code-signing certificate in the Windows store.
#   PEERWAY_SIGN_CERT_PATH        Path to a .pfx file (set PEERWAY_SIGN_CERT_PASSWORD too).
$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $File)) {
  throw "No existe el archivo a firmar: $File"
}

$thumbprint = $env:PEERWAY_SIGN_CERT_THUMBPRINT
$pfx = $env:PEERWAY_SIGN_CERT_PATH
if (-not $thumbprint -and -not $pfx) {
  Write-Host "sign-windows: sin certificado configurado; $File queda sin firmar."
  exit 0
}

$certificate = $null
if ($pfx) {
  $password = $env:PEERWAY_SIGN_CERT_PASSWORD
  if ($password) {
    $secure = ConvertTo-SecureString $password -AsPlainText -Force
    $certificate = Get-PfxCertificate -FilePath $pfx -Password $secure
  } else {
    $certificate = Get-PfxCertificate -FilePath $pfx
  }
} else {
  $certificate = Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My -ErrorAction SilentlyContinue |
    Where-Object { $_.Thumbprint -eq $thumbprint -and $_.HasPrivateKey } |
    Select-Object -First 1
}

if (-not $certificate) {
  throw "No se encontró el certificado de firma configurado."
}

$result = Set-AuthenticodeSignature -FilePath $File -Certificate $certificate `
  -HashAlgorithm SHA256 -TimestampServer "http://timestamp.digicert.com"
if ($result.Status -ne "Valid") {
  throw "La firma de $File no es válida: $($result.StatusMessage)"
}
Write-Host "sign-windows: firmado $File"
