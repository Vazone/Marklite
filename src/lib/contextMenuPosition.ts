export type ContextMenuPosition = {
  x: number;
  y: number;
  width: number;
  maxHeight: number;
};

export function clampContextMenuPosition(
  x: number,
  y: number,
  menuWidth: number,
  menuHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  padding = 8
): ContextMenuPosition {
  const horizontalPadding = Math.min(padding, Math.max(0, viewportWidth) / 2);
  const verticalPadding = Math.min(padding, Math.max(0, viewportHeight) / 2);
  const width = Math.min(menuWidth, Math.max(0, viewportWidth - horizontalPadding * 2));
  const maxHeight = Math.min(menuHeight, Math.max(0, viewportHeight - verticalPadding * 2));
  const maximumX = viewportWidth - horizontalPadding - width;
  const maximumY = viewportHeight - verticalPadding - maxHeight;

  return {
    x: Math.max(horizontalPadding, Math.min(x, maximumX)),
    y: Math.max(verticalPadding, Math.min(y, maximumY)),
    width,
    maxHeight
  };
}
