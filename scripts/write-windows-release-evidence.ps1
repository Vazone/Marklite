[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$ArtifactPath,
  [ValidateSet("x64")]
  [string]$Architecture = "x64",
  [ValidateSet("full", "skip-tauri-build", "post-sign")]
  [string]$BuildMode = "full",
  [string]$RootPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RootPath)) {
  $RootPath = Join-Path $PSScriptRoot ".."
}

function Assert-WorkspacePath {
  param([string]$Path, [string]$WorkspaceRoot)

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $rootPrefix = $WorkspaceRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
  if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Release evidence path is outside the workspace: $fullPath"
  }
  return $fullPath
}

function Get-Sha256Hex {
  param([string]$Path)

  $stream = [System.IO.File]::OpenRead($Path)
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = $sha256.ComputeHash($stream)
    return (($bytes | ForEach-Object { $_.ToString("x2") }) -join "")
  } finally {
    $sha256.Dispose()
    $stream.Dispose()
  }
}

function Get-OptionalSha256Hex {
  param([string]$Path)

  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $null
  }
  return Get-Sha256Hex -Path $Path
}

function Write-Utf8WithoutBom {
  param([string]$Path, [string]$Value)

  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Test-PortableExecutableHasCertificateTable {
  param([string]$Path)

  $stream = [System.IO.File]::Open(
    $Path,
    [System.IO.FileMode]::Open,
    [System.IO.FileAccess]::Read,
    [System.IO.FileShare]::ReadWrite
  )
  $reader = New-Object System.IO.BinaryReader($stream)
  try {
    if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) {
      throw "Release artifact is not a valid PE file."
    }
    $stream.Seek(0x3C, [System.IO.SeekOrigin]::Begin) | Out-Null
    $peOffset = $reader.ReadInt32()
    if ($peOffset -lt 0 -or ($peOffset + 24) -gt $stream.Length) {
      throw "Release artifact has an invalid PE header offset."
    }
    $stream.Seek($peOffset, [System.IO.SeekOrigin]::Begin) | Out-Null
    if ($reader.ReadUInt32() -ne 0x00004550) {
      throw "Release artifact is missing the PE signature."
    }
    $stream.Seek($peOffset + 20, [System.IO.SeekOrigin]::Begin) | Out-Null
    $optionalHeaderSize = $reader.ReadUInt16()
    $optionalHeaderOffset = $peOffset + 24
    if (($optionalHeaderOffset + $optionalHeaderSize) -gt $stream.Length) {
      throw "Release artifact has a truncated optional header."
    }
    $stream.Seek($optionalHeaderOffset, [System.IO.SeekOrigin]::Begin) | Out-Null
    $optionalMagic = $reader.ReadUInt16()
    if ($optionalMagic -eq 0x10B) {
      $certificateEntryOffset = $optionalHeaderOffset + 128
    } elseif ($optionalMagic -eq 0x20B) {
      $certificateEntryOffset = $optionalHeaderOffset + 144
    } else {
      throw "Release artifact uses an unsupported PE optional header."
    }
    if (($certificateEntryOffset + 8) -gt ($optionalHeaderOffset + $optionalHeaderSize)) {
      throw "Release artifact does not contain a complete certificate directory entry."
    }
    $stream.Seek($certificateEntryOffset, [System.IO.SeekOrigin]::Begin) | Out-Null
    $certificateOffset = $reader.ReadUInt32()
    $certificateSize = $reader.ReadUInt32()
    if ($certificateOffset -eq 0 -and $certificateSize -eq 0) {
      return $false
    }
    if ($certificateOffset -eq 0 -or $certificateSize -eq 0 -or ([uint64]$certificateOffset + [uint64]$certificateSize) -gt [uint64]$stream.Length) {
      throw "Release artifact has an invalid PE certificate table."
    }
    return $true
  } finally {
    $reader.Dispose()
    $stream.Dispose()
  }
}

$root = (Resolve-Path -LiteralPath $RootPath).Path
$artifact = (Resolve-Path -LiteralPath $ArtifactPath).Path
$artifact = Assert-WorkspacePath -Path $artifact -WorkspaceRoot $root
if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
  throw "Release artifact is not a regular file: $artifact"
}
$artifactItem = Get-Item -LiteralPath $artifact
if ($artifactItem.Length -le 0) {
  throw "Release artifact is empty: $artifact"
}

