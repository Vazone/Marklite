[CmdletBinding()]
param(
  [switch]$SkipTauriBuild,
  [ValidateSet("x64")]
  [string]$Architecture = "x64",
  [string]$RootPath,
  [string]$BuildScriptPath,
  [string]$EvidenceScriptPath,
  [string]$WindowsPowerShellPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot "lib\WindowsPackaging.psm1") -Force

if ([string]::IsNullOrWhiteSpace($RootPath)) {
  $RootPath = Join-Path $PSScriptRoot ".."
}

$root = (Resolve-Path -LiteralPath $RootPath).Path
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $root "src-tauri\tauri.conf.json") | ConvertFrom-Json
$version = [string]$tauriConfig.version
$artifact = Assert-WorkspacePath -Path (Join-Path $root "src-tauri\target\release\bundle\nsis\MarkLite_$($version)_$($Architecture)-setup.exe") -WorkspaceRoot $root
$outputs = @($artifact, "$artifact.sha256", "$artifact.release.json")
if ([string]::IsNullOrWhiteSpace($BuildScriptPath)) {
  $BuildScriptPath = Join-Path $PSScriptRoot "build-windows-installer.ps1"
}
if ([string]::IsNullOrWhiteSpace($EvidenceScriptPath)) {
  $EvidenceScriptPath = Join-Path $PSScriptRoot "write-windows-release-evidence.ps1"
}
foreach ($scriptPath in @($BuildScriptPath, $EvidenceScriptPath)) {
  if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
    throw "Packaging stage script was not found: $scriptPath"
  }
}
if ([string]::IsNullOrWhiteSpace($WindowsPowerShellPath)) {
  $WindowsPowerShellPath = Resolve-WindowsPowerShellPath
}

$succeeded = $false
try {
  $buildArguments = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", $BuildScriptPath,
    "-Architecture", $Architecture,
    "-RootPath", $root,
    "-SkipEvidence"
  )
  if ($SkipTauriBuild) {
    $buildArguments += "-SkipTauriBuild"
  }
  Invoke-CheckedNative -FilePath $WindowsPowerShellPath -Arguments $buildArguments -Description "Windows installer build"

  $buildMode = $(if ($SkipTauriBuild) { "skip-tauri-build" } else { "full" })
  Invoke-CheckedNative -FilePath $WindowsPowerShellPath -Arguments @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", $EvidenceScriptPath,
    "-ArtifactPath", $artifact,
    "-Architecture", $Architecture,
    "-BuildMode", $buildMode,
    "-RootPath", $root
  ) -Description "Release evidence generation"

  $evidence = Get-Content -Raw -LiteralPath "$artifact.release.json" | ConvertFrom-Json
  $succeeded = $true
  Write-Host "Windows package complete: $($evidence.artifact.fileName)"
  Write-Host "SHA-256: $($evidence.artifact.sha256)"
  Write-Host "Authenticode status: $($evidence.signature.status)"
} finally {
  if (-not $succeeded) {
    foreach ($output in $outputs) {
      try {
        $validated = Assert-WorkspacePath -Path $output -WorkspaceRoot $root
        if (Test-Path -LiteralPath $validated -PathType Leaf) {
          Remove-Item -LiteralPath $validated -Force
        }
      } catch {
        Write-Warning "Failed to clean packaging output '$output': $($_.Exception.Message)"
      }
    }
  }
}
