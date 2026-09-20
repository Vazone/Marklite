$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$caseId = 'marklite-menu-' + [Guid]::NewGuid().ToString('N')
$caseRoot = Join-Path $root "tmp/$caseId"
$registryRoot = "Software\MarkLiteTests\$caseId"
New-Item -ItemType Directory -Path $caseRoot | Out-Null
$moduleText = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'nsis/ExportMenu.nsh')).Replace('Software\Classes', $registryRoot)
$fixture = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
SilentUnInstall silent
Name "MarkLite menu regression"
OutFile "__EXE__"
!include LogicLib.nsh
!define MAINBINARYNAME "marklite"
__MODULE__
Var Log
!macro Check KEY NAME VALUE LABEL
  ReadRegStr $2 HKCU "${KEY}" "${NAME}"
  ${If} $2 != "${VALUE}"
    FileWrite $Log "FAIL ${LABEL}: $2$\r$\n"
    FileClose $Log
    SetErrorLevel 1
    Quit
  ${EndIf}
  FileWrite $Log "PASS ${LABEL}$\r$\n"
!macroend
!macro SeedLegacy NAME FORMAT
  WriteRegStr HKCU "__REG__\SystemFileAssociations\.md\shell\MarkLite.Export${NAME}" "MarkLiteInstallDir" "$INSTDIR"
  WriteRegStr HKCU "__REG__\SystemFileAssociations\.md\shell\MarkLite.Export${NAME}\command" "" "$\"$INSTDIR\marklite.exe$\" --marklite-shell-export --input $\"%1$\" --format ${FORMAT}"
!macroend
Section
  SetRegView 64
  FileOpen $Log "__LOG__" w
  StrCpy $INSTDIR "C:\MarkLite A"
  !insertmacro SeedLegacy "Pdf" "pdf"
  !insertmacro SeedLegacy "Docx" "docx"
  !insertmacro SeedLegacy "Html" "html"
  WriteRegStr HKCU "__REG__\.md" "" "OtherApp.md"
  Call MigrateMarkLiteExportMenus
  Call RegisterMarkLiteExportMenu
  !insertmacro Check "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir" "$INSTDIR" "legacy upgrade owner"
  !insertmacro Check "${MARKLITE_EXPORT_PARENT}" "ExtendedSubCommandsKey" "MarkLite.ExportCommands" "cascade link"
  !insertmacro Check "__REG__\SystemFileAssociations\.md\shell\MarkLite.ExportPdf" "MarkLiteInstallDir" "" "legacy PDF removed"
  !insertmacro Check "__REG__\SystemFileAssociations\.md\shell\MarkLite.ExportDocx" "MarkLiteInstallDir" "" "legacy DOCX removed"
  !insertmacro Check "__REG__\SystemFileAssociations\.md\shell\MarkLite.ExportHtml" "MarkLiteInstallDir" "" "legacy HTML removed"
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}\shell\ThirdParty" "Custom" "keep"
  Call MigrateMarkLiteExportMenus
  Call RegisterMarkLiteExportMenu
  !insertmacro Check "${MARKLITE_EXPORT_COMMANDS}\shell\ThirdParty" "Custom" "keep" "same-path reinstall preserves extension"
  StrCpy $INSTDIR "C:\MarkLite B"
  Call MigrateMarkLiteExportMenus
  Call RegisterMarkLiteExportMenu
  !insertmacro Check "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir" "$INSTDIR" "changed-path owner"
  !insertmacro Check "${MARKLITE_EXPORT_COMMANDS}\shell\pdf\command" "" "$\"C:\MarkLite B\marklite.exe$\" --marklite-shell-export --input $\"%1$\" --format pdf" "changed-path command"
  WriteUninstaller "__UNINSTALLER__"
  ExecWait '"__UNINSTALLER__" /S _?=C:\MarkLite A' $3
  !insertmacro Check "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir" "C:\MarkLite B" "late A uninstaller preserves B"
  ExecWait '"__UNINSTALLER__" /S _?=C:\MarkLite B' $3
  !insertmacro Check "${MARKLITE_EXPORT_PARENT}" "ExtendedSubCommandsKey" "" "B uninstaller removes parent"
  !insertmacro Check "${MARKLITE_EXPORT_COMMANDS}\shell\pdf\command" "" "" "B uninstaller removes child"
  !insertmacro Check "${MARKLITE_EXPORT_COMMANDS}\shell\ThirdParty" "Custom" "keep" "uninstall preserves extension"
  !insertmacro Check "__REG__\.md" "" "OtherApp.md" "default app preserved"
  Call RegisterMarkLiteExportMenu
  Call MigrateMarkLiteExportMenus
  Call RemoveMarkLiteOpenMenus
  !insertmacro Check "${MARKLITE_EXPORT_PARENT}" "ExtendedSubCommandsKey" "" "unchecked reinstall removes cascade"
  WriteRegStr HKCU "__REG__\SystemFileAssociations\.md\shell\MarkLite.ExportPdf" "MarkLiteInstallDir" "C:\Foreign"
  WriteRegStr HKCU "__REG__\SystemFileAssociations\.md\shell\MarkLite.ExportPdf\command" "" "foreign.exe %1"
  Call MigrateMarkLiteExportMenus
  !insertmacro Check "__REG__\SystemFileAssociations\.md\shell\MarkLite.ExportPdf\command" "" "foreign.exe %1" "foreign legacy command preserved"
  FileClose $Log
SectionEnd
Section Uninstall
  SetRegView 64
  Call un.UnregisterMarkLiteExportMenu
SectionEnd
'@
$exe = Join-Path $caseRoot 'fixture.exe'
$log = Join-Path $caseRoot 'results.txt'
$fixture = $fixture.Replace('__MODULE__', $moduleText).Replace('__REG__', $registryRoot).Replace('__EXE__', $exe).Replace('__LOG__', $log).Replace('__UNINSTALLER__', (Join-Path $caseRoot 'uninstall.exe'))
$source = Join-Path $caseRoot 'fixture.nsi'
[IO.File]::WriteAllText($source, $fixture, [Text.UTF8Encoding]::new($true))
$compiler = Join-Path $env:LOCALAPPDATA 'tauri/NSIS/Bin/makensis.exe'
try {
  & $compiler /V2 $source
  if ($LASTEXITCODE -ne 0) { throw 'Menu fixture compile failed' }
  $process = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
  if (-not $process.WaitForExit(30000)) { Stop-Process -Id $process.Id; throw 'Menu fixture timed out' }
  $results = Get-Content -LiteralPath $log
  $results | Write-Output
  if ($process.ExitCode -ne 0 -or @($results | Where-Object { $_ -like 'PASS *' }).Count -ne 15) { throw 'Menu runtime assertions failed' }
  Write-Output 'Isolated registry runtime assertions passed: 15'
} finally {
  # Delete only this invocation's exact random test key, never Classes or .md.
  $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\MarkLiteTests', $true)
  if ($null -ne $key) { try { $key.DeleteSubKeyTree($caseId, $false) } finally { $key.Dispose() } }
}
