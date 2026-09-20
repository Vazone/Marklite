/// <reference types="node" />

import { readFileSync } from 'node:fs';
import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import MindMapPane from './MindMapPane.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;
let previewStyle: HTMLStyleElement;
const previewCss = readFileSync('src/styles/markdown-preview.css', 'utf8');

class ResizeObserverMock {
  static instances: ResizeObserverMock[] = [];
  private readonly callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    ResizeObserverMock.instances.push(this);
  }

  observe() {}
  unobserve() {}
  disconnect() {}

  trigger(target: Element) {
    this.callback([{ target } as ResizeObserverEntry], this as unknown as ResizeObserver);
  }
}

function pointerEvent(type: string, clientX: number, clientY: number, pointerId = 1) {
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX,
    clientY,
    button: 0
  });
  Object.defineProperty(event, 'pointerId', { value: pointerId });
  return event;
}

beforeEach(() => {
  ResizeObserverMock.instances = [];
  vi.stubGlobal('ResizeObserver', ResizeObserverMock);
  previewStyle = document.createElement('style');
  previewStyle.textContent = previewCss;
  document.head.append(previewStyle);
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
  previewStyle.remove();
  vi.unstubAllGlobals();
});
describe('MindMapPane', () => {
  test('collapses descendants without editing and jumps through the supplied callback', async () => {
    const onJumpToLine = vi.fn();
    target = document.createElement('div');
    document.body.append(target);
    component = mount(MindMapPane, {
      target,
      props: {
        documentTitle: 'Project.md',
        outline: [
          { level: 1, title: 'Plan', line: 1, slug: 'plan' },
          { level: 2, title: 'Details', line: 3, slug: 'details' }
        ],
        onJumpToLine
      }
    });
    await tick();

    const plan = [...target.querySelectorAll<HTMLButtonElement>('.mind-map-node-label')].find(
      (button) => button.textContent?.includes('Plan')
    )!;
    plan.click();
    expect(onJumpToLine).toHaveBeenCalledWith(1);

    const toggle = target.querySelector<HTMLButtonElement>(
      '[aria-label="Collapse the children of “Plan”"]'
    )!;
    toggle.click();
    await tick();
    expect(target.textContent).not.toContain('Details');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
  });

  test('explains the standard heading syntax for a document without headings', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(MindMapPane, {
      target,
      props: {
        documentTitle: 'Empty.md',
        outline: [],
        onJumpToLine: vi.fn()
      }
    });
    await tick();

    expect(target.textContent).toContain('Use #, ##, and ### in the editor');
    expect(target.querySelector('.mind-map-node')).toBeNull();
  });

  test('measures both dimensions and never line-clamps a complete title', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(MindMapPane, {
      target,
      props: {
        documentTitle: 'Project.md',
        outline: [
          {
            level: 1,
            title: '项目管理机构人员配比与安全责任说明',
            line: 104,
            slug: 'project-team'
          }
        ],
        onJumpToLine: vi.fn()
      }
    });
    await tick();

    const label = [...target.querySelectorAll<HTMLButtonElement>('.mind-map-node-label')]
      .find((button) => button.textContent?.includes('项目管理机构'))!;
    const node = label.closest<HTMLElement>('.mind-map-node')!;
    expect(label.querySelector('small')?.textContent).toBe('Line 104');
    expect(node.style.height).toBe('');
    expect(getComputedStyle(node).width).toBe('max-content');
    expect(getComputedStyle(node).minWidth).toBe('196px');
    expect(getComputedStyle(node).maxWidth).toBe('360px');
    expect(getComputedStyle(node).minHeight).toBe('60px');
    const title = label.querySelector('span')!;
    expect(getComputedStyle(title).overflow).not.toBe('hidden');
    expect(getComputedStyle(title).getPropertyValue('-webkit-line-clamp')).toBe('');

    Object.defineProperty(node, 'offsetWidth', { configurable: true, value: 328 });
    Object.defineProperty(node, 'offsetHeight', { configurable: true, value: 88 });
    ResizeObserverMock.instances[0].trigger(node);
    await tick();
    expect(node.dataset.layoutWidth).toBe('328');
    expect(node.dataset.layoutHeight).toBe('88');
  });

  test('pans the overflowing canvas with primary-button pointer capture without stealing node controls', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(MindMapPane, {
      target,
      props: {
        documentTitle: 'Project.md',
        outline: [{ level: 1, title: 'Plan', line: 1, slug: 'plan' }],
        onJumpToLine: vi.fn()
      }
    });
    await tick();

    const viewport = target.querySelector<HTMLElement>('.mind-map-viewport')!;
    const setPointerCapture = vi.fn();
    const releasePointerCapture = vi.fn();
    Object.defineProperties(viewport, {
      setPointerCapture: { configurable: true, value: setPointerCapture },
      hasPointerCapture: { configurable: true, value: () => true },
      releasePointerCapture: { configurable: true, value: releasePointerCapture }
    });
    viewport.scrollLeft = 200;
    viewport.scrollTop = 120;

    viewport.dispatchEvent(pointerEvent('pointerdown', 100, 100, 7));
    viewport.dispatchEvent(pointerEvent('pointermove', 70, 55, 7));
    await tick();
    expect(viewport.scrollLeft).toBe(230);
    expect(viewport.scrollTop).toBe(165);
    expect(viewport.classList.contains('dragging')).toBe(true);
    expect(setPointerCapture).toHaveBeenCalledWith(7);

    viewport.dispatchEvent(pointerEvent('pointerup', 70, 55, 7));
    await tick();
    expect(viewport.classList.contains('dragging')).toBe(false);
    expect(releasePointerCapture).toHaveBeenCalledWith(7);

    target.querySelector<HTMLButtonElement>('.mind-map-node-label')!
      .dispatchEvent(pointerEvent('pointerdown', 20, 20, 8));
    expect(setPointerCapture).toHaveBeenCalledTimes(1);
  });

  test('keeps a 150,000-heading canvas scrollable while mounting only viewport nodes', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(MindMapPane, {
      target,
      props: {
        documentTitle: 'Large.md',
        outline: Array.from({ length: 150_000 }, (_, index) => ({
          level: 1,
          title: `Heading ${index + 1}`,
          line: index + 1,
          slug: `heading-${index + 1}`
        })),
        onJumpToLine: vi.fn()
      }
    });
    await tick();

    const canvas = target.querySelector<HTMLElement>('.mind-map-canvas')!;
    const viewport = target.querySelector<HTMLElement>('.mind-map-viewport')!;
    expect(canvas.dataset.totalLayoutNodes).toBe('150001');
    expect(Number(canvas.dataset.renderedLayoutNodes)).toBeLessThan(30);

    viewport.scrollTop = Number.parseFloat(canvas.style.height) - 800;
    viewport.dispatchEvent(new Event('scroll'));
    await vi.waitFor(() => expect(target.textContent).toContain('Heading 150000'));
    expect(Number(canvas.dataset.renderedLayoutNodes)).toBeLessThan(30);
  }, 30_000);

  test('shows the explicit layout limit instead of silently truncating headings', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(MindMapPane, {
      target,
      props: {
        documentTitle: 'Too-large.md',
        outline: Array.from({ length: 200_001 }, (_, index) => ({
          level: 1,
          title: `Heading ${index + 1}`,
          line: index + 1,
          slug: `heading-${index + 1}`
        })),
        onJumpToLine: vi.fn()
      }
    });
    await tick();

    expect(target.querySelector('[role="alert"]')).not.toBeNull();
    expect(target.querySelector('.mind-map-canvas')).toBeNull();
    expect(target.textContent).toContain('200000');
  });
});
