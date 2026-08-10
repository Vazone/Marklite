[CmdletBinding()]
param([string]$InstallerScript)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($InstallerScript)) {
  $InstallerScript = Join-Path $PSScriptRoot "..\src-tauri\target\release\nsis\x64\installer.nsi"
}
$InstallerScript = (Resolve-Path -LiteralPath $InstallerScript).Path
$content = [System.IO.File]::ReadAllText($InstallerScript, [System.Text.Encoding]::UTF8)
$newline = $(if ($content.Contains("`r`n")) { "`r`n" } else { "`n" })

function Decode-Utf8Base64([string]$Value) {
  [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($Value))
}

function Convert-LineEndings {
  param([string]$Value, [string]$LineEnding)

  $normalized = $Value.Replace("`r`n", "`n").Replace("`r", "`n").TrimEnd("`r", "`n")
  return $normalized.Replace("`n", $LineEnding)
}

function Get-LiteralCount {
  param([string]$Value, [string]$Literal)

  return ([regex]::Matches($Value, [regex]::Escape($Literal))).Count
}

function Assert-UniqueLiteral {
  param([string]$Value, [string]$Literal, [string]$Kind)

  $count = Get-LiteralCount -Value $Value -Literal $Literal
  if ($count -ne 1) {
    throw "NSIS $Kind '$Literal' expected exactly once, found $count."
  }
}

function Replace-UniqueLiteral {
  param([string]$Value, [string]$Anchor, [string]$Replacement)

  Assert-UniqueLiteral -Value $Value -Literal $Anchor -Kind "template anchor"
  return $Value.Replace($Anchor, $Replacement)
}

$includePatch = @'
!include nsDialogs.nsh
; MARKLITE_INTEGRATION_PATCH: optional Windows integration controls
'@

$variablePatch = @'
Var RegisterContextMenuCheckbox
Var RegisterContextMenuCheckboxState
Var RegisterDefaultMarkdownCheckbox
Var RegisterDefaultMarkdownCheckboxState
'@

$integrationLabel = Decode-Utf8Base64 "Q2hvb3NlIFdpbmRvd3MgaW50ZWdyYXRpb24gb3B0aW9ucyBmb3IgTWFya0xpdGUuIC8g6YCJ5oupIE1hcmtMaXRlIOeahCBXaW5kb3dzIOmbhuaIkOmAiemhueOAgg=="
$contextMenuLabel = Decode-Utf8Base64 "QWRkIE9wZW4gd2l0aCBNYXJrTGl0ZSB0byB0aGUgcmlnaHQtY2xpY2sgbWVudSBmb3IgLm1kLCAubWFya2Rvd24gYW5kIC50eHQgZmlsZXMuIC8g5re75Yqg5Yiw5Y+z6ZSu6I+c5Y2V"
$defaultMarkdownLabel = Decode-Utf8Base64 "U2V0IE1hcmtMaXRlIGFzIHRoZSBkZWZhdWx0IGFwcCBmb3IgTWFya2Rvd24gZmlsZXMgKC5tZCwgLm1hcmtkb3duKS4gV2luZG93cyBtYXkgc3RpbGwgYXNrIGZvciBjb25maXJtYXRpb24gaW4gRGVmYXVsdCBBcHMuIC8g6K6+5Li6IE1hcmtkb3duIOm7mOiupOaJk+W8gOaWueW8j+OAgg=="

$pagePatch = @'
; MarkLite Windows integration page
Page custom PageMarkLiteIntegrations PageLeaveMarkLiteIntegrations
'@

$functionPatch = @'
Function PageMarkLiteIntegrations
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    Abort
  ${EndIf}

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 32u "__MARKLITE_INTEGRATION_LABEL__"
  Pop $1

  ${NSD_CreateCheckbox} 0 42u 100% 18u "__MARKLITE_CONTEXT_MENU_LABEL__"
  Pop $RegisterContextMenuCheckbox
  ${NSD_SetState} $RegisterContextMenuCheckbox ${BST_CHECKED}

  ${NSD_CreateCheckbox} 0 66u 100% 28u "__MARKLITE_DEFAULT_MARKDOWN_LABEL__"
  Pop $RegisterDefaultMarkdownCheckbox
  ${NSD_SetState} $RegisterDefaultMarkdownCheckbox ${BST_UNCHECKED}

  nsDialogs::Show
