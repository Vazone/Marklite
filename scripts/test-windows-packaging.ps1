[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:AssertionCount = 0

Import-Module (Join-Path $PSScriptRoot "lib\WindowsPackaging.psm1") -Force

function Assert-True {
  param([bool]$Condition, [string]$Message)

  $script:AssertionCount += 1
  if (-not $Condition) {
    throw "Assertion failed: $Message"
  }
}

function Assert-Equal {
  param($Actual, $Expected, [string]$Message)

  $script:AssertionCount += 1
  if ($Actual -ne $Expected) {
    throw "Assertion failed: $Message. Expected '$Expected', got '$Actual'."
  }
}

function Assert-Throws {
  param([scriptblock]$Action, [string]$MessagePattern, [string]$Message)

  $script:AssertionCount += 1
  try {
    & $Action
  } catch {
    if ($_.Exception.Message -notlike "*$MessagePattern*") {
      throw "Assertion failed: $Message. Unexpected error: $($_.Exception.Message)"
    }
    return
  }
  throw "Assertion failed: $Message. No error was thrown."
}

function Write-InstallerTemplate {
  param([string]$Path, [string]$LineEnding = "`n")

  $lines = @(
    "!include MUI2.nsh",
    "Var OldMainBinaryName",
    "; 7. Installation page",
    "Function .onInit",
    "FunctionEnd",
    "Section",
    "  ; Create file associations",
    "SectionEnd",
    "Section Uninstall",
    "  ; Delete app associations",
    "SectionEnd"
  )
  Write-Utf8WithoutBom -Path $Path -Value (($lines -join $LineEnding) + $LineEnding)
}

function Write-CompilableInstallerTemplate {
  param([string]$Path, [string]$LineEnding = "`r`n")

  $lines = @(
    "Unicode true",
    'Name "MarkLite Packaging Fixture"',
    'OutFile "nsis-output.exe"',
    'InstallDir "$TEMP\MarkLitePackagingFixture"',
    "RequestExecutionLevel user",
    '!define MAINBINARYNAME "marklite"',
    "!include MUI2.nsh",
    "Var PassiveMode",
    "Var OldMainBinaryName",
    "; 7. Installation page",
    "Page instfiles",
    "UninstPage instfiles",
    "Function .onInit",
    '  StrCpy $PassiveMode 0',
    "FunctionEnd",
    'Section "Install"',
    "  SetOutPath `$INSTDIR",
    "  ; Create file associations",
    "  WriteUninstaller `"`$INSTDIR\uninstall.exe`"",
    "SectionEnd",
    'Section "Uninstall"',
    "  ; Delete app associations",
    "  Delete `"`$INSTDIR\uninstall.exe`"",
    "SectionEnd"
  )
  Write-Utf8WithoutBom -Path $Path -Value (($lines -join $LineEnding) + $LineEnding)
}

$patchScript = Join-Path $PSScriptRoot "patch-nsis-installer.ps1"
$buildScript = Join-Path $PSScriptRoot "build-windows-installer.ps1"
$packageScript = Join-Path $PSScriptRoot "package-windows.ps1"
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "marklite-packaging-tests-$([guid]::NewGuid().ToString('N'))"

New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
  $helperRoot = Join-Path $testRoot "helper-root"
  New-Item -ItemType Directory -Path $helperRoot | Out-Null
  $helperFile = Join-Path $helperRoot "unicode.txt"
  Write-Utf8WithoutBom -Path $helperFile -Value "MarkLite-治理"
  $helperBytes = [System.IO.File]::ReadAllBytes($helperFile)
  Assert-True -Condition ($helperBytes.Length -gt 3) -Message "UTF-8 helper must write content"
  Assert-True -Condition (-not ($helperBytes[0] -eq 0xEF -and $helperBytes[1] -eq 0xBB -and $helperBytes[2] -eq 0xBF)) -Message "UTF-8 helper must not write a BOM"
  Assert-Equal -Actual (Get-Sha256Hex -Path $helperFile).Length -Expected 64 -Message "SHA-256 helper must return lowercase hexadecimal"
  Assert-Equal -Actual (Assert-WorkspacePath -Path $helperFile -WorkspaceRoot $helperRoot) -Expected ([System.IO.Path]::GetFullPath($helperFile)) -Message "Workspace helper must accept a child path"
  Assert-Throws -Action { Assert-WorkspacePath -Path " " -WorkspaceRoot $helperRoot } -MessagePattern "must not be empty" -Message "Workspace helper must reject an empty path"
  Assert-Throws -Action { Assert-WorkspacePath -Path (Join-Path $testRoot "outside.txt") -WorkspaceRoot $helperRoot } -MessagePattern "outside the workspace" -Message "Workspace helper must reject a sibling path"
  $helperFailCommand = Join-Path $testRoot "helper-fail.cmd"
  [System.IO.File]::WriteAllText($helperFailCommand, "@echo off`r`nexit /b 29`r`n", [System.Text.Encoding]::ASCII)
  Assert-Throws -Action { Invoke-CheckedNative -FilePath $helperFailCommand -Arguments @() -Description "Helper native command" } -MessagePattern "exit code 29" -Message "Native helper must propagate a non-zero exit code"

  $lfTemplate = Join-Path $testRoot "lf-installer.nsi"
  Write-InstallerTemplate -Path $lfTemplate -LineEnding "`n"
  & $patchScript -InstallerScript $lfTemplate
  $firstHash = Get-Sha256Hex -Path $lfTemplate
  & $patchScript -InstallerScript $lfTemplate
  $secondHash = Get-Sha256Hex -Path $lfTemplate
  Assert-Equal -Actual $secondHash -Expected $firstHash -Message "Complete patch must be idempotent"
  $patchedContent = [System.IO.File]::ReadAllText($lfTemplate)
  Assert-Equal -Actual ([regex]::Matches($patchedContent, "MARKLITE_INTEGRATION_PATCH")).Count -Expected 1 -Message "Patch marker must be unique"
  foreach ($format in @("pdf", "docx", "html")) {
    Assert-True -Condition $patchedContent.Contains("!insertmacro MarkLiteRegisterExportVerb `"$format`"") -Message "Shell $format must use the shared finite command macro"
  }
  Assert-True -Condition $patchedContent.Contains('--marklite-shell-export --input $\"%1$\" --format ${FORMAT}') -Message 'Shared Shell command must quote the selected path'
  foreach ($name in @('Pdf', 'Docx', 'Html')) {
    Assert-True -Condition $patchedContent.Contains("!insertmacro MarkLiteMigrateLegacyExport `"$name`"") -Message "Legacy $name must migrate"
    Assert-True -Condition (-not $patchedContent.Contains("WriteRegStr HKCU `"Software\Classes\SystemFileAssociations\.md\shell\MarkLite.Export$name`"")) -Message "Legacy $name must not be registered"
  }
  Assert-True -Condition $patchedContent.Contains('"ExtendedSubCommandsKey" "MarkLite.ExportCommands"') -Message 'Parent must link the HKCU private command class'
  Assert-True -Condition $patchedContent.Contains('"${MARKLITE_EXPORT_PARENT}" "MultiSelectModel" "Single"') -Message 'Parent must use single selection'
  Assert-True -Condition $patchedContent.Contains('"${MARKLITE_EXPORT_COMMANDS}\shell\${FORMAT}" "MarkLiteInstallDir" "$INSTDIR"') -Message 'Children must record owner'
  Assert-True -Condition $patchedContent.Contains('${AndIf} $0 == "${OWNER}"') -Message 'Removal must guard owner'
  Assert-True -Condition $patchedContent.Contains('DeleteRegKey /ifempty HKCU "${KEY}"') -Message 'Unknown extension values must survive cleanup'
  Assert-True -Condition $patchedContent.Contains('Call RemoveMarkLiteOpenMenus') -Message 'Disabling integration on reinstall must remove owned menus'
  Assert-True -Condition (-not $patchedContent.Contains("SystemFileAssociations\.markdown\shell\MarkLite.Export")) -Message "Conversion verbs must be scoped to .md"
  Assert-True -Condition (-not $patchedContent.Contains("SystemFileAssociations\.txt\shell\MarkLite.Export")) -Message "Conversion verbs must not affect .txt"
  Assert-True -Condition $patchedContent.Contains('  ${OrIf} ${Silent}') -Message "Silent installs must retain the default context-menu registration"
  Assert-True -Condition (-not $patchedContent.Contains("`r`n")) -Message "LF template must retain LF line endings"
  $patchedBytes = [System.IO.File]::ReadAllBytes($lfTemplate)
  Assert-True -Condition ($patchedBytes.Length -ge 3 -and $patchedBytes[0] -eq 0xEF -and $patchedBytes[1] -eq 0xBB -and $patchedBytes[2] -eq 0xBF) -Message "Patched NSIS script must have a UTF-8 BOM"

  $crlfTemplate = Join-Path $testRoot "crlf-installer.nsi"
  Write-InstallerTemplate -Path $crlfTemplate -LineEnding "`r`n"
  & $patchScript -InstallerScript $crlfTemplate
  $crlfContent = [System.IO.File]::ReadAllText($crlfTemplate)
  Assert-Equal -Actual ([regex]::Matches($crlfContent, "MARKLITE_INTEGRATION_PATCH")).Count -Expected 1 -Message "CRLF template must receive the complete patch once"
  Assert-True -Condition ($crlfContent.Contains("`r`n")) -Message "CRLF template must retain CRLF line endings"
  Assert-True -Condition (-not [regex]::IsMatch($crlfContent, "(?<!`r)`n")) -Message "CRLF patch must not introduce bare LF lines"

  $realMakeNsis = Join-Path $env:LOCALAPPDATA "tauri\NSIS\Bin\makensis.exe"
  if (Test-Path -LiteralPath $realMakeNsis -PathType Leaf) {
    $realCompileDir = Join-Path $testRoot "real-nsis"
    New-Item -ItemType Directory -Path $realCompileDir | Out-Null
    $realInstaller = Join-Path $realCompileDir "installer.nsi"
    Write-CompilableInstallerTemplate -Path $realInstaller
    & $patchScript -InstallerScript $realInstaller
    Push-Location $realCompileDir
    try {
      Invoke-CheckedNative -FilePath $realMakeNsis -Arguments @("/V2", "installer.nsi") -Description "Real makensis fixture compile"
    } finally {
      Pop-Location
    }
    Assert-True -Condition (Test-Path -LiteralPath (Join-Path $realCompileDir "nsis-output.exe") -PathType Leaf) -Message "Real makensis must compile the patched CRLF fixture"

    $invalidInstaller = Join-Path $realCompileDir "invalid-installer.nsi"
    Write-Utf8WithoutBom -Path $invalidInstaller -Value 'This is deliberately invalid NSIS syntax.'
    Push-Location $realCompileDir
    try {
      Assert-Throws -Action { Invoke-CheckedNative -FilePath $realMakeNsis -Arguments @("/V2", "invalid-installer.nsi") -Description "Invalid makensis fixture compile" } -MessagePattern "failed with exit code" -Message "Real makensis must reject invalid syntax"
    } finally {
      Pop-Location
    }
  } else {
    Write-Host "Real makensis fixture compile skipped: Tauri NSIS is not installed on this machine."
  }

  $partialTemplate = Join-Path $testRoot "partial-installer.nsi"
  Write-InstallerTemplate -Path $partialTemplate
  [System.IO.File]::AppendAllText($partialTemplate, "; MARKLITE_INTEGRATION_PATCH: optional Windows integration controls`n")
  Assert-Throws -Action { & $patchScript -InstallerScript $partialTemplate } -MessagePattern "patched fragment" -Message "Partial patch must fail"

  $missingAnchorTemplate = Join-Path $testRoot "missing-anchor-installer.nsi"
  Write-InstallerTemplate -Path $missingAnchorTemplate
  $missingContent = [System.IO.File]::ReadAllText($missingAnchorTemplate).Replace("Function .onInit", "Function Other")
  Write-Utf8WithoutBom -Path $missingAnchorTemplate -Value $missingContent
  Assert-Throws -Action { & $patchScript -InstallerScript $missingAnchorTemplate } -MessagePattern "Function .onInit*found 0" -Message "Missing template anchor must fail"

  $duplicateAnchorTemplate = Join-Path $testRoot "duplicate-anchor-installer.nsi"
  Write-InstallerTemplate -Path $duplicateAnchorTemplate
  [System.IO.File]::AppendAllText($duplicateAnchorTemplate, "!include MUI2.nsh`n")
  Assert-Throws -Action { & $patchScript -InstallerScript $duplicateAnchorTemplate } -MessagePattern "!include MUI2.nsh*found 2" -Message "Duplicate template anchor must fail"

  $fakeRoot = Join-Path $testRoot "fake-repository"
  $nsisDir = Join-Path $fakeRoot "src-tauri\target\release\nsis\x64"
  $bundleDir = Join-Path $fakeRoot "src-tauri\target\release\bundle\nsis"
  New-Item -ItemType Directory -Force -Path $nsisDir, $bundleDir | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $fakeRoot "src-tauri") | Out-Null
  Write-Utf8WithoutBom -Path (Join-Path $fakeRoot "src-tauri\tauri.conf.json") -Value '{"productName":"MarkLite","version":"9.8.7"}'

  $installerScript = Join-Path $nsisDir "installer.nsi"
  $nsisOutput = Join-Path $nsisDir "nsis-output.exe"
  $finalInstaller = Join-Path $bundleDir "MarkLite_9.8.7_x64-setup.exe"
  $failCommand = Join-Path $testRoot "fail.cmd"
  $makeNsisFailCommand = Join-Path $testRoot "makensis-fail.cmd"
  $tauriSuccessCommand = Join-Path $testRoot "tauri-success.cmd"
  $successCommand = Join-Path $testRoot "success.cmd"
  $referenceExecutable = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
  Assert-True -Condition (Test-Path -LiteralPath $referenceExecutable -PathType Leaf) -Message "Signed Windows fixture executable must exist"
  [System.IO.File]::WriteAllText($failCommand, "@echo off`r`n> `"src-tauri\target\release\bundle\nsis\MarkLite_9.8.7_x64-setup.exe`" echo untrusted-output`r`nexit /b 23`r`n", [System.Text.Encoding]::ASCII)
  [System.IO.File]::WriteAllText($makeNsisFailCommand, "@echo off`r`nexit /b 23`r`n", [System.Text.Encoding]::ASCII)
  [System.IO.File]::WriteAllText($tauriSuccessCommand, "@echo off`r`ncopy /y `"$lfTemplate`" `"src-tauri\target\release\nsis\x64\installer.nsi`" > nul`r`n> `"src-tauri\target\release\bundle\nsis\MarkLite_9.8.7_x64-setup.exe`" echo tauri-default-bundle`r`nexit /b 0`r`n", [System.Text.Encoding]::ASCII)
  [System.IO.File]::WriteAllText($successCommand, "@echo off`r`ncopy /y `"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`" nsis-output.exe > nul`r`nexit /b 0`r`n", [System.Text.Encoding]::ASCII)

  Write-InstallerTemplate -Path $installerScript
  Write-Utf8WithoutBom -Path $nsisOutput -Value "stale-output"
  Write-Utf8WithoutBom -Path $finalInstaller -Value "stale-final"
  Assert-Throws -Action { & $buildScript -RootPath $fakeRoot -NpmCommand $failCommand -MakeNsisPath $successCommand } -MessagePattern "Tauri build failed with exit code 23" -Message "Native Tauri build failure must stop packaging"
  Assert-True -Condition (-not (Test-Path -LiteralPath $installerScript)) -Message "Failed full build must not retain the old installer script"
  Assert-True -Condition (-not (Test-Path -LiteralPath $nsisOutput)) -Message "Failed full build must remove stale NSIS output"
  Assert-True -Condition (-not (Test-Path -LiteralPath $finalInstaller)) -Message "Failed full build must remove stale or newly emitted untrusted final installer"

  Write-InstallerTemplate -Path $installerScript
  Write-Utf8WithoutBom -Path $nsisOutput -Value "stale-output"
  Write-Utf8WithoutBom -Path $finalInstaller -Value "stale-final"
  Assert-Throws -Action { & $buildScript -SkipTauriBuild -RootPath $fakeRoot -MakeNsisPath $makeNsisFailCommand } -MessagePattern "makensis failed with exit code 23" -Message "Native makensis failure must stop packaging"
  Assert-True -Condition (-not (Test-Path -LiteralPath $nsisOutput)) -Message "Failed makensis must not leave stale output"
  Assert-True -Condition (-not (Test-Path -LiteralPath $finalInstaller)) -Message "Failed makensis must not leave stale final installer"

  Assert-Throws -Action { & $buildScript -RootPath $fakeRoot -NpmCommand $tauriSuccessCommand -MakeNsisPath $successCommand } -MessagePattern "does not install and remove marklite-cli.exe" -Message "Full build must reject Tauri output without the CLI sidecar"
  [System.IO.File]::WriteAllText($tauriSuccessCommand, "@echo off`r`ncopy /y `"$lfTemplate`" `"src-tauri\target\release\nsis\x64\installer.nsi`" > nul`r`n>> `"src-tauri\target\release\nsis\x64\installer.nsi`" echo File /a /oname=marklite-cli.exe fixture-cli.exe`r`n>> `"src-tauri\target\release\nsis\x64\installer.nsi`" echo Delete `"`$INSTDIR\marklite-cli.exe`"`r`n> `"src-tauri\target\release\bundle\nsis\MarkLite_9.8.7_x64-setup.exe`" echo tauri-default-bundle`r`nexit /b 0`r`n", [System.Text.Encoding]::ASCII)
  & $buildScript -RootPath $fakeRoot -NpmCommand $tauriSuccessCommand -MakeNsisPath $successCommand
  Assert-True -Condition (Test-Path -LiteralPath $finalInstaller -PathType Leaf) -Message "Successful makensis must create final installer"
  Assert-Equal -Actual (Get-Sha256Hex -Path $finalInstaller) -Expected (Get-Sha256Hex -Path $referenceExecutable) -Message "Custom installer must replace Tauri's default bundle"
  $finalHash = Get-Sha256Hex -Path $finalInstaller
  $checksumPath = "$finalInstaller.sha256"
  $evidencePath = "$finalInstaller.release.json"
  Assert-True -Condition (Test-Path -LiteralPath $checksumPath -PathType Leaf) -Message "Checksum sidecar must exist"
  Assert-True -Condition (Test-Path -LiteralPath $evidencePath -PathType Leaf) -Message "Release evidence must exist"
  $checksum = (Get-Content -Raw -LiteralPath $checksumPath).Trim()
  Assert-Equal -Actual $checksum -Expected "$finalHash *MarkLite_9.8.7_x64-setup.exe" -Message "Checksum sidecar must bind hash to file name"
  $evidence = Get-Content -Raw -LiteralPath $evidencePath | ConvertFrom-Json
  $rawEvidence = Get-Content -Raw -LiteralPath $evidencePath
  Assert-Equal -Actual $evidence.schemaVersion -Expected 1 -Message "Evidence schema version"
  Assert-Equal -Actual $evidence.version -Expected "9.8.7" -Message "Evidence version"
  Assert-Equal -Actual $evidence.architecture -Expected "x64" -Message "Evidence architecture"
  Assert-Equal -Actual $evidence.artifact.sha256 -Expected $finalHash -Message "Evidence SHA-256"
  Assert-True -Condition (-not [string]::IsNullOrWhiteSpace([string]$evidence.signature.status)) -Message "Evidence signature status must be explicit"
  Assert-True -Condition (-not $rawEvidence.Contains($testRoot)) -Message "Evidence must not contain local absolute paths"

  $wrapperRoot = Join-Path $testRoot "wrapper-repository"
  $wrapperBundleDir = Join-Path $wrapperRoot "src-tauri\target\release\bundle\nsis"
  New-Item -ItemType Directory -Force -Path (Join-Path $wrapperRoot "src-tauri"), $wrapperBundleDir | Out-Null
  Write-Utf8WithoutBom -Path (Join-Path $wrapperRoot "src-tauri\tauri.conf.json") -Value '{"productName":"MarkLite","version":"7.6.5"}'
  $wrapperArtifact = Join-Path $wrapperBundleDir "MarkLite_7.6.5_x64-setup.exe"
  $wrapperBuildMarker = Join-Path $wrapperRoot "build-stage.txt"
  $wrapperEvidenceMarker = Join-Path $wrapperRoot "evidence-stage.txt"
  $wrapperBuildSuccess = Join-Path $testRoot "wrapper-build-success.ps1"
  $wrapperBuildFailure = Join-Path $testRoot "wrapper-build-failure.ps1"
  $wrapperEvidenceSuccess = Join-Path $testRoot "wrapper-evidence-success.ps1"
  $wrapperEvidenceFailure = Join-Path $testRoot "wrapper-evidence-failure.ps1"
  $escapedReferenceExecutable = $referenceExecutable.Replace("'", "''")
  $escapedBuildMarker = $wrapperBuildMarker.Replace("'", "''")
  $escapedEvidenceMarker = $wrapperEvidenceMarker.Replace("'", "''")

$buildSuccessFixture = @'
param(
  [string]$Architecture,
  [string]$RootPath,
  [switch]$SkipEvidence,
  [switch]$SkipTauriBuild
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$config = Get-Content -Raw -LiteralPath (Join-Path $RootPath 'src-tauri\tauri.conf.json') | ConvertFrom-Json
$bundle = Join-Path $RootPath 'src-tauri\target\release\bundle\nsis'
New-Item -ItemType Directory -Force -Path $bundle | Out-Null
$artifact = Join-Path $bundle "MarkLite_$($config.version)_$($Architecture)-setup.exe"
Copy-Item -LiteralPath '__REFERENCE_EXE__' -Destination $artifact
[System.IO.File]::WriteAllText('__BUILD_MARKER__', "skip=$($SkipTauriBuild.IsPresent);evidence=$($SkipEvidence.IsPresent)")
'@.Replace('__REFERENCE_EXE__', $escapedReferenceExecutable).Replace('__BUILD_MARKER__', $escapedBuildMarker)
  Write-Utf8WithoutBom -Path $wrapperBuildSuccess -Value $buildSuccessFixture

$buildFailureFixture = @'
param(
  [string]$Architecture,
  [string]$RootPath,
  [switch]$SkipEvidence,
  [switch]$SkipTauriBuild
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
[System.IO.File]::WriteAllText('__BUILD_MARKER__', 'failed')
exit 37
'@.Replace('__BUILD_MARKER__', $escapedBuildMarker)
  Write-Utf8WithoutBom -Path $wrapperBuildFailure -Value $buildFailureFixture

$evidenceSuccessFixture = @'
param(
  [string]$ArtifactPath,
  [string]$Architecture,
  [string]$BuildMode,
  [string]$RootPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
[System.IO.File]::WriteAllText('__EVIDENCE_MARKER__', $BuildMode)
$stream = [System.IO.File]::OpenRead($ArtifactPath)
$sha256 = [System.Security.Cryptography.SHA256]::Create()
try {
  $hash = (($sha256.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) -join '')
} finally {
  $sha256.Dispose()
  $stream.Dispose()
}
$fileName = [System.IO.Path]::GetFileName($ArtifactPath)
[System.IO.File]::WriteAllText("$ArtifactPath.sha256", "$hash *$fileName`r`n", [System.Text.Encoding]::ASCII)
$body = [ordered]@{
  artifact = [ordered]@{ fileName = $fileName; sha256 = $hash }
  signature = [ordered]@{ status = 'NotSigned' }
}
[System.IO.File]::WriteAllText("$ArtifactPath.release.json", (($body | ConvertTo-Json -Depth 3) + "`n"), [System.Text.UTF8Encoding]::new($false))
'@.Replace('__EVIDENCE_MARKER__', $escapedEvidenceMarker)
  Write-Utf8WithoutBom -Path $wrapperEvidenceSuccess -Value $evidenceSuccessFixture

$evidenceFailureFixture = @'
param(
  [string]$ArtifactPath,
  [string]$Architecture,
  [string]$BuildMode,
  [string]$RootPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
[System.IO.File]::WriteAllText('__EVIDENCE_MARKER__', 'failed')
exit 38
'@.Replace('__EVIDENCE_MARKER__', $escapedEvidenceMarker)
  Write-Utf8WithoutBom -Path $wrapperEvidenceFailure -Value $evidenceFailureFixture

  $windowsPowerShell = Resolve-WindowsPowerShellPath
  Assert-Throws -Action {
    & $packageScript -RootPath $wrapperRoot -BuildScriptPath $wrapperBuildFailure -EvidenceScriptPath $wrapperEvidenceSuccess -WindowsPowerShellPath $windowsPowerShell
  } -MessagePattern "exit code 37" -Message "Wrapper must stop when the build stage fails"
  Assert-True -Condition (Test-Path -LiteralPath $wrapperBuildMarker -PathType Leaf) -Message "Wrapper must invoke the build stage"
  Assert-True -Condition (-not (Test-Path -LiteralPath $wrapperEvidenceMarker)) -Message "Wrapper must not invoke evidence after a build failure"
  Assert-True -Condition (-not (Test-Path -LiteralPath $wrapperArtifact)) -Message "Build-stage failure must not retain an installer"

  Remove-Item -LiteralPath $wrapperBuildMarker -Force
  Assert-Throws -Action {
    & $packageScript -RootPath $wrapperRoot -BuildScriptPath $wrapperBuildSuccess -EvidenceScriptPath $wrapperEvidenceFailure -WindowsPowerShellPath $windowsPowerShell
  } -MessagePattern "exit code 38" -Message "Wrapper must surface an evidence-stage failure"
  Assert-True -Condition (Test-Path -LiteralPath $wrapperBuildMarker -PathType Leaf) -Message "Evidence failure case must complete the build stage"
  Assert-True -Condition (Test-Path -LiteralPath $wrapperEvidenceMarker -PathType Leaf) -Message "Evidence failure case must invoke the evidence stage"
  foreach ($wrapperOutput in @($wrapperArtifact, "$wrapperArtifact.sha256", "$wrapperArtifact.release.json")) {
    Assert-True -Condition (-not (Test-Path -LiteralPath $wrapperOutput)) -Message "Evidence-stage failure must remove output $wrapperOutput"
  }

  Remove-Item -LiteralPath $wrapperBuildMarker, $wrapperEvidenceMarker -Force
  & $packageScript -SkipTauriBuild -RootPath $wrapperRoot -BuildScriptPath $wrapperBuildSuccess -EvidenceScriptPath $wrapperEvidenceSuccess -WindowsPowerShellPath $windowsPowerShell
  Assert-True -Condition (Test-Path -LiteralPath $wrapperArtifact -PathType Leaf) -Message "Successful wrapper must retain the installer"
  Assert-True -Condition (Test-Path -LiteralPath "$wrapperArtifact.sha256" -PathType Leaf) -Message "Successful wrapper must retain checksum evidence"
  Assert-True -Condition (Test-Path -LiteralPath "$wrapperArtifact.release.json" -PathType Leaf) -Message "Successful wrapper must retain JSON evidence"
  $wrapperEvidence = Get-Content -Raw -LiteralPath "$wrapperArtifact.release.json" | ConvertFrom-Json
  Assert-Equal -Actual $wrapperEvidence.artifact.sha256 -Expected (Get-Sha256Hex -Path $wrapperArtifact) -Message "Successful wrapper evidence must bind the actual artifact hash"
  Assert-Equal -Actual (Get-Content -Raw -LiteralPath $wrapperBuildMarker) -Expected "skip=True;evidence=True" -Message "Wrapper must forward SkipTauriBuild and always defer build-stage evidence"
  Assert-Equal -Actual (Get-Content -Raw -LiteralPath $wrapperEvidenceMarker) -Expected "skip-tauri-build" -Message "Wrapper must forward the evidence build mode"

  if (Test-Path -LiteralPath $realMakeNsis -PathType Leaf) {
    & (Join-Path $PSScriptRoot 'test-export-menu.ps1')
  }
  Write-Host "Windows packaging tests passed: $script:AssertionCount assertions."
} finally {
  $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
  $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
  if ($resolvedTestRoot.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolvedTestRoot -Leaf).StartsWith("marklite-packaging-tests-")) {
    Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
  } else {
    throw "Refusing to clean unexpected packaging test directory: $resolvedTestRoot"
  }
}
