import { api } from './tauriApi';
import { DiagramRenderer, type DiagramPresentation } from './diagramRenderer';
import { MermaidSandboxPort } from './mermaidSandboxPort';
import type { DiagramDiagnostic, DiagramSource, DiagramTheme } from './platform/contracts';

function createRenderer() {
  return new DiagramRenderer(
    new MermaidSandboxPort(() => api.loadDiagramRuntime()),
    (diagram) => api.validateDiagramSvg(diagram)
  );
}

let renderer = createRenderer();

export const previewDiagramRuntime = {
  render(
    sources: readonly DiagramSource[],
    diagnostics: readonly DiagramDiagnostic[],
    theme: DiagramTheme,
    presentation: DiagramPresentation
  ) {
    return renderer.render(sources, diagnostics, theme, presentation);
  },
  cancel() {
    renderer.cancel();
  },
  release() {
    renderer.release();
  },
  reset() {
    renderer.dispose();
    renderer = createRenderer();
  },
  cacheStats() {
    return renderer.cacheStats();
  }
};
