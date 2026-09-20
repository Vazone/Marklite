[CmdletBinding()]
param(
  [switch]$SkipTauriBuild,
  [switch]$SkipEvidence,
  [ValidateSet("x64")]
  [string]$Architecture = "x64",
  [string]$RootPath,
  [string]$NpmCommand = "npm",
  [string]$MakeNsisPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Import-Module (Join-Path $PSScriptRoot "lib\WindowsPackaging.psm1") -Force

if ([string]::IsNullOrWhiteSpace($RootPath)) {
  $RootPath = Join-Path $PSScriptRoot ".."
}

$root = (Resolve-Path -LiteralPath $RootPath).Path
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$env:Path = "$cargoBin;$env:Path"
$tauriConfigPath = Join-Path $root "src-tauri\tauri.conf.json"
$tauriConfig = Get-Content -Raw -LiteralPath $tauriConfigPath | ConvertFrom-Json
$version = [string]$tauriConfig.version
if ([string]::IsNullOrWhiteSpace($version)) {
  throw "Tauri configuration does not contain a version."
}

$nsisDir = Join-Path $root "src-tauri\target\release\nsis\$Architecture"
$installerScript = Join-Path $nsisDir "installer.nsi"
$nsisOutput = Join-Path $nsisDir "nsis-output.exe"
$bundleDir = Join-Path $root "src-tauri\target\release\bundle\nsis"
$artifactName = "MarkLite_$($version)_$($Architecture)-setup.exe"
$final = Join-Path $bundleDir $artifactName
$checksumPath = "$final.sha256"
$evidencePath = "$final.release.json"

$installerScript = Assert-WorkspacePath -Path $installerScript -WorkspaceRoot $root
$nsisOutput = Assert-WorkspacePath -Path $nsisOutput -WorkspaceRoot $root
$final = Assert-WorkspacePath -Path $final -WorkspaceRoot $root
$checksumPath = Assert-WorkspacePath -Path $checksumPath -WorkspaceRoot $root
$evidencePath = Assert-WorkspacePath -Path $evidencePath -WorkspaceRoot $root
$packagingSucceeded = $false

Push-Location $root
try {
  Remove-WorkspaceFileIfPresent -Path $nsisOutput -WorkspaceRoot $root
  Remove-WorkspaceFileIfPresent -Path $final -WorkspaceRoot $root
  Remove-WorkspaceFileIfPresent -Path $checksumPath -WorkspaceRoot $root
  Remove-WorkspaceFileIfPresent -Path $evidencePath -WorkspaceRoot $root

  if (-not $SkipTauriBuild) {
    Remove-WorkspaceFileIfPresent -Path $installerScript -WorkspaceRoot $root
    Invoke-CheckedNative -FilePath $NpmCommand -Arguments @("run", "tauri", "--", "build", "--bundles", "nsis") -Description "Tauri build"
  }

  if (-not (Test-Path -LiteralPath $installerScript -PathType Leaf)) {
    throw "Tauri did not produce the expected $Architecture NSIS script: $installerScript"
  }
  if ((Get-Item -LiteralPath $installerScript).Length -le 0) {
    throw "Tauri produced an empty NSIS script: $installerScript"
  }
  if (-not $SkipTauriBuild) {
    $generatedInstaller = Get-Content -Raw -LiteralPath $installerScript
    if (-not $generatedInstaller.Contains('/oname=marklite-cli.exe') -or
        -not $generatedInstaller.Contains('Delete "$INSTDIR\marklite-cli.exe"')) {
      throw 'Tauri NSIS output does not install and remove marklite-cli.exe beside the main binary.'
    }
  }

  & (Join-Path $PSScriptRoot "patch-nsis-installer.ps1") -InstallerScript $installerScript

  if ([string]::IsNullOrWhiteSpace($MakeNsisPath)) {
    $MakeNsisPath = Join-Path $env:LOCALAPPDATA "tauri\NSIS\makensis.exe"
    if (-not (Test-Path -LiteralPath $MakeNsisPath -PathType Leaf)) {
      $MakeNsisPath = Join-Path $env:LOCALAPPDATA "tauri\NSIS\Bin\makensis.exe"
    }
  }
  if (-not (Test-Path -LiteralPath $MakeNsisPath -PathType Leaf)) {
    throw "makensis.exe not found. Run 'npm run tauri -- build --bundles nsis' once so Tauri can download NSIS."
  }
  $MakeNsisPath = (Resolve-Path -LiteralPath $MakeNsisPath).Path

  Push-Location $nsisDir
  try {
    Invoke-CheckedNative -FilePath $MakeNsisPath -Arguments @("installer.nsi") -Description "makensis"
  } finally {
    Pop-Location
  }

  if (-not (Test-Path -LiteralPath $nsisOutput -PathType Leaf)) {
    throw "NSIS output was not produced by this invocation: $nsisOutput"
  }
  $outputItem = Get-Item -LiteralPath $nsisOutput
  if ($outputItem.Length -le 0) {
    throw "NSIS produced an empty installer: $nsisOutput"
  }

  New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
  $candidate = Assert-WorkspacePath -Path (Join-Path $bundleDir ".$artifactName.candidate-$([guid]::NewGuid().ToString('N'))") -WorkspaceRoot $root
  try {
    Copy-Item -LiteralPath $nsisOutput -Destination $candidate
    $sourceHash = Get-Sha256Hex -Path $nsisOutput
    $candidateHash = Get-Sha256Hex -Path $candidate
    if ($sourceHash -ne $candidateHash) {
      throw "Candidate installer hash does not match the fresh NSIS output."
    }
    # Tauri creates its default bundle before the custom patched makensis pass.
    # Remove that exact file only after the custom output and candidate hashes agree.
    Remove-WorkspaceFileIfPresent -Path $final -WorkspaceRoot $root
    Move-Item -LiteralPath $candidate -Destination $final
  } finally {
    if (Test-Path -LiteralPath $candidate) {
      Remove-Item -LiteralPath $candidate -Force
    }
  }

  $finalHash = Get-Sha256Hex -Path $final
  if ($finalHash -ne $sourceHash) {
    throw "Final installer hash does not match the fresh NSIS output."
  }

  if (-not $SkipEvidence) {
    $buildMode = $(if ($SkipTauriBuild) { "skip-tauri-build" } else { "full" })
    $windowsPowerShell = Resolve-WindowsPowerShellPath
    Invoke-CheckedNative -FilePath $windowsPowerShell -Arguments @(
      "-NoProfile",
      "-ExecutionPolicy", "Bypass",
      "-File", (Join-Path $PSScriptRoot "write-windows-release-evidence.ps1"),
      "-ArtifactPath", $final,
      "-Architecture", $Architecture,
      "-BuildMode", $buildMode,
      "-RootPath", $root
    ) -Description "Release evidence generation"
  }

  $packagingSucceeded = $true

  Write-Host "Custom installer created: $final"
  if ($SkipEvidence) {
    Write-Host "Release evidence deferred to the package wrapper."
  } else {
    $releaseEvidence = Get-Content -Raw -LiteralPath $evidencePath | ConvertFrom-Json
    Write-Host "Verified release evidence for $($releaseEvidence.artifact.fileName)."
  }
} finally {
  if (-not $packagingSucceeded) {
    foreach ($failedOutput in @($final, $checksumPath, $evidencePath)) {
      try {
        Remove-WorkspaceFileIfPresent -Path $failedOutput -WorkspaceRoot $root
      } catch {
        Write-Warning "Failed to clean packaging output '$failedOutput': $($_.Exception.Message)"
      }
    }
  }
  Pop-Location
}
