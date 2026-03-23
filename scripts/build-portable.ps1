Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$portableDir = Join-Path $repoRoot "dist\release"
$releaseExe = Join-Path $repoRoot "src-tauri\target\release\my-sync.exe"
$portableExe = Join-Path $portableDir "my-sync.exe"

Push-Location $repoRoot
try {
  # 使用 tauri build --no-bundle 替代 cargo build，确保资源正确嵌入。
  # 它会自动运行 tauri.conf.json 中的 beforeBuildCommand (pnpm build:web)
  pnpm tauri build --no-bundle

  New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
  Get-ChildItem -Path $portableDir -File -ErrorAction SilentlyContinue | Remove-Item -Force
  Copy-Item -Force $releaseExe $portableExe

  Write-Host "Portable executable ready:"
  Write-Host $portableExe
}
finally {
  Pop-Location
}
