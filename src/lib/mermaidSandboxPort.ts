import type { DiagramSource } from './platform/contracts';
import { normalizeMermaidSvgDocument } from '../shared/mermaidSvgNormalization.mjs';
import type {
  DiagramExecution,
  DiagramExecutionPort,
  DiagramRenderOptions,
  UntrustedRenderedDiagram
} from './diagramRenderer';

export type MermaidRuntimeAsset = {
  rendererId: 'mermaid-offline-11.17.2';
  scriptUtf8: string;
};

export type MermaidRuntimeLoader = () => Promise<MermaidRuntimeAsset>;

type PendingRender = {
  resolve(value: UntrustedRenderedDiagram): void;
  reject(error: Error): void;
};

type Surface = {
  frame: HTMLIFrameElement;
  port: MessagePort;
  pending: Map<number, PendingRender>;
};

export class MermaidSandboxPort implements DiagramExecutionPort {
  private surfacePromise: Promise<Surface> | null = null;
  private surface: Surface | null = null;
  private pendingFrame: HTMLIFrameElement | null = null;
  private rejectPendingFrame: ((error: Error) => void) | null = null;
  private surfaceEpoch = 0;
  private requestId = 0;

  constructor(
    private readonly loadRuntime: MermaidRuntimeLoader,
    private readonly documentRoot: Document = document
  ) {}

  start(source: DiagramSource, options: DiagramRenderOptions): DiagramExecution {
    let terminated = false;
    let rejectExecution!: (error: Error) => void;
    const result = new Promise<UntrustedRenderedDiagram>((resolve, reject) => {
      rejectExecution = reject;
      void this.ensureSurface()
        .then((surface) => {
          if (terminated) {
            this.destroySurface(new Error('Mermaid sandbox terminated before it became ready'));
            return;
          }
          const requestId = ++this.requestId;
          surface.pending.set(requestId, { resolve, reject });
          surface.port.postMessage({
            type: 'render',
            requestId,
            source: source.sourceUtf8,
            diagramId: source.diagramId,
            sourceSha256: source.sourceSha256,
            theme: options.theme,
            fontFamily: options.fontFamily,
            fontSize: options.fontSize,
            lineHeight: options.lineHeight
          });
        })
        .catch(reject);
    });
    return {
      result,
      terminate: (reason) => {
        if (terminated) return;
        terminated = true;
        rejectExecution(new Error(`Mermaid sandbox terminated: ${reason}`));
        this.destroySurface(new Error(`Mermaid sandbox terminated: ${reason}`));
      }
    };
  }

  dispose(): void {
    this.destroySurface(new Error('Mermaid sandbox disposed'));
  }

  release(): void {
    this.destroySurface(new Error('Mermaid sandbox released'));
  }

  private ensureSurface(): Promise<Surface> {
    if (this.surface) return Promise.resolve(this.surface);
    if (this.surfacePromise) return this.surfacePromise;
    const epoch = this.surfaceEpoch;
    this.surfacePromise = this.loadRuntime()
      .then((asset) => {
        if (epoch !== this.surfaceEpoch) throw new Error('Mermaid sandbox request was released');
        return this.createSurface(asset);
      })
      .then((surface) => {
        if (epoch !== this.surfaceEpoch) {
          surface.port.close();
          surface.frame.remove();
          throw new Error('Mermaid sandbox request was released');
        }
        this.surface = surface;
        this.surfacePromise = null;
        return surface;
      })
      .catch((error) => {
        if (epoch === this.surfaceEpoch) this.surfacePromise = null;
        throw error;
      });
    return this.surfacePromise;
  }

  private createSurface(asset: MermaidRuntimeAsset): Promise<Surface> {
    if (asset.rendererId !== 'mermaid-offline-11.17.2') {
      return Promise.reject(new Error('Unexpected Mermaid renderer identity'));
    }
    return new Promise((resolve, reject) => {
      const frame = this.documentRoot.createElement('iframe');
      let settled = false;
      const clearPendingFrame = () => {
        if (this.pendingFrame === frame) this.pendingFrame = null;
        if (this.rejectPendingFrame === fail) this.rejectPendingFrame = null;
      };
      const fail = (error = new Error('Mermaid sandbox failed to load')) => {
        if (settled) return;
        settled = true;
        clearPendingFrame();
        frame.remove();
        reject(error);
      };
      this.pendingFrame = frame;
      this.rejectPendingFrame = fail;
      frame.style.cssText = 'position:fixed;left:-20000px;top:0;width:1200px;height:800px;opacity:0;pointer-events:none;border:0';
      frame.tabIndex = -1;
      frame.setAttribute('aria-hidden', 'true');
      frame.setAttribute('sandbox', 'allow-scripts');
      frame.srcdoc = buildMermaidSandboxDocument(asset.scriptUtf8);
      frame.addEventListener('error', () => fail(), { once: true });
      frame.addEventListener(
        'load',
        () => {
          if (settled) return;
          const contentWindow = frame.contentWindow;
          if (!contentWindow) {
            fail();
            return;
          }
          const channel = new MessageChannel();
          const pending = new Map<number, PendingRender>();
          const surface: Surface = { frame, port: channel.port1, pending };
          channel.port1.onmessage = (event: MessageEvent<unknown>) => {
            const message = event.data as {
              type?: string;
              requestId?: number;
              value?: UntrustedRenderedDiagram;
              error?: string;
            };
            if (message.type === 'ready' && !settled) {
              settled = true;
              clearPendingFrame();
              resolve(surface);
              return;
            }
            if (!Number.isInteger(message.requestId)) return;
            const request = pending.get(Number(message.requestId));
            if (!request) return;
            pending.delete(Number(message.requestId));
            if (message.type === 'rendered' && message.value) request.resolve(message.value);
            else request.reject(new Error(message.error ?? 'Mermaid sandbox failed'));
          };
          channel.port1.start();
          contentWindow.postMessage({ type: 'marklite-mermaid-connect' }, '*', [channel.port2]);
        },
        { once: true }
      );
      this.documentRoot.body.append(frame);
    });
  }

