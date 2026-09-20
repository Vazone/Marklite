import { describe, expect, test, vi } from 'vitest';
import { measurePreviewChunks, rescalePreviewChunks, wrapPreviewChunks } from './previewChunks';

describe('preview chunks', () => {
  test('keeps every article block in order and scales the scroll extent from visible geometry', () => {
    const template = document.createElement('template');
    template.innerHTML = Array.from({ length: 9_300 }, (_, index) => `<p id="block-${index}">Block ${index}</p>`).join('');
    const originalText = template.content.textContent;
    const chunks = wrapPreviewChunks(template.content);
    const host = document.createElement('section');
    host.append(template.content);

    expect(chunks.leaves).toHaveLength(25);
    expect(chunks.groups).toHaveLength(2);
    expect(host.querySelectorAll('p')).toHaveLength(9_300);
    expect(host.textContent).toBe(originalText);
    expect(host.querySelector('#block-9299')?.textContent).toBe('Block 9299');

    let width = 500;
    Object.defineProperty(host, 'clientWidth', { configurable: true, get: () => width });
    chunks.leaves.forEach((leaf) => vi.spyOn(leaf, 'getBoundingClientRect').mockReturnValue({ height: 100 } as DOMRect));
    chunks.groups.forEach((group) => vi.spyOn(group, 'getBoundingClientRect').mockReturnValue({ height: 2_400 } as DOMRect));
    measurePreviewChunks(host, chunks);
    expect(chunks.leaves[0].style.contentVisibility).toBe('auto');
    expect(chunks.groups[0].style.contentVisibility).toBe('auto');
    chunks.seenGroups.add(chunks.groups[0]);

    width = 700;
    vi.mocked(chunks.leaves[0].getBoundingClientRect).mockReturnValue({ height: 80 } as DOMRect);
    const previousElementFromPoint = document.elementFromPoint;
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: () => chunks.leaves[0].firstElementChild
    });
    try {
      expect(rescalePreviewChunks(host, chunks)).toBe(true);
      expect(host.style.getPropertyValue('--preview-group-scale')).toBe('0.8');
      expect(chunks.groups[0].style.getPropertyValue('--preview-leaf-scale')).toBe('0.8');
      expect(chunks.groups[0].style.contentVisibility).toBe('auto');
      expect(rescalePreviewChunks(host, chunks)).toBe(false);
    } finally {
      Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: previousElementFromPoint });
    }
  });
});