$tauriConfigPath = Join-Path $root "src-tauri\tauri.conf.json"
$tauriConfig = Get-Content -Raw -LiteralPath $tauriConfigPath | ConvertFrom-Json
$version = [string]$tauriConfig.version
$expectedFileName = "MarkLite_$($version)_$($Architecture)-setup.exe"
if ($artifactItem.Name -ne $expectedFileName) {
  throw "Release artifact name '$($artifactItem.Name)' does not match '$expectedFileName'."
}

$artifactHash = Get-Sha256Hex -Path $artifact
$signatureStatus = "Unavailable"
$signatureErrorType = $null
$signerSubject = $null
$signerThumbprint = $null
$timeStamperSubject = $null
$timeStamperThumbprint = $null
try {
  if (-not (Test-PortableExecutableHasCertificateTable -Path $artifact)) {
    $signatureStatus = "NotSigned"
  } else {
    Import-Module Microsoft.PowerShell.Security -ErrorAction Stop
    $signature = $null
    for ($attempt = 1; $attempt -le 5; $attempt += 1) {
      try {
        $signature = Get-AuthenticodeSignature -LiteralPath $artifact -ErrorAction Stop
        break
      } catch {
        $signatureErrorType = $_.Exception.GetType().FullName
        if ($attempt -lt 5) {
          Start-Sleep -Milliseconds 250
        }
      }
    }
    if ($null -ne $signature) {
      $signatureStatus = [string]$signature.Status
      $signatureErrorType = $null
      if ($null -ne $signature.SignerCertificate) {
        $signerSubject = [string]$signature.SignerCertificate.Subject
        $signerThumbprint = [string]$signature.SignerCertificate.Thumbprint
      }
      if ($null -ne $signature.TimeStamperCertificate) {
        $timeStamperSubject = [string]$signature.TimeStamperCertificate.Subject
        $timeStamperThumbprint = [string]$signature.TimeStamperCertificate.Thumbprint
      }
    } else {
      $signatureStatus = "Error"
    }
  }
} catch {
  $signatureStatus = "Error"
  $signatureErrorType = $_.Exception.GetType().FullName
}

$evidence = [ordered]@{
  schemaVersion = 1
  productName = [string]$tauriConfig.productName
  version = $version
  architecture = $Architecture
  generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
  buildMode = $BuildMode
  artifact = [ordered]@{
    fileName = $artifactItem.Name
    length = $artifactItem.Length
    sha256 = $artifactHash
  }
  signature = [ordered]@{
    status = $signatureStatus
    errorType = $signatureErrorType
    signerSubject = $signerSubject
    signerThumbprint = $signerThumbprint
    timeStamperSubject = $timeStamperSubject
    timeStamperThumbprint = $timeStamperThumbprint
  }
  source = [ordered]@{
    tauriConfigSha256 = Get-OptionalSha256Hex -Path $tauriConfigPath
    packageLockSha256 = Get-OptionalSha256Hex -Path (Join-Path $root "package-lock.json")
    cargoLockSha256 = Get-OptionalSha256Hex -Path (Join-Path $root "src-tauri\Cargo.lock")
  }
}

$checksumPath = Assert-WorkspacePath -Path "$artifact.sha256" -WorkspaceRoot $root
$evidencePath = Assert-WorkspacePath -Path "$artifact.release.json" -WorkspaceRoot $root
$nonce = [guid]::NewGuid().ToString("N")
$checksumCandidate = Assert-WorkspacePath -Path "$checksumPath.candidate-$nonce" -WorkspaceRoot $root
$evidenceCandidate = Assert-WorkspacePath -Path "$evidencePath.candidate-$nonce" -WorkspaceRoot $root
try {
  [System.IO.File]::WriteAllText($checksumCandidate, "$artifactHash *$($artifactItem.Name)`r`n", [System.Text.Encoding]::ASCII)
  Write-Utf8WithoutBom -Path $evidenceCandidate -Value (($evidence | ConvertTo-Json -Depth 6) + "`n")
  foreach ($existingSidecar in @($checksumPath, $evidencePath)) {
    if (Test-Path -LiteralPath $existingSidecar) {
      Remove-Item -LiteralPath $existingSidecar -Force
    }
  }
  Move-Item -LiteralPath $checksumCandidate -Destination $checksumPath
  Move-Item -LiteralPath $evidenceCandidate -Destination $evidencePath
} finally {
  foreach ($candidate in @($checksumCandidate, $evidenceCandidate)) {
    if (Test-Path -LiteralPath $candidate) {
      Remove-Item -LiteralPath $candidate -Force
    }
  }
}

Write-Host "SHA-256: $artifactHash"
Write-Host "Authenticode status: $signatureStatus"
Write-Host "Release evidence: $evidencePath"