  private destroySurface(error: Error): void {
    this.surfaceEpoch += 1;
    const rejectPendingFrame = this.rejectPendingFrame;
    this.rejectPendingFrame = null;
    rejectPendingFrame?.(error);
    this.pendingFrame?.remove();
    this.pendingFrame = null;
    const surface = this.surface;
    this.surface = null;
    this.surfacePromise = null;
    if (!surface) return;
    surface.port.close();
    surface.frame.remove();
    for (const pending of surface.pending.values()) pending.reject(error);
    surface.pending.clear();
  }
}

export function buildMermaidSandboxDocument(runtimeScript: string): string {
  const escapedRuntime = runtimeScript
    .replaceAll('</script', '<\\/script')
    .replaceAll('\u2028', '\\u2028')
    .replaceAll('\u2029', '\\u2029');
  const normalizationSource = normalizeMermaidSvgDocument.toString();
  return `<!doctype html>
<html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; connect-src 'none'; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'"></head>
<body><script>${escapedRuntime}</script><script>
(() => {
  'use strict';
  let channel = null;
  const normalizeMermaidSvgDocument = ${normalizationSource};
  const initialize = (theme, fontFamily, fontSize) => globalThis.mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'strict',
    theme: theme === 'dark' ? 'dark' : 'default',
    look: 'classic',
    layout: 'dagre',
    htmlLabels: true,
    fontFamily,
    themeVariables: { fontFamily, fontSize: fontSize + 'px' },
    logLevel: 'fatal',
    flowchart: { htmlLabels: true, defaultRenderer: 'dagre-wrapper' }
  });
  addEventListener('message', (event) => {
    if (channel || event.data?.type !== 'marklite-mermaid-connect' || event.ports.length !== 1) return;
    channel = event.ports[0];
    channel.onmessage = async (messageEvent) => {
      const request = messageEvent.data;
      if (request?.type !== 'render' || !Number.isInteger(request.requestId)) return;
      try {
        const requestedFontFamily = typeof request.fontFamily === 'string'
          ? request.fontFamily.trim().replace(/["']/g, '')
          : '';
        const unsafeFontFamily = [...requestedFontFamily].some((character) => {
          const code = character.charCodeAt(0);
          return code < 32 || code === 127 || ';{}<>\\\\()[]:'.includes(character);
        });
        const fontFamily = requestedFontFamily && requestedFontFamily.length <= 200 && !unsafeFontFamily
          ? requestedFontFamily
          : 'Arial, system-ui, sans-serif';
        const fontSize = Number.isFinite(request.fontSize) ? Math.min(28, Math.max(12, request.fontSize)) : 16;
        const lineHeight = Number.isFinite(request.lineHeight) ? Math.min(2.4, Math.max(1, request.lineHeight)) : 1.6;
        document.body.style.fontFamily = fontFamily;
        document.body.style.fontSize = fontSize + 'px';
        document.body.style.lineHeight = String(lineHeight);
        initialize(request.theme, fontFamily, fontSize);
        const rendered = await globalThis.mermaid.render('marklite-diagram-' + request.requestId, request.source);
        const parsed = new DOMParser().parseFromString(rendered.svg, 'image/svg+xml');
        const root = normalizeMermaidSvgDocument(parsed.documentElement);
        const safeSvg = new XMLSerializer().serializeToString(root);
        const viewBox = (root.getAttribute('viewBox') || '').trim().split(/[\\s,]+/).map(Number);
        if (viewBox.length !== 4 || viewBox.some((value) => !Number.isFinite(value))) throw new Error('Mermaid SVG has no finite viewBox');
        channel.postMessage({ type: 'rendered', requestId: request.requestId, value: {
          diagramId: request.diagramId,
          sourceSha256: request.sourceSha256,
          svgUtf8: safeSvg,
          width: viewBox[2],
          height: viewBox[3],
          viewBox,
          accessibleTitle: root.querySelector('title')?.textContent || null,
          accessibleDescription: root.querySelector('desc')?.textContent || null,
          warnings: []
        }});
      } catch (error) {
        channel.postMessage({ type: 'failed', requestId: request.requestId, error: error instanceof Error ? error.message : String(error) });
      }
    };
    channel.start();
    channel.postMessage({ type: 'ready' });
  });
})();
</script></body></html>`;
}
