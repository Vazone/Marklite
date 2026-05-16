param(
  [switch]$SkipTauriBuild
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\.."
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$tauriConfig = Get-Content -Raw (Join-Path $root "src-tauri\tauri.conf.json") | ConvertFrom-Json
$version = $tauriConfig.version

Push-Location $root
try {
  if (-not $SkipTauriBuild) {
    npm run tauri -- build --bundles nsis
  }

  $installerScript = Join-Path $root "src-tauri\target\release\nsis\x64\installer.nsi"
  & "$PSScriptRoot\patch-nsis-installer.ps1" -InstallerScript $installerScript

  $makensis = Join-Path $env:LOCALAPPDATA "tauri\NSIS\makensis.exe"
  if (-not (Test-Path $makensis)) {
    $makensis = Join-Path $env:LOCALAPPDATA "tauri\NSIS\Bin\makensis.exe"
  }
  if (-not (Test-Path $makensis)) {
    throw "makensis.exe not found. Run 'npm run tauri -- build --bundles nsis' once so Tauri can download NSIS."
  }

  $nsisDir = Split-Path $installerScript -Parent
  Push-Location $nsisDir
  try {
    & $makensis "installer.nsi"
  } finally {
    Pop-Location
  }

  $output = Join-Path $nsisDir "nsis-output.exe"
  if (-not (Test-Path $output)) {
    throw "NSIS output was not produced: $output"
  }

  $bundleDir = Join-Path $root "src-tauri\target\release\bundle\nsis"
  New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
  $final = Join-Path $bundleDir "MarkLite_$($version)_x64-setup.exe"
  Copy-Item -LiteralPath $output -Destination $final -Force
  Write-Host "Custom installer created: $final"
} finally {
  Pop-Location
}
