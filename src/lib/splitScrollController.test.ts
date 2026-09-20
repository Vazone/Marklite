import { afterEach, describe, expect, test, vi } from 'vitest';
import { createSplitScrollController, type SplitScrollAnchor } from './splitScrollController';

const position = { line: 1, offsetUtf16: 0, ratio: 0, totalLines: 1, scrollTop: 0, scrollHeight: 10, clientHeight: 10 };

afterEach(() => vi.restoreAllMocks());

describe('split scroll controller', () => {
  test('coalesces frames and discards a previous tab or revision before applying', () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    let nextFrame = 1;
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callbacks.set(nextFrame, callback);
      return nextFrame++;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => { callbacks.delete(id); });
    const flush = () => {
      const queued = [...callbacks.values()];
      callbacks.clear();
      queued.forEach((callback) => callback(0));
    };
    const apply = vi.fn();
    const controller = createSplitScrollController(apply);
    const anchor = (tabId: string, contentRevision: number, ratio: number): SplitScrollAnchor => ({
      tabId, contentRevision, position: { ...position, ratio }
    });

    controller.setActive('first', 1);
    controller.update(anchor('first', 1, 0.2));
    controller.update(anchor('first', 1, 0.4));
    flush();
    expect(apply).toHaveBeenCalledOnce();
    expect(apply).toHaveBeenLastCalledWith(anchor('first', 1, 0.4));

    controller.update(anchor('first', 1, 0.8));
    controller.setActive('first', 2);
    flush();
    expect(apply).toHaveBeenCalledOnce();
    controller.update(anchor('first', 2, 0.6));
    controller.layoutReady();
    controller.clear();
    flush();
    expect(apply).toHaveBeenCalledOnce();
    controller.update(anchor('first', 2, 0.6));
    controller.setActive('second', 0);
    flush();
    expect(apply).toHaveBeenCalledOnce();
    controller.dispose();
  });

  test('holds an anchor while asynchronous layout is pending and replays the latest one', () => {
    const frames = new Map<number, FrameRequestCallback>();
    let nextFrame = 1;
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      frames.set(nextFrame, callback);
      return nextFrame++;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => { frames.delete(id); });
    const apply = vi.fn();
    const controller = createSplitScrollController(apply);
    controller.setActive('tab', 2);
    controller.layoutPending();
    controller.update({ tabId: 'tab', contentRevision: 2, position });
    expect(frames.size).toBe(0);
    expect(apply).not.toHaveBeenCalled();
    controller.layoutReady();
    expect(frames.size).toBe(1);
    for (const callback of frames.values()) callback(0);
    expect(apply).toHaveBeenCalledOnce();
    controller.dispose();
  });
});
