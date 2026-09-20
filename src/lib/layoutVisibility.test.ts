import { describe, expect, it } from 'vitest';
import { paneVisibility } from './layoutVisibility';

describe('effective pane visibility', () => {
  it('does not mount hidden split-preview work in a narrow viewport', () => {
    expect(paneVisibility('split', true)).toEqual({
      editor: true,
      preview: false,
      separator: false
    });
  });

  it('keeps explicit preview mode available and restores desktop split', () => {
    expect(paneVisibility('preview', true).preview).toBe(true);
    expect(paneVisibility('split', false)).toEqual({
      editor: true,
      preview: true,
      separator: true
    });
  });
});
