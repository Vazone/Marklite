export type ModalState = {
  settingsOpen: boolean;
  commandPaletteOpen: boolean;
  aboutOpen: boolean;
  markdownGuideOpen: boolean;
  exitConfirmationOpen: boolean;
  exportDialogOpen: boolean;
};

export function shouldHandleGlobalShortcut(event: KeyboardEvent, modalState: ModalState): boolean {
  if (event.defaultPrevented || Object.values(modalState).some(Boolean)) return false;
  const target = event.target;
  if (!(target instanceof Element)) return true;
  if (target.closest('input, textarea, select, [role="dialog"], [role="alertdialog"]')) return false;

  const editable = target.closest('[contenteditable="true"]');
  if (!editable) return true;

  return Boolean(target.closest('[data-marklite-application-shortcuts="true"]'));
}