FunctionEnd

Function PageLeaveMarkLiteIntegrations
  ${NSD_GetState} $RegisterContextMenuCheckbox $RegisterContextMenuCheckboxState
  ${NSD_GetState} $RegisterDefaultMarkdownCheckbox $RegisterDefaultMarkdownCheckboxState
FunctionEnd

Function RegisterMarkLiteApplication
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".md" ""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".markdown" ""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".txt" ""

  WriteRegStr HKCU "Software\MarkLite\Capabilities" "ApplicationName" "MarkLite"
  WriteRegStr HKCU "Software\MarkLite\Capabilities" "ApplicationDescription" "Lightweight Markdown editor"
  WriteRegStr HKCU "Software\MarkLite\Capabilities\FileAssociations" ".md" "MarkLite.md"
  WriteRegStr HKCU "Software\MarkLite\Capabilities\FileAssociations" ".markdown" "MarkLite.markdown"
  WriteRegStr HKCU "Software\MarkLite\Capabilities\FileAssociations" ".txt" "MarkLite.txt"
  WriteRegStr HKCU "Software\RegisteredApplications" "MarkLite" "Software\MarkLite\Capabilities"

  WriteRegStr HKCU "Software\Classes\MarkLite.md" "" "Markdown Document"
  WriteRegStr HKCU "Software\Classes\MarkLite.md\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\MarkLite.md\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""

  WriteRegStr HKCU "Software\Classes\MarkLite.markdown" "" "Markdown Document"
  WriteRegStr HKCU "Software\Classes\MarkLite.markdown\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\MarkLite.markdown\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""

  WriteRegStr HKCU "Software\Classes\MarkLite.txt" "" "Text Document"
  WriteRegStr HKCU "Software\Classes\MarkLite.txt\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\MarkLite.txt\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
FunctionEnd

Function RegisterMarkLiteContextMenu
  Call RegisterMarkLiteApplication

  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.md\shell\MarkLite" "" "Open with MarkLite"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.md\shell\MarkLite" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.md\shell\MarkLite\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""

  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.markdown\shell\MarkLite" "" "Open with MarkLite"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.markdown\shell\MarkLite" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.markdown\shell\MarkLite\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""

  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.txt\shell\MarkLite" "" "Open with MarkLite"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.txt\shell\MarkLite" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.txt\shell\MarkLite\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""

  System::Call 'Shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
FunctionEnd

Function RegisterMarkLiteDefaultMarkdown
  Call RegisterMarkLiteApplication

  WriteRegStr HKCU "Software\Classes\.md" "" "MarkLite.md"
  WriteRegStr HKCU "Software\Classes\.markdown" "" "MarkLite.markdown"
  System::Call 'Shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
FunctionEnd

Function un.UnregisterMarkLiteIntegrations
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.md\shell\MarkLite"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.markdown\shell\MarkLite"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.txt\shell\MarkLite"

  DeleteRegKey HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe"
  DeleteRegKey HKCU "Software\Classes\MarkLite.md"
  DeleteRegKey HKCU "Software\Classes\MarkLite.markdown"
  DeleteRegKey HKCU "Software\Classes\MarkLite.txt"
  DeleteRegValue HKCU "Software\RegisteredApplications" "MarkLite"
  DeleteRegKey HKCU "Software\MarkLite\Capabilities"
  DeleteRegKey /ifempty HKCU "Software\MarkLite"

  ReadRegStr $0 HKCU "Software\Classes\.md" ""
  ${If} $0 == "MarkLite.md"
    DeleteRegValue HKCU "Software\Classes\.md" ""
  ${EndIf}
  ReadRegStr $0 HKCU "Software\Classes\.markdown" ""
  ${If} $0 == "MarkLite.markdown"
    DeleteRegValue HKCU "Software\Classes\.markdown" ""
  ${EndIf}

  System::Call 'Shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
