import { describe, expect, it, vi } from 'vitest';
import { AppInitializationError, createAppInitializationGate } from './appInitialization';

describe('app initialization gate', () => {
  it('does not report ready before App initialization completes', async () => {
    const gate = createAppInitializationGate();
    const ready = vi.fn();
    void gate.wait().then(ready);

    await Promise.resolve();
    expect(ready).not.toHaveBeenCalled();

    gate.succeed();
    await gate.wait();
    expect(ready).toHaveBeenCalledOnce();
  });

  it('uses a fixed failure without leaking the original initialization error', async () => {
    const gate = createAppInitializationGate();
    const waiting = gate.wait();

    gate.fail();

    await expect(waiting).rejects.toBeInstanceOf(AppInitializationError);
    await expect(waiting).rejects.not.toHaveProperty('message', 'C:\\private\\note.md');
    gate.succeed();
    await expect(gate.wait()).rejects.toBeInstanceOf(AppInitializationError);
  });

  it('settles only once', async () => {
    const gate = createAppInitializationGate();
    gate.succeed();
    gate.fail();
    await expect(gate.wait()).resolves.toBeUndefined();
  });
});
