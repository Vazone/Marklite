import type {
  DiagramDiagnostic,
  DiagramSource,
  DiagramTheme,
  RenderedDiagram
} from './platform/contracts';

export const DIAGRAM_RENDERER_ID = 'mermaid-offline-11.17.2' as const;
export const DIAGRAM_CONFIG_VERSION = 2;
export const DIAGRAM_DOCUMENT_DEADLINE_MS = 30_000;
export const DIAGRAM_ITEM_DEADLINE_MS = 5_000;
export const DIAGRAM_CACHE_ENTRIES = 64;
export const DIAGRAM_CACHE_BYTES = 32 * 1024 * 1024;

export type DiagramRenderOptions = {
  rendererId: typeof DIAGRAM_RENDERER_ID;
  theme: DiagramTheme;
  fontKey: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  configVersion: typeof DIAGRAM_CONFIG_VERSION;
};

export type DiagramPresentation = {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
};

export type UntrustedRenderedDiagram = Omit<RenderedDiagram, 'cacheKey' | 'rendererId'>;

export type DiagramExecution = {
  result: Promise<UntrustedRenderedDiagram>;
  terminate(reason: 'cancelled' | 'timeout' | 'disposed'): void;
};

export type DiagramExecutionPort = {
  start(source: DiagramSource, options: DiagramRenderOptions): DiagramExecution;
  release?(): void;
  dispose?(): void;
};

export type DiagramArtifactValidator = (diagram: RenderedDiagram) => Promise<RenderedDiagram>;

export type DiagramBatch = {
  artifacts: RenderedDiagram[];
  diagnostics: DiagramDiagnostic[];
};

type ActiveExecution = {
  token: number;
  terminate(reason: 'cancelled' | 'timeout' | 'disposed'): void;
};

export class DiagramRenderError extends Error {
  constructor(
    readonly code: 'DIAGRAM_TIMEOUT' | 'DIAGRAM_CANCELLED' | 'DIAGRAM_RUNTIME_CRASHED',
    message: string
  ) {
    super(message);
  }
}

export class DiagramRenderer {
  private token = 0;
  private active: ActiveExecution | null = null;
  private disposed = false;
  private cache = new Map<string, { artifact: RenderedDiagram; bytes: number }>();
  private cacheBytes = 0;

  constructor(
    private readonly port: DiagramExecutionPort,
    private readonly validate: DiagramArtifactValidator,
    private readonly now: () => number = () => performance.now(),
    private readonly itemDeadlineMs = DIAGRAM_ITEM_DEADLINE_MS,
    private readonly documentDeadlineMs = DIAGRAM_DOCUMENT_DEADLINE_MS
  ) {}

  async render(
    sources: readonly DiagramSource[],
    diagnostics: readonly DiagramDiagnostic[],
    theme: DiagramTheme,
    presentation: DiagramPresentation = {
      fontFamily: 'Arial, system-ui, sans-serif',
      fontSize: 16,
      lineHeight: 1.6
    }
  ): Promise<DiagramBatch> {
    if (this.disposed) throw new DiagramRenderError('DIAGRAM_CANCELLED', 'Diagram renderer is disposed');
    this.cancelActive('cancelled');
    if (sources.length === 0) return { artifacts: [], diagnostics: [...diagnostics] };

    const token = ++this.token;
    const started = this.now();
    const rejectedIds = new Set(diagnostics.map((diagnostic) => diagnostic.diagramId));
    const artifacts: RenderedDiagram[] = [];
    const normalizedPresentation = normalizeDiagramPresentation(presentation);
    const options: DiagramRenderOptions = {
      rendererId: DIAGRAM_RENDERER_ID,
      theme,
      fontKey: [normalizedPresentation.fontFamily, normalizedPresentation.fontSize, normalizedPresentation.lineHeight].join('\0'),
      ...normalizedPresentation,
      configVersion: DIAGRAM_CONFIG_VERSION
    };

    for (const source of sources) {
      this.assertCurrent(token);
      if (rejectedIds.has(source.diagramId)) continue;
      if (this.now() - started >= this.documentDeadlineMs) {
        this.cancelActive('timeout');
        throw new DiagramRenderError('DIAGRAM_TIMEOUT', 'Diagram document deadline exceeded');
      }
      const cacheKey = await diagramCacheKey(source.sourceSha256, options);
      const cached = this.readCache(cacheKey);
      if (cached) {
        artifacts.push(cached.diagramId === source.diagramId
          ? cached
          : { ...cached, diagramId: source.diagramId, sourceSha256: source.sourceSha256 });
        continue;
      }

      const execution = this.port.start(source, options);
      this.active = { token, terminate: execution.terminate };
      let timeoutId: ReturnType<typeof setTimeout> | null = null;
      try {
        const remaining = Math.max(1, this.documentDeadlineMs - (this.now() - started));
        const timeout = Math.min(this.itemDeadlineMs, remaining);
        const untrusted = await Promise.race([
          execution.result,
          new Promise<never>((_, reject) => {
            timeoutId = setTimeout(() => {
              if (this.active?.token === token) {
                execution.terminate('timeout');
                this.active = null;
              }
              reject(new DiagramRenderError('DIAGRAM_TIMEOUT', 'Diagram item deadline exceeded'));
            }, timeout);
          })
        ]);
        this.assertCurrent(token);
        if (
          untrusted.diagramId !== source.diagramId ||
          untrusted.sourceSha256 !== source.sourceSha256
        ) {
          throw new Error('Mermaid sandbox returned a mismatched source identity');
        }
        const validated = await this.validate({
          ...untrusted,
          cacheKey,
          rendererId: DIAGRAM_RENDERER_ID
        });
        this.assertCurrent(token);
        this.writeCache(cacheKey, validated);
        artifacts.push(validated);
      } catch (error) {
        if (error instanceof DiagramRenderError) throw error;
        this.assertCurrent(token);
        throw new DiagramRenderError(
          'DIAGRAM_RUNTIME_CRASHED',
          error instanceof Error ? error.message : 'Diagram runtime failed'
        );
      } finally {
        if (timeoutId !== null) clearTimeout(timeoutId);
        if (this.active?.token === token) this.active = null;
      }
    }
    this.assertCurrent(token);
    return { artifacts, diagnostics: [...diagnostics] };
  }

