import { createLatestTaskQueue } from './latestTaskQueue';
import type { MarkdownAnalysisDto, RenderedMarkdownDto } from './tauriApi';

export type MarkdownRenderRequest = {
  kind: 'render' | 'analysis';
  tabId: string;
  content: string;
  contentRevision: number;
};

type ActiveMarkdown = Omit<MarkdownRenderRequest, 'kind'>;

type MarkdownRenderOperations = {
  render: (content: string, tabId: string, contentRevision: number) => Promise<RenderedMarkdownDto>;
  analyze: (content: string) => Promise<MarkdownAnalysisDto>;
  onRendered: (request: MarkdownRenderRequest, result: RenderedMarkdownDto) => boolean;
  onAnalyzed: (request: MarkdownRenderRequest, result: MarkdownAnalysisDto) => boolean;
  onError: (request: MarkdownRenderRequest, error: unknown) => void;
};

export function createMarkdownRenderCoordinator(operations: MarkdownRenderOperations) {
  let timer: number | undefined;
  let lastIntent = '';
  let disposed = false;

  const queue = createLatestTaskQueue<MarkdownRenderRequest, boolean>(async (request) => {
    try {
      if (request.kind === 'render') {
        const result = await operations.render(request.content, request.tabId, request.contentRevision);
        return !disposed && operations.onRendered(request, result);
      }
      const result = await operations.analyze(request.content);
      return !disposed && operations.onAnalyzed(request, result);
    } catch (error) {
      if (!disposed) operations.onError(request, error);
      return false;
    }
  });

  function cancelTimer() {
    if (timer === undefined) return;
    window.clearTimeout(timer);
    timer = undefined;
  }

  return {
    update(active: ActiveMarkdown | null, kind: MarkdownRenderRequest['kind'], analysisCurrent: boolean, debounceMs: number) {
      if (disposed) return;
      if (!active) {
        cancelTimer();
        lastIntent = '';
        return;
      }
      const intent = `${kind}:${active.tabId}:${active.contentRevision}`;
      if (kind === 'analysis' && analysisCurrent) {
        cancelTimer();
        lastIntent = '';
        return;
      }
      if (intent === lastIntent) return;
      cancelTimer();
      lastIntent = intent;
      const request = { ...active, kind };
      timer = window.setTimeout(() => {
        timer = undefined;
        void queue.submit(request);
      }, Math.max(100, debounceMs));
    },
    async runNow(request: MarkdownRenderRequest): Promise<boolean> {
      const result = await queue.submit(request);
      return result.status === 'completed' && result.value;
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      cancelTimer();
      queue.dispose();
    }
  };
}
