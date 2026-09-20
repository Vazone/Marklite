import { describe, expect, test, vi } from 'vitest';
import type { DiagramSource, RenderedDiagram } from './platform/contracts';
import {
  DIAGRAM_RENDERER_ID,
  DiagramRenderer,
  type DiagramExecution,
  type DiagramExecutionPort,
  type UntrustedRenderedDiagram
} from './diagramRenderer';

function source(index: number, hash = index.toString(16).padStart(64, 'a')): DiagramSource {
  return {
    diagramId: `diagram-${index}-${hash.slice(0, 12)}`,
    ordinal: index,
    sourceUtf8: 'flowchart TD\nA-->B',
    sourceSha256: hash,
    sourceStartByte: index * 20,
    sourceEndByte: index * 20 + 18
  };
}

function output(item: DiagramSource, text = 'safe'): UntrustedRenderedDiagram {
  return {
    diagramId: item.diagramId,
    sourceSha256: item.sourceSha256,
    svgUtf8: `<svg viewBox="0 0 100 50"><text>${text}</text></svg>`,
    width: 100,
    height: 50,
    viewBox: [0, 0, 100, 50],
    accessibleTitle: null,
    accessibleDescription: null,
    warnings: []
  };
}

function controlledPort() {
  const executions: Array<{
    source: DiagramSource;
    resolve(value: UntrustedRenderedDiagram): void;
    reject(error: Error): void;
    terminate: ReturnType<typeof vi.fn>;
  }> = [];
  const release = vi.fn();
  const port: DiagramExecutionPort = {
    start(item): DiagramExecution {
      let resolve!: (value: UntrustedRenderedDiagram) => void;
      let reject!: (error: Error) => void;
      const result = new Promise<UntrustedRenderedDiagram>((ok, fail) => {
        resolve = ok;
        reject = fail;
      });
      const terminate = vi.fn();
      executions.push({ source: item, resolve, reject, terminate });
      return { result, terminate };
    },
    release
  };
  return { port, executions, release };
}

const validate = vi.fn(async (artifact: RenderedDiagram) => artifact);

