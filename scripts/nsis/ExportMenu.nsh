; Current-user static cascade. Never delete an entire shared Shell tree.
!define MARKLITE_EXPORT_PARENT "Software\Classes\SystemFileAssociations\.md\shell\MarkLite.Export"
!define MARKLITE_EXPORT_COMMANDS "Software\Classes\MarkLite.ExportCommands"

!macro MarkLiteRemoveExportVerb KEY FORMAT OWNER
  ReadRegStr $0 HKCU "${KEY}" "MarkLiteInstallDir"
  ReadRegStr $1 HKCU "${KEY}\command" ""
  ${If} $0 != ""
  ${AndIf} $0 == "${OWNER}"
  ${AndIf} $1 == "$\"$0\${MAINBINARYNAME}.exe$\" --marklite-shell-export --input $\"%1$\" --format ${FORMAT}"
    DeleteRegValue HKCU "${KEY}\command" ""
    DeleteRegKey /ifempty HKCU "${KEY}\command"
    DeleteRegValue HKCU "${KEY}" ""
    DeleteRegValue HKCU "${KEY}" "MUIVerb"
    DeleteRegValue HKCU "${KEY}" "Icon"
    DeleteRegValue HKCU "${KEY}" "MultiSelectModel"
    DeleteRegValue HKCU "${KEY}" "MarkLiteInstallDir"
    DeleteRegKey /ifempty HKCU "${KEY}"
  ${EndIf}
!macroend

!macro MarkLiteRemoveExportMenu OWNER
  !insertmacro MarkLiteRemoveExportVerb "${MARKLITE_EXPORT_COMMANDS}\shell\pdf" "pdf" "${OWNER}"
  !insertmacro MarkLiteRemoveExportVerb "${MARKLITE_EXPORT_COMMANDS}\shell\docx" "docx" "${OWNER}"
  !insertmacro MarkLiteRemoveExportVerb "${MARKLITE_EXPORT_COMMANDS}\shell\html" "html" "${OWNER}"
  ReadRegStr $0 HKCU "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir"
  ReadRegStr $1 HKCU "${MARKLITE_EXPORT_PARENT}" "ExtendedSubCommandsKey"
  ${If} $0 != ""
  ${AndIf} $0 == "${OWNER}"
  ${AndIf} $1 == "MarkLite.ExportCommands"
    DeleteRegValue HKCU "${MARKLITE_EXPORT_PARENT}" "MUIVerb"
    DeleteRegValue HKCU "${MARKLITE_EXPORT_PARENT}" "Icon"
    DeleteRegValue HKCU "${MARKLITE_EXPORT_PARENT}" "MultiSelectModel"
    DeleteRegValue HKCU "${MARKLITE_EXPORT_PARENT}" "ExtendedSubCommandsKey"
    DeleteRegValue HKCU "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir"
    DeleteRegKey /ifempty HKCU "${MARKLITE_EXPORT_PARENT}"
  ${EndIf}
  ReadRegStr $0 HKCU "${MARKLITE_EXPORT_COMMANDS}" "MarkLiteInstallDir"
  ${If} $0 != ""
  ${AndIf} $0 == "${OWNER}"
    DeleteRegValue HKCU "${MARKLITE_EXPORT_COMMANDS}" "MarkLiteInstallDir"
    DeleteRegKey /ifempty HKCU "${MARKLITE_EXPORT_COMMANDS}\shell"
    DeleteRegKey /ifempty HKCU "${MARKLITE_EXPORT_COMMANDS}"
  ${EndIf}
!macroend

!macro MarkLiteMigrateLegacyExport NAME FORMAT
  ReadRegStr $R8 HKCU "Software\Classes\SystemFileAssociations\.md\shell\MarkLite.Export${NAME}" "MarkLiteInstallDir"
  !insertmacro MarkLiteRemoveExportVerb "Software\Classes\SystemFileAssociations\.md\shell\MarkLite.Export${NAME}" "${FORMAT}" "$R8"
!macroend

Function MigrateMarkLiteExportMenus
  ; An owner plus its exact command proves membership in the upgrade chain.
  !insertmacro MarkLiteMigrateLegacyExport "Pdf" "pdf"
  !insertmacro MarkLiteMigrateLegacyExport "Docx" "docx"
  !insertmacro MarkLiteMigrateLegacyExport "Html" "html"
  ReadRegStr $R8 HKCU "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir"
  !insertmacro MarkLiteRemoveExportMenu "$R8"
FunctionEnd

!macro MarkLiteRegisterExportVerb FORMAT LABEL
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}\shell\${FORMAT}" "MUIVerb" "${LABEL}"
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}\shell\${FORMAT}" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}\shell\${FORMAT}" "MultiSelectModel" "Single"
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}\shell\${FORMAT}" "MarkLiteInstallDir" "$INSTDIR"
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}\shell\${FORMAT}\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" --marklite-shell-export --input $\"%1$\" --format ${FORMAT}"
!macroend

Function RegisterMarkLiteExportMenu
  WriteRegStr HKCU "${MARKLITE_EXPORT_COMMANDS}" "MarkLiteInstallDir" "$INSTDIR"
  !insertmacro MarkLiteRegisterExportVerb "pdf" "PDF"
  !insertmacro MarkLiteRegisterExportVerb "docx" "Word (.docx)"
  !insertmacro MarkLiteRegisterExportVerb "html" "HTML"
  WriteRegStr HKCU "${MARKLITE_EXPORT_PARENT}" "MUIVerb" "Convert with MarkLite"
  WriteRegStr HKCU "${MARKLITE_EXPORT_PARENT}" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "${MARKLITE_EXPORT_PARENT}" "MultiSelectModel" "Single"
  WriteRegStr HKCU "${MARKLITE_EXPORT_PARENT}" "MarkLiteInstallDir" "$INSTDIR"
  WriteRegStr HKCU "${MARKLITE_EXPORT_PARENT}" "ExtendedSubCommandsKey" "MarkLite.ExportCommands"
FunctionEnd

Function un.UnregisterMarkLiteExportMenu
  !insertmacro MarkLiteRemoveExportMenu "$INSTDIR"
  !insertmacro MarkLiteRemoveExportVerb "Software\Classes\SystemFileAssociations\.md\shell\MarkLite.ExportPdf" "pdf" "$INSTDIR"
  !insertmacro MarkLiteRemoveExportVerb "Software\Classes\SystemFileAssociations\.md\shell\MarkLite.ExportDocx" "docx" "$INSTDIR"
  !insertmacro MarkLiteRemoveExportVerb "Software\Classes\SystemFileAssociations\.md\shell\MarkLite.ExportHtml" "html" "$INSTDIR"
FunctionEnd

!macro MarkLiteRemoveOpenMenu EXT
  ReadRegStr $0 HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\MarkLite\command" ""
  ${If} $0 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  ${OrIf} $0 == "$\"$R8\${MAINBINARYNAME}.exe$\" $\"%1$\""
    DeleteRegValue HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\MarkLite\command" ""
    DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\MarkLite\command"
    DeleteRegValue HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\MarkLite" ""
    DeleteRegValue HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\MarkLite" "Icon"
    DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\MarkLite"
  ${EndIf}
!macroend

Function RemoveMarkLiteOpenMenus
  !insertmacro MarkLiteRemoveOpenMenu ".md"
  !insertmacro MarkLiteRemoveOpenMenu ".markdown"
  !insertmacro MarkLiteRemoveOpenMenu ".txt"
  System::Call 'Shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
FunctionEnd
