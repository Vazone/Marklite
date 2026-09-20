Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-CheckedNative {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$FilePath,
    [Parameter(Mandatory = $true)]
    [AllowEmptyCollection()]
    [string[]]$Arguments,
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Description
  )

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
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [string]$WorkspaceRoot
  )

  if ([string]::IsNullOrWhiteSpace($Path)) {
    throw "Packaging path must not be empty."
  }
  if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    throw "Workspace root must not be empty."
  }

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $fullRoot = [System.IO.Path]::GetFullPath($WorkspaceRoot).TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar,
    [System.IO.Path]::AltDirectorySeparatorChar
  )
  $rootPrefix = $fullRoot + [System.IO.Path]::DirectorySeparatorChar
  if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to modify a packaging path outside the workspace: $fullPath"
  }

  return $fullPath
}

function Remove-WorkspaceFileIfPresent {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [string]$WorkspaceRoot
  )

  $validatedPath = Assert-WorkspacePath -Path $Path -WorkspaceRoot $WorkspaceRoot
  if (-not (Test-Path -LiteralPath $validatedPath)) {
    return
  }
  if (-not (Test-Path -LiteralPath $validatedPath -PathType Leaf)) {
    throw "Expected a file but found another filesystem object: $validatedPath"
  }
  Remove-Item -LiteralPath $validatedPath -Force
}

function Get-Sha256Hex {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Path
  )

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

function Write-Utf8WithoutBom {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [AllowEmptyString()]
    [string]$Value
  )

  $encoding = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Resolve-WindowsPowerShellPath {
  [CmdletBinding()]
  param()

  $windowsPowerShell = Join-Path $PSHOME "powershell.exe"
  if (-not (Test-Path -LiteralPath $windowsPowerShell -PathType Leaf)) {
    throw "Windows PowerShell executable was not found: $windowsPowerShell"
  }
  return $windowsPowerShell
}

Export-ModuleMember -Function @(
  "Invoke-CheckedNative",
  "Assert-WorkspacePath",
  "Remove-WorkspaceFileIfPresent",
  "Get-Sha256Hex",
  "Write-Utf8WithoutBom",
  "Resolve-WindowsPowerShellPath"
)
