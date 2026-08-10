[CmdletBinding()]
param(
  [switch]$SkipTauriBuild,
  [ValidateSet("x64")]
  [string]$Architecture = "x64",
  [string]$RootPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($RootPath)) {
  $RootPath = Join-Path $PSScriptRoot ".."
}

function Invoke-CheckedNative {
  param([string]$FilePath, [string[]]$Arguments, [string]$Description)

  & $FilePath @Arguments
  $exitCode = $LASTEXITCODE
  if ($null -eq $exitCode) {
    $exitCode = 0
  }
  if ($exitCode -ne 0) {
    throw "$Description failed with exit code $exitCode."
  }
}

function Assert-WorkspacePath {
  param([string]$Path, [string]$WorkspaceRoot)

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $rootPrefix = $WorkspaceRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
  if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to modify a packaging path outside the workspace: $fullPath"
  }
  return $fullPath
}

$root = (Resolve-Path -LiteralPath $RootPath).Path
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $root "src-tauri\tauri.conf.json") | ConvertFrom-Json
$version = [string]$tauriConfig.version
$artifact = Assert-WorkspacePath -Path (Join-Path $root "src-tauri\target\release\bundle\nsis\MarkLite_$($version)_$($Architecture)-setup.exe") -WorkspaceRoot $root
$outputs = @($artifact, "$artifact.sha256", "$artifact.release.json")
$windowsPowerShell = Join-Path $PSHOME "powershell.exe"
if (-not (Test-Path -LiteralPath $windowsPowerShell -PathType Leaf)) {
  throw "Windows PowerShell executable was not found: $windowsPowerShell"
}

$succeeded = $false
try {
  $buildArguments = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", (Join-Path $PSScriptRoot "build-windows-installer.ps1"),
    "-Architecture", $Architecture,
    "-RootPath", $root,
    "-SkipEvidence"
  )
  if ($SkipTauriBuild) {
    $buildArguments += "-SkipTauriBuild"
  }
  Invoke-CheckedNative -FilePath $windowsPowerShell -Arguments $buildArguments -Description "Windows installer build"

  $buildMode = $(if ($SkipTauriBuild) { "skip-tauri-build" } else { "full" })
  Invoke-CheckedNative -FilePath $windowsPowerShell -Arguments @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", (Join-Path $PSScriptRoot "write-windows-release-evidence.ps1"),
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
