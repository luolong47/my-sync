Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$portableDir = Join-Path $repoRoot "dist-portable"
$releaseExe = Join-Path $repoRoot "src-tauri\target\release\my-sync.exe"
$portableExe = Join-Path $portableDir "My Sync.exe"

Push-Location $repoRoot
try {
  pnpm build
  cargo build --manifest-path src-tauri/Cargo.toml --release

  New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
  Copy-Item -Force $releaseExe $portableExe

  Write-Host "Portable executable ready:"
  Write-Host $portableExe
}
finally {
  Pop-Location
}
