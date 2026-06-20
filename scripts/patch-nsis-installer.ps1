param(
  [string]$InstallerScript = "$PSScriptRoot\..\src-tauri\target\release\nsis\x64\installer.nsi"
)

$ErrorActionPreference = "Stop"
$InstallerScript = (Resolve-Path $InstallerScript).Path
$content = Get-Content -Raw -Encoding UTF8 $InstallerScript

function Decode-Utf8Base64([string]$Value) {
  [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($Value))
}

if ($content.Contains("MARKLITE_INTEGRATION_PATCH")) {
  Write-Host "NSIS installer already contains MarkLite integration options."
  exit 0
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
$defaultMarkdownLabel = Decode-Utf8Base64 "U2V0IE1hcmtMaXRlIGFzIHRoZSBkZWZhdWx0IGFwcCBmb3IgTWFya2Rvd24gZmlsZXMgKC5tZCwgLm1hcmtkb3duKS4gV2luZG93cyBtYXkgc3RpbGwgYXNrIGZvciBjb25maXJtYXRpb24gaW4gRGVmYXVsdCBBcHBzLiAvIOiuvuS4uiBNYXJrZG93biDpu5jorqTmiZPlvIDmlrnlvI8="

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

$content = $content.Replace("!include MUI2.nsh", "!include MUI2.nsh`r`n$includePatch")
$content = $content.Replace("Var OldMainBinaryName`r`n", "Var OldMainBinaryName`r`n$variablePatch")
$content = $content.Replace("; 7. Installation page", "$pagePatch; 7. Installation page")
$content = $content.Replace("Function .onInit", "$functionPatch`r`nFunction .onInit")
$content = $content.Replace("  ; Create file associations`r`n", "  ; Create file associations`r`n$installPatch")
$content = $content.Replace("  ; Delete app associations`r`n", "  ; Delete app associations`r`n$uninstallPatch")

Set-Content -Path $InstallerScript -Value $content -Encoding UTF8
Write-Host "Patched NSIS installer: $InstallerScript"