FunctionEnd
'@

$functionPatch = $functionPatch.Replace("__MARKLITE_INTEGRATION_LABEL__", $integrationLabel)
$functionPatch = $functionPatch.Replace("__MARKLITE_CONTEXT_MENU_LABEL__", $contextMenuLabel)
$functionPatch = $functionPatch.Replace("__MARKLITE_DEFAULT_MARKDOWN_LABEL__", $defaultMarkdownLabel)

$installPatch = @'
  ${If} $RegisterContextMenuCheckboxState = ${BST_CHECKED}
    Call RegisterMarkLiteContextMenu
  ${EndIf}

  ${If} $RegisterDefaultMarkdownCheckboxState = ${BST_CHECKED}
    Call RegisterMarkLiteDefaultMarkdown
  ${EndIf}
'@

$uninstallPatch = @'
  Call un.UnregisterMarkLiteIntegrations
'@

$expectedFragments = @(
  "; MARKLITE_INTEGRATION_PATCH: optional Windows integration controls",
  "Var RegisterContextMenuCheckboxState",
  "Page custom PageMarkLiteIntegrations PageLeaveMarkLiteIntegrations",
  "Function PageMarkLiteIntegrations",
  "Function RegisterMarkLiteApplication",
  "Function RegisterMarkLiteContextMenu",
  "Function RegisterMarkLiteDefaultMarkdown",
  "Function un.UnregisterMarkLiteIntegrations",
  '  ${If} $RegisterContextMenuCheckboxState = ${BST_CHECKED}',
  "Call un.UnregisterMarkLiteIntegrations"
)

$marker = $expectedFragments[0]
$markerCount = Get-LiteralCount -Value $content -Literal $marker
if ($markerCount -gt 0) {
  foreach ($fragment in $expectedFragments) {
    Assert-UniqueLiteral -Value $content -Literal $fragment -Kind "patched fragment"
  }
  Write-Host "NSIS installer already contains the complete MarkLite integration patch."
  return
}

$includePatch = Convert-LineEndings -Value $includePatch -LineEnding $newline
$variablePatch = Convert-LineEndings -Value $variablePatch -LineEnding $newline
$pagePatch = Convert-LineEndings -Value $pagePatch -LineEnding $newline
$functionPatch = Convert-LineEndings -Value $functionPatch -LineEnding $newline
$installPatch = Convert-LineEndings -Value $installPatch -LineEnding $newline
$uninstallPatch = Convert-LineEndings -Value $uninstallPatch -LineEnding $newline

$content = Replace-UniqueLiteral -Value $content -Anchor "!include MUI2.nsh" -Replacement "!include MUI2.nsh$newline$includePatch"
$content = Replace-UniqueLiteral -Value $content -Anchor "Var OldMainBinaryName" -Replacement "Var OldMainBinaryName$newline$variablePatch"
$content = Replace-UniqueLiteral -Value $content -Anchor "; 7. Installation page" -Replacement "$pagePatch$newline$newline; 7. Installation page"
$content = Replace-UniqueLiteral -Value $content -Anchor "Function .onInit" -Replacement "$functionPatch$newline$newline`Function .onInit"
$content = Replace-UniqueLiteral -Value $content -Anchor "  ; Create file associations" -Replacement "  ; Create file associations$newline$installPatch"
$content = Replace-UniqueLiteral -Value $content -Anchor "  ; Delete app associations" -Replacement "  ; Delete app associations$newline$uninstallPatch"

foreach ($fragment in $expectedFragments) {
  Assert-UniqueLiteral -Value $content -Literal $fragment -Kind "patched fragment"
}

$utf8WithBom = New-Object System.Text.UTF8Encoding($true)
[System.IO.File]::WriteAllText($InstallerScript, $content, $utf8WithBom)
Write-Host "Patched NSIS installer: $InstallerScript"