  cancel(): void {
    this.cancelActive('cancelled');
  }

  release(): void {
    if (this.disposed) return;
    this.cancelActive('cancelled');
    this.port.release?.();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.cancelActive('disposed');
    this.cache.clear();
    this.cacheBytes = 0;
    this.port.dispose?.();
  }

  cacheStats(): { entries: number; bytes: number } {
    return { entries: this.cache.size, bytes: this.cacheBytes };
  }

  private cancelActive(reason: 'cancelled' | 'timeout' | 'disposed'): void {
    this.token += 1;
    const active = this.active;
    this.active = null;
    active?.terminate(reason);
  }

  private assertCurrent(token: number): void {
    if (this.disposed || token !== this.token) {
      throw new DiagramRenderError('DIAGRAM_CANCELLED', 'Diagram task no longer owns the result');
    }
  }

  private readCache(key: string): RenderedDiagram | null {
    const entry = this.cache.get(key);
    if (!entry) return null;
    this.cache.delete(key);
    this.cache.set(key, entry);
    return entry.artifact;
  }

  private writeCache(key: string, artifact: RenderedDiagram): void {
    const bytes = new TextEncoder().encode(artifact.svgUtf8).byteLength;
    if (bytes > DIAGRAM_CACHE_BYTES) return;
    const previous = this.cache.get(key);
    if (previous) {
      this.cacheBytes -= previous.bytes;
      this.cache.delete(key);
    }
    this.cache.set(key, { artifact, bytes });
    this.cacheBytes += bytes;
    while (this.cache.size > DIAGRAM_CACHE_ENTRIES || this.cacheBytes > DIAGRAM_CACHE_BYTES) {
      const oldest = this.cache.keys().next().value as string | undefined;
      if (oldest === undefined) break;
      const removed = this.cache.get(oldest);
      this.cache.delete(oldest);
      this.cacheBytes -= removed?.bytes ?? 0;
    }
  }
}

function normalizeDiagramPresentation(presentation: DiagramPresentation): DiagramPresentation {
  const fontFamily = presentation.fontFamily.trim().replace(/["']/g, '');
  return {
    fontFamily: fontFamily && fontFamily.length <= 200 &&
      !/[\u0000-\u001f\u007f;{}<>\\()[\]:]/.test(fontFamily)
      ? fontFamily
      : 'Arial, system-ui, sans-serif',
    fontSize: Number.isFinite(presentation.fontSize)
      ? Math.min(28, Math.max(12, presentation.fontSize))
      : 16,
    lineHeight: Number.isFinite(presentation.lineHeight)
      ? Math.min(2.4, Math.max(1, presentation.lineHeight))
      : 1.6
  };
}

export async function diagramCacheKey(
  sourceSha256: string,
  options: DiagramRenderOptions
): Promise<string> {
  const bytes = new TextEncoder().encode(
    [options.rendererId, options.configVersion, options.theme, options.fontKey, sourceSha256].join('\0')
  );
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