describe('bounded diagram renderer', () => {
  test('does not initialize a port for documents without diagrams', async () => {
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    await expect(renderer.render([], [], 'light')).resolves.toEqual({ artifacts: [], diagnostics: [] });
    expect(executions).toHaveLength(0);
  });

  test('validates once and reuses an immutable artifact by versioned cache key', async () => {
    validate.mockClear();
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const item = source(0);
    const first = renderer.render([item], [], 'light');
    await vi.waitFor(() => expect(executions).toHaveLength(1));
    executions[0].resolve(output(item));
    const firstResult = await first;
    const secondResult = await renderer.render([item], [], 'light');
    expect(executions).toHaveLength(1);
    expect(validate).toHaveBeenCalledTimes(1);
    expect(secondResult.artifacts[0]).toBe(firstResult.artifacts[0]);
    expect(firstResult.artifacts[0].rendererId).toBe(DIAGRAM_RENDERER_ID);
    expect(renderer.cacheStats()).toEqual({
      entries: 1,
      bytes: new TextEncoder().encode(firstResult.artifacts[0].svgUtf8).byteLength
    });
  });

  test('releases the temporary surface without clearing cache and invalidates on typography', async () => {
    const { port, executions, release } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const item = source(0);
    const first = renderer.render([item], [], 'light', {
      fontFamily: 'Inter',
      fontSize: 16,
      lineHeight: 1.5
    });
    await vi.waitFor(() => expect(executions).toHaveLength(1));
    executions[0].resolve(output(item, 'first'));
    await first;
    renderer.release();
    expect(release).toHaveBeenCalledOnce();
    await expect(renderer.render([item], [], 'light', {
      fontFamily: 'Inter',
      fontSize: 16,
      lineHeight: 1.5
    })).resolves.toMatchObject({ artifacts: [{ diagramId: item.diagramId }] });
    expect(executions).toHaveLength(1);

    const resized = renderer.render([item], [], 'light', {
      fontFamily: 'Inter',
      fontSize: 18,
      lineHeight: 1.5
    });
    await vi.waitFor(() => expect(executions).toHaveLength(2));
    executions[1].resolve(output(item, 'resized'));
    await resized;
    expect(renderer.cacheStats().entries).toBe(2);
  });

  test('reuses the SVG when an unchanged source moves to another diagram ordinal', async () => {
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const first = source(0);
    const rendered = renderer.render([first], [], 'light');
    await vi.waitFor(() => expect(executions).toHaveLength(1));
    executions[0].resolve(output(first, 'moved'));
    await rendered;

    const moved = { ...first, ordinal: 1, diagramId: `diagram-1-${first.sourceSha256.slice(0, 12)}` };
    const reused = await renderer.render([moved], [], 'light');
    expect(executions).toHaveLength(1);
    expect(reused.artifacts[0].diagramId).toBe(moved.diagramId);
    expect(reused.artifacts[0].svgUtf8).toContain('moved');
    expect(renderer.cacheStats().entries).toBe(1);
  });

  test('terminates stale work and rejects its late success', async () => {
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const oldSource = source(0);
    const nextSource = source(1);
    const old = renderer.render([oldSource], [], 'light');
    await vi.waitFor(() => expect(executions).toHaveLength(1));
    const next = renderer.render([nextSource], [], 'dark');
    expect(executions[0].terminate).toHaveBeenCalledWith('cancelled');
    await vi.waitFor(() => expect(executions).toHaveLength(2));
    executions[0].resolve(output(oldSource, 'old'));
    executions[1].resolve(output(nextSource, 'next'));
    await expect(old).rejects.toMatchObject({ code: 'DIAGRAM_CANCELLED' });
    await expect(next).resolves.toMatchObject({ artifacts: [{ diagramId: nextSource.diagramId }] });
  });

  test('terminates a timed out execution and ignores a late completion', async () => {
    vi.useFakeTimers();
    try {
      const { port, executions } = controlledPort();
      const renderer = new DiagramRenderer(port, validate, () => 0, 25, 100);
      const item = source(0);
      const pending = renderer.render([item], [], 'light');
      await vi.waitFor(() => expect(executions).toHaveLength(1));
      const rejected = expect(pending).rejects.toMatchObject({ code: 'DIAGRAM_TIMEOUT' });
      await vi.advanceTimersByTimeAsync(25);
      await rejected;
      expect(executions[0].terminate).toHaveBeenCalledWith('timeout');
      executions[0].resolve(output(item, 'late'));
      await Promise.resolve();
      expect(renderer.cacheStats().entries).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  test('skips sources already rejected by native diagnostics', async () => {
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const item = source(0);
    const diagnostics = [
      {
        code: 'DIAGRAM_UNSUPPORTED_TYPE',
        diagramId: item.diagramId,
        sourceStartByte: item.sourceStartByte,
        sourceEndByte: item.sourceEndByte,
        message: 'unsupported',
        retryable: false
      }
    ];
    await expect(renderer.render([item], diagnostics, 'light')).resolves.toEqual({
      artifacts: [],
      diagnostics
    });
    expect(executions).toHaveLength(0);
  });

  test('rejects a sandbox result for another source before native validation', async () => {
    validate.mockClear();
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const item = source(0);
    const pending = renderer.render([item], [], 'light');
    await vi.waitFor(() => expect(executions).toHaveLength(1));
    executions[0].resolve({ ...output(item), sourceSha256: 'f'.repeat(64) });
    await expect(pending).rejects.toMatchObject({ code: 'DIAGRAM_RUNTIME_CRASHED' });
    expect(validate).not.toHaveBeenCalled();
  });

  test('does not cache a runtime failure and allows the same source to retry', async () => {
    const { port, executions } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    const item = source(0);
    const failed = renderer.render([item], [], 'light');
    await vi.waitFor(() => expect(executions).toHaveLength(1));
    executions[0].reject(new Error('runtime unavailable'));
    await expect(failed).rejects.toMatchObject({ code: 'DIAGRAM_RUNTIME_CRASHED' });
    expect(renderer.cacheStats()).toEqual({ entries: 0, bytes: 0 });

    const retry = renderer.render([item], [], 'light');
    await vi.waitFor(() => expect(executions).toHaveLength(2));
    executions[1].resolve(output(item, 'retry'));
    await expect(retry).resolves.toMatchObject({ artifacts: [{ diagramId: item.diagramId }] });
  });

  test('keeps the shared cache bounded through 100 render and release cycles', async () => {
    const { port, executions, release } = controlledPort();
    const renderer = new DiagramRenderer(port, validate);
    for (let index = 0; index < 100; index += 1) {
      const hash = index.toString(16).padStart(64, '0');
      const item = source(index, hash);
      const pending = renderer.render([item], [], 'light');
      await vi.waitFor(() => expect(executions).toHaveLength(index + 1));
      executions[index].resolve(output(item, `diagram-${index}`));
      await pending;
      renderer.release();
    }
    expect(release).toHaveBeenCalledTimes(100);
    expect(renderer.cacheStats().entries).toBe(64);
    expect(renderer.cacheStats().bytes).toBeLessThanOrEqual(32 * 1024 * 1024);
  }, 20_000);
});
