<script lang="ts">
  import PreviewPane from '../components/editor/PreviewPane.svelte';
  import {
    defaultSettings,
    type AppSettings,
    type DiagramDiagnostic,
    type DiagramSource,
    type SanitizedMarkdownHtml
  } from '../lib/tauriApi';

  export let html: SanitizedMarkdownHtml;
  export let documentPath: string;
  export let initialAllowLocalImages = false;
  export let alternateHtml: SanitizedMarkdownHtml | null = null;
  export let alternateDocumentPath = '';
  export let diagrams: DiagramSource[] = [];
  export let diagramDiagnostics: DiagramDiagnostic[] = [];
  export let alternateDiagrams: DiagramSource[] = [];
  export let alternateDiagramDiagnostics: DiagramDiagnostic[] = [];

  let settings: AppSettings = {
    ...defaultSettings,
    allowLocalImages: initialAllowLocalImages
  };
  let currentHtml = html;
  let currentDocumentPath = documentPath;
  let currentDiagrams = diagrams;
  let currentDiagramDiagnostics = diagramDiagnostics;
  let revision = 0;

  function enableImages() {
    settings = { ...settings, allowLocalImages: true };
  }

  function disableImages() {
    settings = { ...settings, allowLocalImages: false };
  }

  function refreshSettings() {
    settings = { ...settings };
  }

  function useDarkTheme() {
    settings = { ...settings, theme: 'dark' };
  }

  function increasePreviewFont() {
    settings = { ...settings, previewFontSize: settings.previewFontSize + 1 };
  }

  function switchHtml() {
    if (alternateHtml !== null) {
      currentHtml = alternateHtml;
      currentDiagrams = alternateDiagrams;
      currentDiagramDiagnostics = alternateDiagramDiagnostics;
      revision += 1;
    }
  }

  function switchDocumentPath() {
    if (alternateDocumentPath) currentDocumentPath = alternateDocumentPath;
  }

  async function openDocumentForHarness() {
    return true;
  }
</script>

<button type="button" data-testid="enable-images" on:click={enableImages}>Enable images</button>
<button type="button" data-testid="disable-images" on:click={disableImages}>Disable images</button>
<button type="button" data-testid="refresh-settings" on:click={refreshSettings}>
  Refresh settings
</button>
<button type="button" data-testid="dark-theme" on:click={useDarkTheme}>Dark theme</button>
<button type="button" data-testid="increase-preview-font" on:click={increasePreviewFont}>Increase preview font</button>
<button type="button" data-testid="switch-html" on:click={switchHtml}>Switch HTML</button>
<button type="button" data-testid="switch-document" on:click={switchDocumentPath}>
  Switch document
</button>
<PreviewPane
  html={currentHtml}
  tabId="harness-tab"
  contentRevision={revision}
  renderedRevision={revision}
  diagrams={currentDiagrams}
  diagramDiagnostics={currentDiagramDiagnostics}
  documentPath={currentDocumentPath}
  documentTitle="Harness.md"
  outline={[]}
  {settings}
  onOpenDocument={openDocumentForHarness}
  onJumpToLine={() => undefined}
/>
