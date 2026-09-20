import { describe, expect, test, vi } from 'vitest';
import { buildMermaidSandboxDocument, MermaidSandboxPort } from './mermaidSandboxPort';

describe('Mermaid sandbox document', () => {
  test('escapes script terminators and freezes the offline security configuration', () => {
    const html = buildMermaidSandboxDocument('globalThis.marker="</script><script>bad()</script>";');
    expect(html).toContain('<\\/script>');
    expect(html).toContain("default-src 'none'");
    expect(html).toContain("connect-src 'none'");
    expect(html).toContain("securityLevel: 'strict'");
    expect(html).toContain("layout: 'dagre'");
    expect(html).toContain('htmlLabels: true');
    expect(html).toContain('initialize(request.theme, fontFamily, fontSize)');
    expect(html).not.toContain('https://');
    expect(html.replace('http://www.w3.org/2000/svg', '')).not.toContain('http://');
    const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];
    expect(() => new Function(scripts.at(-1)![1])).not.toThrow();
  });

  test('release removes and rejects an iframe that is still waiting for load', async () => {
    const port = new MermaidSandboxPort(async () => ({
      rendererId: 'mermaid-offline-11.17.2',
      scriptUtf8: 'globalThis.mermaid = {}'
    }));
    const execution = port.start({
      diagramId: 'diagram-0-aaaaaaaaaaaa',
      ordinal: 0,
      sourceUtf8: 'flowchart TD\nA-->B',
      sourceSha256: 'a'.repeat(64),
      sourceStartByte: 0,
      sourceEndByte: 18
    }, {
      rendererId: 'mermaid-offline-11.17.2',
      configVersion: 2,
      theme: 'light',
      fontKey: 'Inter\u000016\u00001.5',
      fontFamily: 'Inter',
      fontSize: 16,
      lineHeight: 1.5
    });
    const rejected = expect(execution.result).rejects.toThrow('released');
    await vi.waitFor(() => expect(document.querySelectorAll('iframe')).toHaveLength(1));
    port.release();
    await rejected;
    expect(document.querySelectorAll('iframe')).toHaveLength(0);
  });

  test('release before the pack loads never appends a late iframe', async () => {
    let completeLoad!: (asset: { rendererId: 'mermaid-offline-11.17.2'; scriptUtf8: string }) => void;
    const load = new Promise<{ rendererId: 'mermaid-offline-11.17.2'; scriptUtf8: string }>((resolve) => {
      completeLoad = resolve;
    });
    const port = new MermaidSandboxPort(() => load);
    const execution = port.start({
      diagramId: 'diagram-0-aaaaaaaaaaaa', ordinal: 0, sourceUtf8: 'flowchart TD\nA-->B',
      sourceSha256: 'a'.repeat(64), sourceStartByte: 0, sourceEndByte: 18
    }, {
      rendererId: 'mermaid-offline-11.17.2', configVersion: 2,
      theme: 'light', fontKey: 'Arial\u000016\u00001.6',
      fontFamily: 'Arial', fontSize: 16, lineHeight: 1.6
    });
    const rejected = expect(execution.result).rejects.toThrow('released');
    port.release();
    completeLoad({ rendererId: 'mermaid-offline-11.17.2', scriptUtf8: '' });
    await rejected;
    expect(document.querySelectorAll('iframe')).toHaveLength(0);
  });
});
