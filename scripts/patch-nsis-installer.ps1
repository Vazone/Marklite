param(
  [string]$InstallerScript = "$PSScriptRoot\..\src-tauri\target\release\nsis\x64\installer.nsi"
)

$ErrorActionPreference = "Stop"
$InstallerScript = (Resolve-Path $InstallerScript).Path
$content = Get-Content -Raw -Encoding UTF8 $InstallerScript

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

  ${NSD_CreateLabel} 0 0 100% 32u "Choose Windows integration options for MarkLite. / 选择 MarkLite 的 Windows 集成选项。"
  Pop $1

  ${NSD_CreateCheckbox} 0 42u 100% 18u "Add 'Open with MarkLite' to the right-click menu for .md, .markdown and .txt files. / 添加到右键菜单"
  Pop $RegisterContextMenuCheckbox
  ${NSD_SetState} $RegisterContextMenuCheckbox ${BST_CHECKED}

  ${NSD_CreateCheckbox} 0 66u 100% 28u "Set MarkLite as the default app for Markdown files (.md, .markdown). Windows may still ask for confirmation in Default Apps. / 设为 Markdown 默认打开方式"
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
